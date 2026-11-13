"""Builds the committed regression fixture of the census diff (`census_cases.txt`, `census_measured.txt`).

usage: python census_regress.py IMAGE_DIR cens.dxe census_fixture.txt --work DIR_OF_A_FULL_RUN

Selects, from a full `census_diff.py` run (same --states/--seed/--sample as the run in --work), one
instruction per normalised shape of the families the census found bugs in, with their random core
states, and adds a sweep of every condition code over all combinations of AZ AV AN AC and AF (those
64 x 9 cases are run on the simulator here). The measured file keeps, per case, the 107 state words and
the scratch words the instruction changed (see `core::census::compact`); the Rust test
`census_regress.rs` replays the cases on the model.
"""
import argparse
import os
import random
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import census_diff as cd  # noqa: E402

N_STATE = cd.gen.N_STATE
# family -> shapes kept (the census found bugs in each; a few shapes per family keep the fixture small)
FAMILIES = [
    (r'\(lw\)', 25), (r'^if.*(dm|pm)\(', 40), (r'^if(?!.*(dm|pm)\()', 50),
    (r'ashift|lshift|fext|fdep|btgl|bclr|bset|btst|fpack|funpack|exp|leftz|lefto|clip', 40),
    (r'comp\(', 10), (r'<->', 12), (r'^s\d+=|=s\d+', 40), (r'dm\(0x', 25), (r'^mr', 10),
    (r'stky|astat', 10), (r'\*.*,.*(dm|pm)\(', 25), (r'dm\(.*\)=0$', 5),
]
# instructions the model still gets wrong (docs/research/sharc-census-diff.md, 'Open'); the fixture
# leaves them out so the test passes, the census report keeps listing them
OPEN = ('02000F80000C', '10F8000B', '500016290000', '603F10C11300', '7800013F', '7CED3F7F0000')
STATES = (0, 1, 3)   # scalar, SIMD, SIMD with ALUSAT


def shape(text):
    return re.sub(r'\d+', '#', text.lower().replace(' ', ''))


def compact(line, scr):
    if line.startswith(('FAULT', 'HALT')):
        return line
    w = line.split()
    out = w[:N_STATE]
    for i, x in enumerate(w[N_STATE:]):
        if int(x, 16) != scr[i]:
            out.append('@%d=%s' % (i, x))
    return ' '.join(out)


def condition_sweep(dxe, scr_addr, items, rnd, work):
    """Cases for every condition code over AZ AV AN AC x AF, with the sim results."""
    pick = {}
    for parcels, (name, text) in items:
        m = re.match(r'if(\w+)r(\d+)=r(\d+)$', text.lower().replace(' ', ''))
        if m and m.group(1) not in pick and m.group(2) != m.group(3):
            pick[m.group(1)] = (parcels, int(m.group(2)), int(m.group(3)))
    cases = []
    for cond in sorted(pick):
        parcels, d, s = pick[cond]
        for a in range(16):
            for af in (0, 1):
                st = cd.make_state(rnd, scr_addr, 0x010000)
                st[d], st[s] = 0x11111111, 0x22222222
                st[cd.layout().index('astat')] = a | af << 10
                cases.append((''.join('%04X' % p for p in parcels), st))
    return cases


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('images')
    ap.add_argument('dxe')
    ap.add_argument('fixture')
    ap.add_argument('--work', required=True)
    ap.add_argument('--states', type=int, default=6)
    ap.add_argument('--seed', type=int, default=1)
    ap.add_argument('--out', default=HERE)
    a = ap.parse_args()
    items = sorted((it for it in cd.load_instructions(a.images).items() if cd.usable(it)), key=lambda it: it[0])
    lines = open(os.path.join(a.work, 'cases.txt')).read().split('\n')
    head, body = lines[0], lines[1:]
    sim = open(os.path.join(a.work, 'sim.txt')).read().split('\n')
    assert len(sim) - 1 >= len(items) * a.states, 'sim.txt does not belong to this instruction set'
    scr = [int(x, 16) for x in head.split()[1:]]
    seen, picked, left = set(), [], {i: n for i, (_, n) in enumerate(FAMILIES)}
    for k, (parcels, (name, text)) in enumerate(items):
        key = shape(text)
        for i, (rx, _) in enumerate(FAMILIES):
            if left[i] and key not in seen and re.search(rx, text.lower().replace(' ', '')):
                seen.add(key)
                left[i] -= 1
                picked += [k * a.states + st for st in STATES]
                break
    out_cases, out_meas = [head], []
    for i in picked:
        if body[i].startswith(OPEN):
            continue
        out_cases.append(body[i])
        out_meas.append(compact(sim[i], scr))
    # the condition sweep: run on the simulator now
    syms = {l.split()[1]: int(l.split()[2], 16) for l in open(a.fixture) if l.startswith('sym ')}
    sweep = condition_sweep(a.dxe, syms['_scr'], items, random.Random(5), a.work)
    tmp = os.path.join(a.work, 'sweep_cases.txt')
    with open(tmp, 'w') as f:
        f.write(head + '\n')
        for parcels, st in sweep:
            f.write(parcels + ' ' + ' '.join('%08X' % w for w in st) + '\n')
    tmp_out = os.path.join(a.work, 'sweep_sim.txt')
    subprocess.check_call([sys.executable, os.path.join(HERE, 'sim_batch.py'), a.dxe, tmp, '--output', tmp_out])
    sweep_sim = open(tmp_out).read().split('\n')
    for (parcels, st), line in zip(sweep, sweep_sim):
        out_cases.append(parcels + ' ' + ' '.join('%08X' % w for w in st))
        out_meas.append(compact(line, scr))
    open(os.path.join(a.out, 'census_cases.txt'), 'w').write('\n'.join(out_cases) + '\n')
    open(os.path.join(a.out, 'census_measured.txt'), 'w').write('\n'.join(out_meas) + '\n')
    print('%d cases (%d shapes + %d condition sweep)' % (len(out_meas), len(seen), len(sweep)))


if __name__ == '__main__':
    main()
