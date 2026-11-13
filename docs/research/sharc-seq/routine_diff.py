"""Runs a real firmware routine on the simulator and on the model with random inputs, bit for bit.

usage: python routine_diff.py IMAGE.bin CORE ENTRY_HEX cens.dxe census_fixture.txt --work DIR
           --dm LO:HI [--dm LO:HI ...] --rand LO:HI [--rand ...] [--cases N] [--i1 ADDR]

The routine's code (short words `--code`, which must not overlap the probe or the C runtime of `cens.dxe`) and the DM ranges `--dm` of the DSP image are
loaded into both machines; per case the ranges `--rand` are overwritten with random words and the
core state is random (SIMD on, RND32). The census probe's slot holds `CALL (M8, I8) (DB)` with
I8 = ENTRY, M8 = 0; the routine returns into the probe, which dumps the state. The `--rand` ranges
are read back too (the routine's output). Reports the first differing words per case.
"""
import argparse
import os
import random
import struct
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, '..', '..', '..', 'tools'))
import census_diff as cd  # noqa: E402
import ns4_dsp_ldr as ldr  # noqa: E402

CALL_PARCELS = '08BE04000000' + '00010001'   # CALL (M8, I8) (DB) + two delay slots


def rng(spec):
    lo, hi = spec.split(':')
    return int(lo, 16), int(hi, 16)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('image')
    ap.add_argument('core', type=int)
    ap.add_argument('entry', type=lambda s: int(s, 16))
    ap.add_argument('dxe')
    ap.add_argument('fixture')
    ap.add_argument('--work', required=True)
    ap.add_argument('--dm', action='append', default=[])
    ap.add_argument('--rand', action='append', default=[])
    ap.add_argument('--code', default='122600:122900', help='short-word range copied from the image (must be free in the probe)')
    ap.add_argument('--cases', type=int, default=20)
    ap.add_argument('--mode1', type=lambda s: int(s, 16), default=0xE10000, help='MODE1 at the call')
    ap.add_argument('--seed', type=int, default=1)
    ap.add_argument('--i1', type=lambda s: int(s, 16), default=None, help='set I1 to this DM address')
    a = ap.parse_args()
    os.makedirs(a.work, exist_ok=True)
    img = open(a.image, 'rb').read()
    blocks, _ = ldr.parse_blocks(img)
    mem = bytes(ldr.sram_image(img, ldr.split_cores(blocks)[a.core]))
    rnd = random.Random(a.seed)
    names = cd.layout()
    scr = [rnd.getrandbits(32) for _ in range(cd.SCR_WORDS)]
    lines = ['scr ' + ' '.join('%08X' % w for w in scr)]
    code_lo, code_hi = rng(a.code)
    code = mem[code_lo * 2:code_hi * 2]
    parcels = struct.unpack('<%dH' % (len(code) // 2), code)
    lines.append('code %x %s' % (code_lo, ''.join('%04X' % p for p in parcels)))
    for r in a.dm:
        lo, hi = rng(r)
        words = struct.unpack('<%dI' % (hi - lo), mem[lo * 4:hi * 4])
        lines.append('dm %x %s' % (lo, ' '.join('%08X' % w for w in words)))
    for r in a.rand:
        lo, hi = rng(r)
        lines.append('watch %x %d' % (lo, hi - lo))
    syms = {l.split()[1]: int(l.split()[2], 16) for l in open(a.fixture) if l.startswith('sym ')}
    for _ in range(a.cases):
        st = cd.make_state(rnd, syms['_scr'], a.mode1)
        st[names.index('i8')] = a.entry
        st[names.index('m8')] = 0
        if a.i1 is not None:
            st[names.index('i1')] = a.i1
        chunks = ''
        for r in a.rand:
            lo, hi = rng(r)
            chunks += ' | %x %s' % (lo, ' '.join('%08X' % rnd.getrandbits(32) for _ in range(hi - lo)))
        lines.append(CALL_PARCELS + ' ' + ' '.join('%08X' % w for w in st) + chunks)
    cases = os.path.join(a.work, 'rcases.txt')
    open(cases, 'w').write('\n'.join(lines) + '\n')
    simo, modo = os.path.join(a.work, 'rsim.txt'), os.path.join(a.work, 'rmodel.txt')
    subprocess.check_call([sys.executable, os.path.join(HERE, 'sim_batch.py'), a.dxe, cases, '--output', simo])
    with open(modo, 'w') as f:
        subprocess.check_call(['cargo', 'run', '-q', '-p', 'nord-sharc', '--bin', 'sharc_census_run', '--', a.fixture, cases],
                              stdout=f)
    sim, mod = open(simo).read().split('\n'), open(modo).read().split('\n')
    n_state = cd.gen.N_STATE
    bad = 0
    for i in range(a.cases):
        if sim[i] == mod[i]:
            continue
        bad += 1
        x, y = sim[i].split(), mod[i].split()
        diffs = []
        for j in range(min(len(x), len(y))):
            if x[j] != y[j]:
                where = names[j] if j < n_state else ('scr%d' % (j - n_state) if j < n_state + cd.SCR_WORDS else 'out%d' % (j - n_state - cd.SCR_WORDS))
                diffs.append('%s sim=%s model=%s' % (where, x[j], y[j]))
        print('case %d: %d words differ: %s' % (i, len(diffs), '; '.join(diffs[:6])))
    print('%d of %d cases differ' % (bad, a.cases))


if __name__ == '__main__':
    main()
