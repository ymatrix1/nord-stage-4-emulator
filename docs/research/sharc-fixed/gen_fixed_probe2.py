"""Second set of fixed-point probes: shifter OR forms, FIX/TRUNC underflow and sticky bits, the
background accumulator MRB, integer-rounding multiplier forms, and the MODE1 SRCU swap.

usage: python gen_fixed_probe2.py <build> <outdir>   (needs easm21k/elfdump on PATH)
builds: or fix0 fix1 mrb srcu -> <outdir>/<build>.asm, .c, .json   (see README.md, build_csv2.py)
"""
import json
import os
import sys

from gen_fixed_probe import FY, MRP, SH, V7, V32, mrpre, words

OUT = {'r0': 'r0', 'mr0': 'r6', 'mr1': 'r7', 'mr2': 'r8', 'b0': 'r3', 'b1': 'r4', 'b2': 'r5', 'astat': 'r10',
       'stky': 'r11'}
OUTS3 = ['r0', 'astat', 'stky']
OUTS9 = ['r0', 'mr0', 'mr1', 'mr2', 'b0', 'b1', 'b2', 'astat', 'stky']
FL = [0x3E800000, 0x3F000000, 0x3F400000, 0x3F800000, 0x3FC00000, 0xBE800000, 0xBF000000, 0xBF800000,
      0x4E800000, 0x4F000000, 0xCF000000, 0xCF000100, 0x4F32D05E, 0x00400000, 0x12345678, 0x00800000,
      0x7FC00000, 0x7F800000, 0xFF800000, 0x3F7FFFFF, 0x80000000, 0x3F000001]
SC = [0, 1, -1, 4, -4, 40, -40]
PF = (0, 0x12345678, 0x9ABCDEF0)        # MRF preset
PB = (0x7FFF, 0xFFFFFFFF, 0xFFFFFFFF)   # MRB preset (near the top of the 80-bit range)
MRDB = ['r0 = sat mrb (si);', 'r0 = sat mrb (ui);', 'r0 = sat mrb (sf);', 'r0 = sat mrb (uf);',
        'mrb = sat mrb (si);', 'mrb = sat mrb (uf);', 'r0 = rnd mrb (sf);', 'r0 = rnd mrb (uf);',
        'mrb = rnd mrb (sf);', 'r0 = mr0b;', 'r0 = mr1b;', 'r0 = mr2b;', 'mr0b = r1;', 'mr1b = r1;', 'mr2b = r1;',
        'mrb = 0;']
TOGGLE_ON = ['bit set mode1 0x4;', 'nop;', 'nop;']
TOGGLE_OFF = ['bit clr mode1 0x4;', 'nop;', 'nop;']


def mrpre_b(p):
    return ['r3 = 0x%08X;' % p[2], 'r4 = 0x%08X;' % p[1], 'r5 = 0x%08X;' % p[0],
            'mr0b = r3;', 'mr1b = r4;', 'mr2b = r5;']


def rec_asm(pre, op, outs, indent, pre2=(), post=()):
    lines = ['r15 = 0;', 'astat = r15;', 'stky = r15;'] + list(pre) + ['r0 = 0;'] + list(pre2) + \
            ['r15 = 0;', 'stky = r15;', op, 'r10 = astat;', 'r11 = stky;']
    if any(o.startswith('mr') for o in outs):
        lines += ['r6 = mr0f;', 'r7 = mr1f;', 'r8 = mr2f;']
    if 'b0' in outs:
        lines += ['r3 = mr0b;', 'r4 = mr1b;', 'r5 = mr2b;']
    lines += ['dm(i0, m5) = %s;' % OUT[o] for o in outs] + list(post)
    return ''.join(indent + l + '\n' for l in lines)


