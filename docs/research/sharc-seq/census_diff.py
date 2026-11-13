"""Diff of the Rust model against the simulator for every single-instruction form in the firmware.

usage: python census_diff.py IMAGE_DIR cens.dxe census_fixture.txt --work DIR [--sample N] [--states K]

IMAGE_DIR has the `dsp_image_*.bin` files of `tools/ns4_dsp_images.py`. Every distinct instruction
(raw parcels) of the code spans (same sweep as `tools/sharc_census.py`) that is not control flow or a
timing/stack register access becomes `--states` cases: the instruction is poked into the probe slot
(`gen_census_probe.py`) with a full random core state (special values mixed in, MODE1 varied). The
simulator (`sim_batch.py`, headless) and the model (`sharc_census_run`) run the same cases; the
report lists every difference by instruction. Absolute DM addresses (Type 14a) are redirected into the
scratch table.
"""
import argparse
import collections
import glob
import os
import random
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, '..', '..', '..', 'tools'))
import gen_census_probe as gen  # noqa: E402

SCR_WORDS = 512
SPECIAL = [0, 1, 0xFFFFFFFF, 0x7FFFFFFF, 0x80000000, 0x3F800000, 0xBF800000, 0x7F800000, 0xFF800000,
           0x7FC00000, 0x00800000, 0x00000001, 0x40490FDB, 0x0000FFFF, 0xFFFF0000, 0x3F000000]
MODES = [0x010000, 0x210000, 0x000000, 0x212000, 0x018000, 0x012000]
DENY = re.compile(r'\b(JUMP|CALL|RTS|RTI|DO|UNTIL|IDLE|PUSH|POP|FLUSH|LOOP|CJUMP|RFRAME|NOP|TCOUNT|TPERIOD|'
                  r'EMUCLK|EMUCLK2|PCSTK|PCSTKP|LADDR|CURLCNTR|LCNTR|FADDR|DADDR|PX|PX1|PX2|IRPTL|LIRPTL|'
                  r'IMASK|IMASKP|FLAGS|USTAT1|USTAT2|USTAT3|USTAT4|CACHE)\b|raw|\?\?')


def layout():
    names = []
    for p, n in gen.REGS:
        names += ['%s%d' % (p, k) for k in range(n)]
    return names + list(gen.SYS)


def load_instructions(image_dir):
    import ns4_dsp_ldr as ldr
    import sharc_visa as v
    from sharc_census import MIN_HITS, SRAM_CODE_END, SRAM_CODE_START
    seen = {}
    for f in sorted(glob.glob(os.path.join(image_dir, 'dsp_image_*.bin'))):
        img = open(f, 'rb').read()
        blocks, end = ldr.parse_blocks(img)
        if not blocks or end != len(img):
            continue
        for core in ldr.split_cores(blocks):
            mem = bytes(ldr.sram_image(img, core))
            for lo, hi, hits in v.sweep(mem, SRAM_CODE_START, SRAM_CODE_END):
                if hits < MIN_HITS:
                    continue
                insns, _ = v.tile(v.parcels_le(mem[lo:hi]))
                for name, parcels in insns:
                    seen.setdefault(tuple(parcels), (name, v.decode(parcels)))
    return label_with_adi(seen)


def label_with_adi(seen):
    """Replaces the `sharc_visa` text with ADI's own `elfdump` text; drops what ADI does not decode
    to the same parcels (the tiler also cuts data and mis-aligned code into 'instructions')."""
    import adi_dis
    order = sorted(seen)
    stream, addr = [], []
    for parcels in order:
        addr.append(len(stream))
        stream += list(parcels)
    listing = adi_dis.disassemble(stream)
    out = {}
    for parcels, a in zip(order, addr):
        entry = listing.get(a)
        if entry and entry[0] == list(parcels) and re.search(r'[=<(]', entry[1]):
            out[parcels] = (seen[parcels][0], entry[1])
    return out


