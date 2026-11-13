"""Single-slot probe for the firmware instruction census diff (see ../sharc-census-diff.md).

usage: python gen_census_probe.py <outdir>   -> <outdir>/cens.asm, cens.c, cens.json

The probe loads a complete core state from the DM table `_st` (R, S, I, M, L, B, MODE1, ASTAT x/y,
STKY x/y, MR), runs ONE patchable instruction slot (`_slot`, 3 parcels + 3 nops), then dumps the
state to `_res`. The test instruction is poked into the slot by `sim_batch.py` (simulator) and by the
`sharc_probe` binary (model), so both execute the firmware's own machine code.
"""
import json
import os
import sys

# state layout (words): name -> count
# B before I: loading Bx also loads Ix (VERIFIED by the census diff), so I is loaded after B
REGS = [('r', 16), ('s', 16), ('b', 16), ('l', 16), ('i', 16), ('m', 16)]
SYS = ['mode1', 'astat', 'astaty', 'stky', 'stkyy', 'mr0f', 'mr1f', 'mr2f', 'mr0b', 'mr1b', 'mr2b']
N_STATE = sum(n for _, n in REGS) + len(SYS)


def main(outdir):
    names = []
    for p, n in REGS:
        names += ['%s%d' % (p, k) for k in range(n)]
    names += SYS
    asm = ['/* census-diff probe */', '#include <asm_sprt.h>', '.section/dm seg_dmda;', '.global _st;', '.global _res;',
           '.global _scr;', '.global _done;', '.var _done;', '.var _st[%d];' % N_STATE, '.var _res[%d];' % N_STATE, '.var _scr[512];',
           '.section/pm seg_pmco;', '.global _probe;', '.global _slot;', '.global _probe_exit;', '_probe:',
           '    leaf_entry;', '    bit clr mode1 0x200000;', '    nop;', '    nop;']
    k = 0
    # data and index registers, scalar (SIMD off): S registers are loaded as ordinary universal registers
    for p, n in REGS:
        for j in range(n):
            asm.append('    %s%d = dm(_st+%d);' % (p, j, k))
            k += 1
    # system registers from the table (R15 is free once the state is loaded: save it first)
    # MODE1 last so SIMD/rounding take effect at the slot
    asm += ['    r0 = dm(_st+%d);' % k, '    astat = r0;']  # placeholder, replaced below
    asm = asm[:-2]
    base = k
    sysidx = {s: base + i for i, s in enumerate(SYS)}
    # load R0 temporarily for the system registers, restore R0 afterwards from the table
    asm += ['    r0 = dm(_st+%d);' % sysidx['astat'], '    astat = r0;',
            '    r0 = dm(_st+%d);' % sysidx['astaty'], '    astaty = r0;',
            '    r0 = dm(_st+%d);' % sysidx['stky'], '    stky = r0;',
            '    r0 = dm(_st+%d);' % sysidx['stkyy'], '    stkyy = r0;']
    for s in ['mr0f', 'mr1f', 'mr2f', 'mr0b', 'mr1b', 'mr2b']:
        asm += ['    r0 = dm(_st+%d);' % sysidx[s], '    %s = r0;' % s]
    asm += ['    r0 = dm(_st+%d);' % sysidx['mode1'], '    mode1 = r0;', '    nop;', '    nop;',
            '    r0 = dm(_st+0);', '    nop;',
            '_slot:', '    nop;', '    nop;', '    nop;', '    nop;', '    nop;', '    nop;', '    nop;', '    nop;',
            '_slot_end:', '    nop;', '    nop;',
            '    bit clr mode1 0x200000;', '    nop;', '    nop;']
    # dump: registers first (scalar), system registers last through R0 after R0 is stored
    k = 0
    for p, n in REGS:
        for j in range(n):
            asm.append('    dm(_res+%d) = %s%d;' % (k, p, j))
            k += 1
    for s in SYS:
        if s == 'mode1':
            asm += ['    r0 = mode1;']
        else:
            asm += ['    r0 = %s;' % s]
        asm.append('    dm(_res+%d) = r0;' % k)
        k += 1
    asm += ['    r0 = 0xD0E5;', '    dm(_done) = r0;', '_probe_exit:', '    leaf_exit;', '._probe.end:']
    os.makedirs(outdir, exist_ok=True)
    open(os.path.join(outdir, 'cens.asm'), 'w').write('\n'.join(asm) + '\n')
    open(os.path.join(outdir, 'cens.c'), 'w').write(
        'extern void probe(void);\nint main(void) { probe(); return 0; }\n')
    json.dump(dict(n_state=N_STATE, names=names), open(os.path.join(outdir, 'cens.json'), 'w'))
    print(N_STATE)


if __name__ == '__main__':
    main(sys.argv[1])
