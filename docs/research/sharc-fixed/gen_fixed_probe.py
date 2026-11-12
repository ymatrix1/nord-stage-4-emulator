"""Generator of the fixed-point probes (ALU, multiplier/MR, shifter) for the ADSP-21489 simulator.

usage: python gen_fixed_probe.py <build> <outdir>   (needs easm21k/elfdump on PATH)
builds: alu0 alu1 mul0 mul1 mul2 shift -> <outdir>/<build>.asm, .c, .json
The json holds, per record, the 23-bit compute word (assembled and read back with elfdump) so the
Rust test can execute exactly the instruction the simulator ran.
"""
import json
import os
import re
import subprocess
import sys
import tempfile

OUT = {'r0': 'r0', 'mr0': 'r6', 'mr1': 'r7', 'mr2': 'r8', 'astat': 'r10', 'stky': 'r11'}
V32 = [0x00000000, 0x00000001, 0xFFFFFFFF, 0x00000002, 0x7FFFFFFF, 0x80000000, 0x80000001, 0xFFFF0000,
       0x0000FFFF, 0x12345678, 0x40000000, 0xC0000000]
V7 = [0x00000000, 0x00000001, 0xFFFFFFFF, 0x7FFFFFFF, 0x80000000, 0x40000000, 0x00008000]
SH = [0, 1, -1, 4, -4, 8, -8, 31, -31, 32, -32, 40]
FY = [0x0000, 0x0004, 0x0008, 0x0108, 0x0188, 0x0210, 0x0420, 0x0820, 0x0A04, 0x1F08, 0x1008, 0xFFFF]
MRP = [(0, 0, 0), (0, 0, 1), (0, 0x7FFFFFFF, 0xFFFFFFFF), (0, 0x80000000, 0), (0, 0xFFFFFFFF, 0xFFFFFFFF),
       (0, 1, 0), (0x7FFF, 0xFFFFFFFF, 0xFFFFFFFF), (0x8000, 0, 0), (0xFFFF, 0xFFFFFFFF, 0xFFFFFFFF),
       (0xFFFF, 0x80000000, 0), (0x1234, 0x89ABCDEF, 0x76543210), (0, 0x00008000, 0x80000000)]
FMTS = ['ssi', 'sui', 'usi', 'uui', 'ssf', 'suf', 'usf', 'uuf', 'ssfr', 'sufr', 'usfr', 'uufr']
MULP = [(0x7FFF, 0xFFFFFFFF, 0xFFFFFFFF), (0x8000, 0, 0), (0, 0x00001234, 0x56789ABC)]

ALU = [('add', 'r0 = r1 + r2;', None), ('sub', 'r0 = r1 - r2;', None),
       ('addci0', 'r0 = r1 + r2 + ci;', 0), ('addci1', 'r0 = r1 + r2 + ci;', 1),
       ('subci0', 'r0 = r1 - r2 + ci - 1;', 0), ('subci1', 'r0 = r1 - r2 + ci - 1;', 1),
       ('avg', 'r0 = (r1 + r2)/2;', None), ('comp', 'comp(r1, r2);', None), ('compu', 'compu(r1, r2);', None),
       ('pass', 'r0 = pass r1;', None), ('neg', 'r0 = -r1;', None), ('abs', 'r0 = abs r1;', None),
       ('min', 'r0 = min(r1, r2);', None), ('max', 'r0 = max(r1, r2);', None), ('clip', 'r0 = clip r1 by r2;', None),
       ('inc', 'r0 = r1 + 1;', None), ('dec', 'r0 = r1 - 1;', None), ('not', 'r0 = not r1;', None),
       ('and', 'r0 = r1 and r2;', None), ('or', 'r0 = r1 or r2;', None), ('xor', 'r0 = r1 xor r2;', None),
       ('fix', 'r0 = fix f1;', None), ('trunc', 'r0 = trunc f1;', None), ('float', 'f0 = float r1;', None),
       ('fixby', 'r0 = fix f1 by r2;', None), ('floatby', 'f0 = float r1 by r2;', None)]
SHIFT = [('lshift', 'r0 = lshift r1 by r2;'), ('ashift', 'r0 = ashift r1 by r2;'), ('rot', 'r0 = rot r1 by r2;'),
         ('orlshift', 'r0 = r0 or lshift r1 by r2;'), ('orashift', 'r0 = r0 or ashift r1 by r2;'),
         ('bclr', 'r0 = bclr r1 by r2;'), ('bset', 'r0 = bset r1 by r2;'), ('btgl', 'r0 = btgl r1 by r2;'),
         ('btst', 'btst r1 by r2;'), ('exp', 'r0 = exp r1;'), ('expex', 'r0 = exp r1 (ex);'),
         ('leftz', 'r0 = leftz r1;'), ('lefto', 'r0 = lefto r1;'), ('fpack', 'r0 = fpack f1;'),
         ('funpack', 'f0 = funpack r1;')]