def build(name):
    """Returns (mode1, sections); a section is dict(kind, A, B, recs)."""
    if name == 'or':
        recs = [dict(name='orlshift', op='r0 = r0 or lshift r1 by r2;', ysel='sh'),
                dict(name='orashift', op='r0 = r0 or ashift r1 by r2;', ysel='sh'),
                dict(name='ordep', op='r0 = r0 or fdep r1 by r2;', ysel='fy'),
                dict(name='ordepse', op='r0 = r0 or fdep r1 by r2 (se);', ysel='fy')]
        for r in recs:
            r.update(pre=[], pre2=['r0 = 0x0F0F0F0F;'], outs=OUTS3)
        return 0, [dict(kind='ytable', A=V32, B=None, recs=recs)]
    if name in ('fix0', 'fix1'):
        ops = [('fix', 'r0 = fix f1;'), ('trunc', 'r0 = trunc f1;'), ('fixby', 'r0 = fix f1 by r2;'),
               ('truncby', 'r0 = trunc f1 by r2;')]
        recs = [dict(name=n, op=o, pre=[], outs=OUTS3) for n, o in ops]
        return ((1 << 13) if name == 'fix1' else 0), [dict(kind='pairs', A=FL, B=[s & 0xFFFFFFFF for s in SC],
                                                            recs=recs)]
    if name == 'mrb':
        recs = []
        for fmt in ('ssf', 'ssi', 'ssir', 'uuir'):
            pre = mrpre(PF) + mrpre_b(PB)
            for k, op in (('mulf', 'mrf = r1 * r2 (FMT);'), ('mulb', 'mrb = r1 * r2 (FMT);'),
                          ('macb', 'mrb = mrb + r1 * r2 (FMT);'), ('rmacb', 'r0 = mrb + r1 * r2 (FMT);'),
                          ('rmacf', 'r0 = mrf + r1 * r2 (FMT);')):
                recs.append(dict(name='%s_%s' % (k, fmt), op=op.replace('FMT', fmt), pre=pre, outs=OUTS9,
                                 mrp=PF, mrq=PB))
        data = [dict(name='mrdb', op=op, pre=['r6 = 0;', 'mr0f = r6;', 'mr1f = r6;', 'mr2f = r6;'], outs=OUTS9,
                     mrp=(0, 0, 0), mrq='preset') for op in MRDB]
        return 0, [dict(kind='pairs', A=V7, B=V7, recs=recs), dict(kind='preset', A=None, B=None, recs=data)]
    if name == 'srcu':
        op_f, op_b, op_r = 'mrf = mrf + r1 * r2 (ssf);', 'mrb = mrb + r1 * r2 (ssf);', 'r0 = mrf + r1 * r2 (ssf);'
        pres = mrpre(PF) + mrpre_b(PB)
        recs = [dict(name='srcu_a', op=op_f, pre=pres + TOGGLE_ON, post=TOGGLE_OFF),
                dict(name='srcu_b', op=op_f, pre=TOGGLE_ON + pres, post=TOGGLE_OFF),
                dict(name='srcu_c', op=op_b, pre=TOGGLE_ON + pres, post=TOGGLE_OFF),
                dict(name='srcu_d', op=op_r, pre=TOGGLE_ON + pres, post=TOGGLE_OFF)]
        for r in recs:
            r.update(outs=OUTS9, mrp=PF, mrq=PB)
        return 0, [dict(kind='pairs', A=V7, B=V7, recs=recs)]
    raise SystemExit('unknown build ' + name)


def section_code(idx, sec, n_words):
    """Asm of one section; returns (code, words)."""
    recs, kind = sec['recs'], sec['kind']
    if kind == 'pairs':
        body = ''.join(rec_asm(r['pre'], r['op'], r['outs'], '                ', r.get('pre2', ()), r.get('post', ()))
                       for r in recs)
        n = sum(len(r['outs']) for r in recs) * len(sec['A']) * len(sec['B'])
        return ('        i1 = _a%d;\n        lcntr = %d, do a%d_loop until lce;\n            r1 = dm(i1, m5);\n'
                '            i3 = _b%d;\n            lcntr = %d, do b%d_loop until lce;\n                r2 = dm(i3, m5);\n'
                '%sb%d_loop:         nop;\na%d_loop:     nop;\n' %
                (idx, len(sec['A']), idx, idx, len(sec['B']), idx, body, idx, idx)), n
    if kind == 'ytable':
        parts, n = '', 0
        for i, r in enumerate(recs):
            tbl, cnt = ('_ysh', len(SH)) if r['ysel'] == 'sh' else ('_yfy', len(FY))
            r['ycount'] = cnt
            n += cnt * len(r['outs']) * len(sec['A'])
            parts += ('            i3 = %s;\n            lcntr = %d, do y%d_%d_loop until lce;\n'
                      '                r2 = dm(i3, m5);\n%sy%d_%d_loop:     nop;\n' %
                      (tbl, cnt, idx, i, rec_asm(r['pre'], r['op'], r['outs'], '                ', r.get('pre2', ()),
                                                 r.get('post', ())), idx, i))
        return ('        i1 = _a%d;\n        lcntr = %d, do a%d_loop until lce;\n            r1 = dm(i1, m5);\n'
                '%sa%d_loop:     nop;\n' % (idx, len(sec['A']), idx, parts, idx)), n
    # preset: loop over the 12 accumulator presets, MRB preset loaded from the table
    body = ''
    for r in recs:
        pre = ['r3 = dm(i4, m5);', 'r4 = dm(i4, m5);', 'r5 = dm(i4, m5);', 'modify(i4, -3);',
               'mr0b = r3;', 'mr1b = r4;', 'mr2b = r5;'] + r['pre']
        body += rec_asm(pre, r['op'], r['outs'], '            ', r.get('pre2', ()), r.get('post', ()))
    n = sum(len(r['outs']) for r in recs) * len(MRP)
    return ('        i4 = _mrp;\n        lcntr = %d, do p%d_loop until lce;\n            r1 = 0x7FFFFFF1;\n%s'
            '            modify(i4, 3);\np%d_loop:     nop;\n' % (len(MRP), idx, body, idx)), n


