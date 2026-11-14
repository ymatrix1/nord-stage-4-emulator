#!/usr/bin/env python3
"""Bit-exact model of the wavetable unison stage `0x124985` (ANALOG SUPER waves 2..4).

Listing `0x24930A..0x249872` (ORG / PNO core 2, SIMD). Four detuned voices read a band-limited
single-cycle table (int16 samples, two per DM word) from external memory at `0x08030000`, a
512-phase 8-tap polyphase FIR interpolates two read positions per voice, the voices are mixed with
the `P` weight groups, scaled by `P[16] g`, held for two samples each (`a a b b`) and smoothed by
three identical biquads (a transposed direct form II low pass, coefficients at PM `0x95000`).

    f   = clip(DM(-29, I2), 0.5)             t = s5 + 0.06      v = 0.75 (t + 2 t^4)
    g   = C (B + clip(A - f, B))             inc_j = (rho_j v + (1 - v)) 2 f
    idx = max(0, -(8 max(E, -15) + top3))    E, top3: exponent and 3 mantissa bits of fix(2 f)
    A, B = PM[0x9631F + 4 idx], PM[... + 1]  (broadcast: both lanes use lane x's row)
    per voice and lane: ph1 = ph + D, ph2 = ph1 + D (D = fix(inc 2^31)), the phase word becomes ph2
        pos = (ph B) >> 32, frac = (ph B) mod 2^32, i = pos + A   (ph2: i2 += B on wrap)
        six table words from `0x08030000 + (i >> 1)` per lane (buffers `0xB6F80`, `0xB6F88`)
        MRF / MRB: 4 taps (samples +0 +1 +4 +5 of the lane pair) x 8-tap phase `8 (frac >> 23)`
        out_x = (MRF_x + MRF_y) 2^-14,  out_y = (MRB_x + MRB_y) 2^-14   (`R1 <-> S0` swap)

Compared with the oracle by `tools/synth_wave_oracle.py --stage 0x124985`
(`docs/research/synth-stage-models-12.md`).
"""

from __future__ import annotations

import random
from dataclasses import dataclass, field

import synth_super24_tables as tables
import synth_wave_model as m

# VERIFIED (image words, listing `0x249326..0x249348`): the spread law and fade constants.
SPREAD_OFFSET = m.ld(0x3D75C28F)
V_SCALE = m.ld(0x3F400000)
IDX_CLIP = m.ld(0x3F7FF972)
PITCH_LIMIT = m.ld(0x3F000000)
VOICES = 4
MASK = 0xFFFFFFFF
WAVE_BASE = 0x08030000  # DM(0xC15A5): word address of the table (both lanes, broadcast load)
PM_ROWS = 0x9631F  # DM(0xC15A6)
FIR_BASE = 0x9531F  # polyphase coefficient table: 512 phases x 8 taps
LATTICE_BASE = 0x95000  # biquad coefficients c0..c4
TAP_OFFSETS = (0, 1, 4, 5)


@dataclass
class Tables:
    """The ARM / image supplied tables of the stage (all raw 32-bit words)."""

    rows: list[int]  # PM `0x9631F ..` (3072 words: offset, count, two spare per row)
    fir: list[int]  # PM `0x9531F ..` (4096 words)
    lattice: list[int]  # PM `0x95000 ..` (5 words)
    wave: list[int]  # DM `base ..` window (words, two int16 samples each)
    base: int = WAVE_BASE
    stale: dict[int, int] = field(default_factory=dict)  # extra DM words (default 0)


def exp_of(a: int) -> int:
    """`EXP` of a positive 31-bit value: minus the number of redundant sign bits."""
    return -(31 - a.bit_length()) if a else 0


def row_index(f: m.X) -> int:
    """Octave band `idx` from the doubled pitch (listing `0x2493BA..0x249404`)."""
    u = m.clip(m.add(f, f), IDX_CLIP)
    a = abs(m.s32(m.fix(u, 31)))
    e = exp_of(a)
    norm = (a << -e) & MASK
    top3 = ((norm - 0x40000000) & MASK) >> 27
    r = -(8 * max(e, -15) + top3)
    return max(r, 0)


def sat32(x: int) -> int:
    return max(-(1 << 31), min((1 << 31) - 1, x))


def fir_lane(words: list[int], rel: int, c0: int, fir: list[int], signed: bool = False) -> int:
    """One lane output sum (`R0 = MRF_x + MRF_y`) from its six buffer words.

    `rel` is the first sample's offset (0 .. 11 shorts) in the buffer, `c0` the first coefficient
    index (`8 phase`). Samples are the 16-bit halves (low half first), zero-extended as the oracle
    loads them (`signed` sign-extends: the ASSUMED hardware reading used by the Rust port).
    """
    shorts = []
    for w in words:
        shorts += [w & 0xFFFF, (w >> 16) & 0xFFFF]
    if signed:
        shorts = [v - 0x10000 if v & 0x8000 else v for v in shorts]
    total = 0
    for lane in (0, 1):
        acc = 0
        for k, off in enumerate(TAP_OFFSETS):
            at = rel + off + lane
            d = shorts[at] if at < len(shorts) else 0
            acc += 2 * d * m.s32(fir[c0 + 2 * k + lane])
        total += sat32(acc >> 32)
    return m.s32(total)


def lane_setup(ln: m.Lane):
    f = m.clip(m.ld(ln.pitch), PITCH_LIMIT)
    g = m.ld(m.st(m.mul(m.C_FADE, m.add(m.clip(m.sub(m.A_FADE, f), m.B_FADE), m.B_FADE))))
    t = m.add(m.ld(ln.spread), SPREAD_OFFSET)
    t2 = m.mul(t, t)
    t4 = m.mul(t2, t2)
    v = m.mul(m.add(m.add(t, t4), t4), V_SCALE)
    omv = m.sub(m.ONE, v)
    f2 = m.add(f, f)
    incs = [m.ld(m.st(m.mul(m.add(m.mul(m.ld(ln.ratios[j]), v), omv), f2))) for j in range(VOICES)]
    return f, g, incs