FIELD = [('fdep', 'r0 = fdep r1 by r2;'), ('fdepse', 'r0 = fdep r1 by r2 (se);'),
         ('ordep', 'r0 = r0 or fdep r1 by r2;'), ('fext', 'r0 = fext r1 by r2;'),
         ('fextse', 'r0 = fext r1 by r2 (se);')]
MRD = ['r0 = sat mrf (si);', 'r0 = sat mrf (ui);', 'r0 = sat mrf (sf);', 'r0 = sat mrf (uf);',
       'mrf = sat mrf (si);', 'mrf = sat mrf (ui);', 'mrf = sat mrf (sf);', 'mrf = sat mrf (uf);',
       'r0 = rnd mrf (sf);', 'r0 = rnd mrf (uf);', 'mrf = rnd mrf (sf);', 'mrf = rnd mrf (uf);',
       'r0 = mr0f;', 'r0 = mr1f;', 'r0 = mr2f;', 'mr0f = r1;', 'mr1f = r1;', 'mr2f = r1;', 'mrf = 0;']
OUTS3 = ['r0', 'astat', 'stky']
OUTS6 = ['r0', 'mr0', 'mr1', 'mr2', 'astat', 'stky']


def words(lines):
    """Assemble each op line and return the 23-bit compute word of each."""
    with tempfile.TemporaryDirectory() as d:
        src = os.path.join(d, 't.asm')
        with open(src, 'w') as f:
            f.write('.section/pm seg_pmco;\n' + '\n'.join(lines) + '\n')
        subprocess.run(['easm21k', '-proc', 'ADSP-21489', '-o', os.path.join(d, 't.doj'), src], check=True,
                       capture_output=True)
        txt = subprocess.run(['elfdump', '-n', 'seg_pmco', os.path.join(d, 't.doj')], check=True,
                             capture_output=True, text=True).stdout
    out = []
    for m in re.finditer(r'^[0-9a-f]{8}\s+((?:[0-9a-f]{4} )+)', txt, re.M):
        parcels = m.group(1).split()
        v = int(''.join(parcels), 16)
        out.append((v & 0x7FFFFF, 0) if len(parcels) == 2 else (v, 1) if len(parcels) == 1 else (None, 0))
    assert len(out) == len(lines), (len(out), len(lines))
    return out


def mrpre(p):
    return ['r3 = 0x%08X;' % p[2], 'r4 = 0x%08X;' % p[1], 'r5 = 0x%08X;' % p[0],
            'mr0f = r3;', 'mr1f = r4;', 'mr2f = r5;']


def rec_asm(pre, op, outs, indent):
    lines = ['r15 = 0;', 'astat = r15;', 'stky = r15;'] + pre + ['r0 = 0;', 'r15 = 0;', 'stky = r15;', op,
                                                                  'r10 = astat;', 'r11 = stky;']
    if any(o.startswith('mr') for o in outs):
        lines += ['r6 = mr0f;', 'r7 = mr1f;', 'r8 = mr2f;']
    lines += ['dm(i0, m5) = %s;' % OUT[o] for o in outs]
    return ''.join(indent + l + '\n' for l in lines)


def build(name):
    mode, recs = 0, []
    if name.startswith('alu'):
        mode = (1 << 13) if name == 'alu1' else 0
        for n, op, ci in ALU:
            pre = ['r12 = r14 + r14;']                       # AC = 0 and the other ALU flags clear
            if ci:
                pre = ['r13 = 0xFFFFFFFF;', 'r12 = r13 + r14;']  # carry out: AC = 1
            recs.append(dict(name=n, op=op, ci=ci, outs=OUTS3, pre=pre))
        return mode, recs, ('pairs', V32, V32)
    if name.startswith('mul'):
        grp = int(name[3:])
        for fmt in FMTS[grp * 4:grp * 4 + 4]:
            def f(s):
                return s.replace('FMT', fmt)
            recs.append(dict(name='mul_' + fmt, op=f('mrf = r1 * r2 (FMT);'), pre=[], mrp=None, outs=OUTS6))
            recs.append(dict(name='rmul_' + fmt, op=f('r0 = r1 * r2 (FMT);'), pre=[], mrp=None, outs=OUTS6))
            for k, p in enumerate(MULP[:2]):
                recs.append(dict(name='mac%d_%s' % (k, fmt), op=f('mrf = mrf + r1 * r2 (FMT);'), pre=mrpre(p),
                                 mrp=p, outs=OUTS6))
                recs.append(dict(name='msub%d_%s' % (k, fmt), op=f('mrf = mrf - r1 * r2 (FMT);'), pre=mrpre(p),
                                 mrp=p, outs=OUTS6))
            recs.append(dict(name='rmac_' + fmt, op=f('r0 = mrf + r1 * r2 (FMT);'), pre=mrpre(MULP[2]),
                             mrp=MULP[2], outs=OUTS6))
        return mode, recs, ('pairs', V7, V7)
    for n, op in SHIFT:
        recs.append(dict(name=n, op=op, pre=['r0 = 0x0F0F0F0F;'] if n.startswith('or') else [], outs=OUTS3,
                         ysel='sh'))
    for n, op in FIELD:
        recs.append(dict(name=n, op=op, pre=['r0 = 0x0F0F0F0F;'] if n.startswith('or') else [], outs=OUTS3,
                         ysel='fy'))
    return mode, recs, ('shift', V32, None)


