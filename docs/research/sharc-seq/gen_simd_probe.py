"""SIMD (PEYEN) compute probe (RND32 set, TRUNC clear, like the C runtime) for the ADSP-21489 simulator: float and fixed multiply, add, subtract,
multiply-accumulate and the dual-compute forms on both processing elements, with ASTAT.

usage: python gen_simd_probe.py <outdir>      -> <outdir>/simd.asm, simd.c, simd.json
Operands come as pairs (PEx word, PEy word) loaded with one SIMD load (`r1 = dm(i1, m5)` fills R1
from the address and S1 from the next word). Every result and ASTAT is stored as a pair.
"""
import json
import os
import struct
import sys


def fb(x):
    return struct.unpack('<I', struct.pack('<f', x))[0]


# (PEx, PEy) operand pairs: ordinary, tie-prone, extreme and special values
AV = [(fb(1.5), fb(-2.25)), (fb(1.0 + 2 ** -23), fb(1.0 - 2 ** -24)), (fb(3.0), fb(3.0)), (0x7F7FFFFF, 0x00800000),
      (0x7F800000, 0x7FC00000), (fb(0.0), fb(-0.0)), (0x40000000, 0xC0000000), (0x12345678, 0x9ABCDEF0), (0xFFC00000, 0xFFFFFFFF)]
BV = [(fb(2.0), fb(0.5)), (fb(1.0 + 2 ** -12), fb(1.0 + 2 ** -20)), (fb(-3.0), fb(3.0)), (0x40000000, 0x80000000),
      (0x00000001, 0xFFFFFFFF), (fb(1e20), fb(1e-20)), (0x3FFFFFFF, 0x7FFFFFFF), (0x0000FFFF, 0xFFFF0000)]

OPS = [
    ('fadd', ['f4 = f1 + f5;']),
    ('fsub', ['f4 = f1 - f5;']),
    ('fmul', ['f4 = f1 * f5;']),
    ('favg', ['f4 = (f1 + f5)/2;']),
    ('mul_ssf', ['r4 = r1 * r5 (ssf);']),
    ('mul_ssfr', ['r4 = r1 * r5 (ssfr);']),
    ('mul_ssi', ['r4 = r1 * r5 (ssi);']),
    ('mac_ssf', ['mrf = 0;', 'mrf = mrf + r1 * r5 (ssf);', 'mrf = mrf + r1 * r5 (ssf);', 'r4 = mr1f;', 'r6 = mr0f;']),
    ('msub_ssfr', ['mrf = 0;', 'mrf = mrf - r1 * r5 (ssfr);', 'r4 = mr1f;', 'r6 = mr0f;']),
    ('dual_fmul_fadd', ['f4 = f1 * f5, f8 = f9 + f13;']),
    ('dual_mul_add', ['r4 = r1 * r5 (ssfr), r8 = r9 + r13;']),
    ('dual_addsub', ['r4 = r9 + r13, r8 = r9 - r13;']),
    ('dual_fsubadd', ['f4 = f9 + f13, f8 = f9 - f13;']),
    ('mac_dual', ['mrf = 0;', 'r4 = mrf + r1 * r5 (ssfr), r8 = r9 + r13;']),
    ('cond_add', ['comp(r1, r5);', 'if lt r4 = r1 + r5;']),
]


def main(outdir):
    asm = ['/* SIMD probe for ADSP-21489 (see ../sharc-simd.md) */', '#include <asm_sprt.h>', '.extern _out;',
           '.section/dm seg_dmda;', '.global _va;', '.global _vb;',
           '.var _va[%d] = %s;' % (2 * len(AV), ', '.join('0x%08X' % x for p in AV for x in p)),
           '.var _vb[%d] = %s;' % (2 * len(BV), ', '.join('0x%08X' % x for p in BV for x in p)),
           '.section/pm seg_pmco;', '.global _probe;', '.global _probe_exit;', '_probe:', '    leaf_entry;',
           '    i2 = _out;', '    m5 = 2;', '    bit clr mode1 0x8000;', '    bit set mode1 0x210000;', '    nop;', '    nop;', '    i1 = _va;',
           '    lcntr = %d, do a_loop until lce;' % len(AV), '        r1 = dm(i1, m5);', '        i3 = _vb;',
           '        lcntr = %d, do b_loop until lce;' % len(BV), '            r5 = dm(i3, m5);',
           '            r9 = r1;', '            r13 = r5;']
    n = 0
    names = []
    for name, ops in OPS:
        asm += ['            r4 = 0;', '            r8 = 0;', '            r6 = 0;', '            astat = r6;'] + \
               ['            ' + o for o in ops]
        asm += ['            r7 = astat;', '            dm(i2, m5) = r4;', '            dm(i2, m5) = r8;',
                '            dm(i2, m5) = r6;', '            dm(i2, m5) = r7;']
        names += [name + '.r4', name + '.r8', name + '.r6', name + '.astat']
        n += 8
    asm += ['b_loop:         nop;', 'a_loop:     nop;', '    bit clr mode1 0x200000;', '    nop;', '    nop;',
            '    r0 = 0xD0E5;', '    i2 = _out + %d;' % (len(AV) * len(BV) * n), '    m5 = 1;', '    dm(i2, m5) = r0;',
            '_probe_exit:', '    leaf_exit;', '._probe.end:']
    total = len(AV) * len(BV) * n + 1
    os.makedirs(outdir, exist_ok=True)
    open(os.path.join(outdir, 'simd.asm'), 'w').write('\n'.join(asm) + '\n')
    open(os.path.join(outdir, 'simd.c'), 'w').write(
        '/* Runs on the CCES ADSP-21489 simulator; results are saved from the Memory Browser. */\n'
        '#define NOUT %d\nextern void probe(unsigned *out);\nunsigned out[NOUT];\nvolatile unsigned done;\n'
        'int main(void) { int i; for (i = 0; i < NOUT; i++) out[i] = 0xA5A5A5A5u; probe(out); done = 0xD0E5; '
        'return 0; }\n' % total)
    json.dump(dict(n_words=total, per_iter=n, names=names, a=AV, b=BV), open(os.path.join(outdir, 'simd.json'), 'w'))
    print(total)


if __name__ == '__main__':
    main(sys.argv[1])