def emit(name, outdir):
    mode, sections = build(name)
    allrecs = [r for s in sections for r in s['recs']]
    for r, (w, short) in zip(allrecs, words([r['op'] for r in allrecs])):
        r['word'], r['short'] = w, short
    data, code, n_words = '', '', 0
    for i, s in enumerate(sections):
        if s['A'] is not None:
            data += '.var _a%d[%d] = %s;\n' % (i, len(s['A']), ', '.join('0x%08X' % x for x in s['A']))
        if s['B'] is not None:
            data += '.var _b%d[%d] = %s;\n' % (i, len(s['B']), ', '.join('0x%08X' % x for x in s['B']))
        c, n = section_code(i, s, n_words)
        code += c
        n_words += n
    asm = ('/* fixed-point probe %s (second set) for ADSP-21489 (see ../sharc-fixed.md) */\n#include <asm_sprt.h>\n'
           '.section/dm seg_dmda;\n%s.var _ysh[%d] = %s;\n.var _yfy[%d] = %s;\n.var _mrp[%d] = %s;\n'
           '.global _probe;\n.section/pm seg_pmco;\n_probe:\n    leaf_entry;\n    i0 = r4;\n    m5 = 1;\n'
           '    r14 = 1;\n    r12 = mode1;\n    r13 = 0x1A000;\n    r13 = not r13;\n    r12 = r12 and r13;\n'
           '    r13 = 0x%X;\n    r13 = r12 or r13;\n    mode1 = r13;\n    nop; nop;\n%s'
           '    mode1 = r12;\n    nop; nop;\n    leaf_exit;\n._probe.end:\n' %
           (name, data, len(SH), ', '.join(str(x) for x in SH), len(FY), ', '.join('0x%04X' % x for x in FY),
            3 * len(MRP), ', '.join('0x%08X' % x for p in MRP for x in (p[2], p[1], p[0])), mode, code))
    os.makedirs(outdir, exist_ok=True)
    with open(os.path.join(outdir, name + '.asm'), 'w') as f:
        f.write(asm)
    with open(os.path.join(outdir, name + '.c'), 'w') as f:
        f.write('/* Runs on the CCES ADSP-21489 simulator; results are saved from the Memory Browser. */\n'
                '#define NOUT %d\nextern void probe(unsigned *out);\nunsigned out[NOUT];\nvolatile unsigned done;\n'
                'int main(void) { probe(out); done = 0xD0E5; return 0; }\n' % n_words)
    keep = ('name', 'op', 'word', 'short', 'outs', 'ysel', 'ycount', 'mrp', 'mrq')
    layout = dict(build=name, mode1=mode, n_words=n_words, mrp=MRP, sh=SH, fy=FY,
                  sections=[dict(kind=s['kind'], A=s['A'], B=s['B'],
                                 recs=[{k: v for k, v in r.items() if k in keep} for r in s['recs']])
                            for s in sections])
    json.dump(layout, open(os.path.join(outdir, name + '.json'), 'w'))
    print(name, n_words)


if __name__ == '__main__':
    emit(sys.argv[1], sys.argv[2])