BIG_OFFSET = re.compile(r'(?:dm|pm)\((0x[0-9a-f]{5,}),\s*i\d+\)', re.I)
BANK_BITS = 0x4F8   # MODE1 bits that swap register banks (SRRFL/H, SRD1L/H, SRD2L/H)


def usable(item):
    """Single instructions the probe can run: no control flow, no timer/stack/interrupt registers,
    no MODE1 writes (a random MODE1 breaks the probe), no offsets that leave data memory."""
    parcels, (name, text) = item
    t = text.lower().replace(' ', '')
    if DENY.search(text.upper()) or len(parcels) > 3:
        return False
    # ADI prints a blank compute (leading comma) or 'ch' for bit patterns that are no instruction:
    # data cut into 'code' by the tiler (parcel 3 repeating parcel 1 is the usual sign)
    if text.lstrip().startswith(',') or t.endswith('=ch') or (len(parcels) == 3 and parcels[0] == parcels[2]):
        return False
    if re.match(r'(mode1|mode2)=', t) or re.search(r',(mode1|mode2)=', t):
        return False
    m = BIG_OFFSET.search(text)
    if m and 0x8000 < int(m.group(1), 16) < 0xFFFF8000:
        return False
    b = re.match(r'bit(set|clr|tgl|xor|tst)mode1(0x[0-9a-f]+)', t)
    return not (b and int(b.group(2), 16) & BANK_BITS)


def redirect_abs(parcels, name, scr_addr, rnd):
    """Type 14a `DM/PM(addr)` access: point it into the scratch table."""
    if name != '14a':
        return list(parcels)
    a = scr_addr + 0x40 + rnd.randrange(0x80)
    return [parcels[0], a >> 16, a & 0xFFFF]


def make_state(rnd, scr_addr, mode1):
    def data():
        return rnd.choice(SPECIAL) if rnd.random() < 0.4 else rnd.getrandbits(32)
    s = [data() for _ in range(32)]                                             # r, s
    s += [0] * 16 + [0] * 16                                                    # b, l
    s += [scr_addr + 0x100 + rnd.randrange(64) for _ in range(16)]             # i
    s += [rnd.choice([1, 2, 3, 4, 0xFFFFFFFF, 0xFFFFFFFE]) for _ in range(16)]  # m
    def astat():   # all 32 bits random, but AZ and AN never hold together
        a = rnd.getrandbits(32)
        return a & ~0x4 if a & 1 else a
    s += [mode1, astat(), astat(), rnd.getrandbits(32), rnd.getrandbits(32)]
    s += [data() for _ in range(6)]
    return s


def build_cases(items, states, scr_addr, seed, path):
    rnd = random.Random(seed)
    scr = [rnd.getrandbits(32) for _ in range(SCR_WORDS)]
    meta = []
    with open(path, 'w') as out:
        out.write('scr ' + ' '.join('%08X' % w for w in scr) + '\n')
        for parcels, (name, text) in items:
            for k in range(states):
                ps = redirect_abs(parcels, name, scr_addr, rnd)
                st = make_state(rnd, scr_addr, MODES[k % len(MODES)])
                out.write(''.join('%04X' % p for p in ps) + ' ' + ' '.join('%08X' % w for w in st) + '\n')
                meta.append((text, name, k))
    return meta


def run(cmd, outfile=None):
    r = subprocess.run(cmd, stdout=open(outfile, 'w') if outfile else None, universal_newlines=True)
    if r.returncode:
        sys.exit('failed: %s' % ' '.join(cmd))