def emit(name, outdir):
    mode, recs, loops = build(name)
    mrrecs = []
    if loops[0] == 'shift':
        mrrecs = [dict(name='mrd', op=op, pre=[], mrp='preset', outs=OUTS6, ysel=None) for op in MRD]
    allrecs = recs + mrrecs
    for r, (w, short) in zip(allrecs, words([r['op'] for r in allrecs])):
        r['word'], r['short'] = w, short
    n_words = 0
    if loops[0] == 'pairs':
        A, B = loops[1], loops[2]
        body = ''.join(rec_asm(r['pre'], r['op'], r['outs'], '                ') for r in recs)
        n_words = sum(len(r['outs']) for r in recs) * len(A) * len(B)
        data = ('.var _va[%d] = %s;\n.var _vb[%d] = %s;\n' %
                (len(A), ', '.join('0x%08X' % x for x in A), len(B), ', '.join('0x%08X' % x for x in B)))
        code = ('        i1 = _va;\n        lcntr = %d, do a_loop until lce;\n            r1 = dm(i1, m5);\n'
                '            i3 = _vb;\n            lcntr = %d, do b_loop until lce;\n                r2 = dm(i3, m5);\n'
                '%sb_loop:         nop;\na_loop:     nop;\n' % (len(A), len(B), body))
    else:
        A = loops[1]
        parts = ''
        for i, r in enumerate(recs):
            tbl, cnt = ('_ysh', len(SH)) if r['ysel'] == 'sh' else ('_yfy', len(FY))
            r['ycount'] = cnt
            n_words += cnt * len(r['outs']) * len(A)
            parts += ('            i3 = %s;\n            lcntr = %d, do y%d_loop until lce;\n'
                      '                r2 = dm(i3, m5);\n%sy%d_loop:     nop;\n' %
                      (tbl, cnt, i, rec_asm(r['pre'], r['op'], r['outs'], '                '), i))
        mr = ''
        for r in mrrecs:
            pre = ['r3 = dm(i4, m5);', 'r4 = dm(i4, m5);', 'r5 = dm(i4, m5);', 'modify(i4, -3);',
                   'mr0f = r3;', 'mr1f = r4;', 'mr2f = r5;']
            mr += rec_asm(pre, r['op'], r['outs'], '            ')
            n_words += 6 * len(MRP)
        data = '.var _va[%d] = %s;\n' % (len(A), ', '.join('0x%08X' % x for x in A))
        code = ('        i1 = _va;\n        lcntr = %d, do a_loop until lce;\n            r1 = dm(i1, m5);\n'
                '%sa_loop:     nop;\n        i4 = _mrp;\n        lcntr = %d, do p_loop until lce;\n'
                '            r1 = 0x7FFFFFF1;\n%s            modify(i4, 3);\np_loop:     nop;\n' %
                (len(A), parts, len(MRP), mr))
    asm = ('/* fixed-point probe %s for ADSP-21489 (see ../sharc-float.md) */\n#include <asm_sprt.h>\n'
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
    keep = ('name', 'op', 'word', 'short', 'outs', 'ci', 'mrp', 'ysel', 'ycount')
    json.dump(dict(build=name, mode1=mode, n_words=n_words, loops=loops[0], a=loops[1], b=loops[2], sh=SH, fy=FY,
                   mrp=MRP, recs=[{k: v for k, v in r.items() if k in keep} for r in allrecs]),
              open(os.path.join(outdir, name + '.json'), 'w'))
    print(name, n_words)


if __name__ == '__main__':
    emit(sys.argv[1], sys.argv[2])