def voice_outputs(tab: Tables, lanes, incs, count: int, offset: int, j: int):
    """`[q][lane]` int sums (FIR pairs) and the new phase words of voice `j`."""
    pos = []
    for i, ln in enumerate(lanes):
        d = m.s32(m.fix(incs[i][j], 31))
        ph1 = (ln.phases[j] + d) & MASK
        ph2 = (ph1 + d) & MASK
        p1, p2 = ph1 * count, ph2 * count
        i1 = (p1 >> 32) + offset
        i2 = (p2 >> 32) + offset
        if i2 < i1:
            i2 += count
        w1 = i1 >> 1
        pos.append(
            (
                w1,
                i1 - 2 * w1,
                i2 - 2 * w1,
                (p1 & MASK) >> 23 & 0x1FF,
                (p2 & MASK) >> 23 & 0x1FF,
                ph2,
            )
        )
    bufs = [
        [tab.wave[pos[i][0] + n] if pos[i][0] + n < len(tab.wave) else 0 for n in range(6)]
        for i in (0, 1)
    ]
    out = []
    for q in (0, 1):
        x = fir_lane(bufs[0], pos[0][1 + q], pos[0][3 + q] * 8, tab.fir)
        y = fir_lane(bufs[1], pos[1][1 + q], pos[1][3 + q] * 8, tab.fir)
        out.append((x, y))
    return out, [p[5] for p in pos]


def biquad(words: list[int], coefs, s1: m.X, s2: m.X):
    """One section over four samples (transposed form II); `(y words, s1, s2)`."""
    c0, c1, c2, c3, c4 = coefs
    ys = []
    for w in words:
        x = m.ld(w)
        p0, p1, p2 = m.mul(x, c0), m.mul(x, c1), m.mul(x, c2)
        y = m.add(p0, s1)
        q = m.add(p1, s2)
        s1 = m.sub(q, m.mul(c3, y))
        s2 = m.sub(p2, m.mul(c4, y))
        ys.append(m.st(y))
    return ys, s1, s2


def stage_super24(lanes: list[m.Lane], tab: Tables):
    """Stage `0x124985` for both lanes: `(out_bits[lane][4], new phases, lattice state words)`."""
    info = [lane_setup(ln) for ln in lanes]
    # the row comes from lane x (`I9 = R2`) and is broadcast
    idx = row_index(info[0][0])
    count, offset = tab.rows[4 * idx + 1], tab.rows[4 * idx]
    incs = [info[i][2] for i in (0, 1)]
    mixes = [[[None, None] for _ in range(2)] for _ in range(VOICES)]  # [j][q][lane]
    new_phases = [[0] * VOICES for _ in lanes]
    for j in range(VOICES):
        res, phs = voice_outputs(tab, lanes, incs, count, offset, j)
        for q in (0, 1):
            xo, yo = res[q]
            mixes[j][q] = [m.ld(m.st(m.flt(xo, -14))), m.ld(m.st(m.flt(yo, -14)))]
        for i in (0, 1):
            new_phases[i][j] = phs[i]
    pair = []
    for q in (0, 1):
        lane_vals = []
        for i in (0, 1):
            run = None
            for j in range(VOICES):
                w = [m.ld(w_) for w_ in lanes[0].groups[j]]
                own = m.mul(mixes[j][q][i], w[i])
                cross = m.mul(mixes[j][q][1 - i], w[3 - i])
                run = m.add(cross, own) if j == 0 else m.add(m.add(run, own), cross)
            val = m.mul(m.mul(run, m.ld(lanes[i].out_gain)), info[i][1])
            lane_vals.append(m.st(val))
        pair.append(lane_vals)
    coefs = [m.ld(w) for w in tab.lattice]
    outs, states = [], []
    for i, ln in enumerate(lanes):
        words = [pair[0][i], pair[0][i], pair[1][i], pair[1][i]]
        new_states = []
        for n in range(3):
            lat = ln.lat or (0,) * 6
            s1, s2 = m.ld(lat[2 * n]), m.ld(lat[2 * n + 1])
            words, s1, s2 = biquad(words, coefs, s1, s2)
            new_states += [m.st(s1), m.st(s2)]
        outs.append(words)
        states.append(new_states)
    return outs, new_phases, states


_TABLES: list[Tables] = []


def load_tables() -> Tables:
    """The committed tables (`tools/synth_super24_tables.py`), loaded once."""
    if not _TABLES:
        t = tables.load()
        _TABLES.append(Tables(t["rows"], t["fir"], t["lattice"], t["wave"]))
    return _TABLES[0]


def table_pokes() -> dict[int, int]:
    """DM pokes of the PM row table and the wave table (the ARM boot words the oracle lacks)."""
    t = load_tables()
    d = {PM_ROWS + i: w for i, w in enumerate(t.rows)}
    d.update({WAVE_BASE + i: w for i, w in enumerate(t.wave)})
    return d


def random_state(seed: int, lanes: list[m.Lane]) -> None:
    """Four ratios near 1, random voice phases and biquad states (own generator)."""
    r2 = random.Random(seed * 6700417)
    for ln in lanes:
        ln.ratios = tuple(m.f2u(2 ** r2.uniform(-0.3, 0.3)) for _ in range(VOICES))
        ln.phases = tuple(r2.getrandbits(32) for _ in range(VOICES))
        ln.lat = tuple(m.f2u(r2.uniform(-0.5, 0.5)) for _ in range(6))