def run_sim(dxe, cases, out, jobs):
    """Runs sim_batch.py on `jobs` shards of the case file in parallel (one simulator per process)."""
    lines = open(cases).read().split(chr(10))
    head, body = lines[0], [l for l in lines[1:] if l]
    size = -(-len(body) // jobs)
    shards = [body[k:k + size] for k in range(0, len(body), size)]
    procs = []
    for k, sh in enumerate(shards):
        c, o = '%s.%d' % (cases, k), '%s.%d' % (out, k)
        open(c, 'w').write(head + chr(10) + chr(10).join(sh) + chr(10))
        cmd = [sys.executable, os.path.join(HERE, 'sim_batch.py'), dxe, c, '--output', o]
        procs.append((subprocess.Popen(cmd, stdout=subprocess.DEVNULL), o))
    with open(out, 'w') as f:
        for p, o in procs:
            if p.wait():
                sys.exit('shard failed: ' + o)
            f.write(open(o).read())


def report(meta, sim_path, model_path, n_state, path):
    names = layout()
    sim = open(sim_path).read().split('\n')
    mod = open(model_path).read().split('\n')
    groups = collections.defaultdict(list)
    bad = 0
    for i, (text, name, k) in enumerate(meta):
        a, b = sim[i], mod[i]
        if a == b or (a.startswith('HALT') and b.startswith('FAULT')):   # both refuse the instruction
            continue
        bad += 1
        x, y = a.split(), b.split()
        if y[:1] == ['FAULT'] or x[:1] == ['HALT']:
            diff = [a[:60], b[:60]]
        else:
            diff = ['%s sim=%s model=%s' % (names[j] if j < n_state else 'scr%d' % (j - n_state), x[j], y[j])
                    for j in range(min(len(x), len(y))) if x[j] != y[j]][:6]
        key = re.sub(r'\b[RFIMLBS]\d+\b', 'X', re.sub(r'0x[0-9a-f]+', '#', text))
        groups[key].append((text, k, diff))
    with open(path, 'w') as f:
        f.write('%d of %d cases differ, %d instruction shapes\n\n' % (bad, len(meta), len(groups)))
        for key, lst in sorted(groups.items(), key=lambda kv: -len(kv[1])):
            f.write('%s  (%d cases)\n' % (key, len(lst)))
            for text, k, diff in lst[:3]:
                f.write('    %s [state %d]: %s\n' % (text, k, '; '.join(diff)))
    return bad


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('images')
    ap.add_argument('dxe')
    ap.add_argument('fixture')
    ap.add_argument('--work', required=True)
    ap.add_argument('--sample', type=int, default=0, help='random subset of N instructions (0 = all)')
    ap.add_argument('--states', type=int, default=6)
    ap.add_argument('--seed', type=int, default=1)
    ap.add_argument('--reuse-sim', action='store_true', help='keep sim.txt, rerun only the model and the report')
    ap.add_argument('--jobs', type=int, default=1, help='parallel simulator processes')
    a = ap.parse_args()
    os.makedirs(a.work, exist_ok=True)
    items = sorted((it for it in load_instructions(a.images).items() if usable(it)), key=lambda it: it[0])
    if a.sample:
        items = random.Random(a.seed).sample(items, min(a.sample, len(items)))
    syms = {l.split()[1]: int(l.split()[2], 16) for l in open(a.fixture) if l.startswith('sym ')}
    cases, simo, modo = (os.path.join(a.work, n) for n in ('cases.txt', 'sim.txt', 'model.txt'))
    meta = build_cases(items, a.states, syms['_scr'], a.seed, cases)
    print('%d instructions, %d cases' % (len(items), len(meta)))
    if a.reuse_sim:   # sim.txt from an earlier run with the same --seed/--states/--sample
        pass
    elif a.jobs > 1:
        run_sim(a.dxe, cases, simo, a.jobs)
    else:
        run([sys.executable, os.path.join(HERE, 'sim_batch.py'), a.dxe, cases, '--output', simo])
    run(['cargo', 'run', '-q', '-p', 'nord-sharc', '--bin', 'sharc_census_run', '--', a.fixture, cases], modo)
    bad = report(meta, simo, modo, gen.N_STATE, os.path.join(a.work, 'report.txt'))
    print('%d differing cases, see %s' % (bad, os.path.join(a.work, 'report.txt')))


if __name__ == '__main__':
    main()
