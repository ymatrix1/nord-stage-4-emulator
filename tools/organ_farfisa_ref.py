#!/usr/bin/env python3
"""Bit-exact Python model of the Farfisa tone generator `0x122A71` (PNO core 1, organ model 2).

The routine is a free-running **top-octave divider**: 12 phase accumulators (one per semitone,
SIMD lanes x / y of 6 pairs) each feed a ladder of 8 or 9 octave rows, and every row is a soft pulse
wave read from the doubled phase. Nothing in it depends on the keys; the keys act in the processor
`0x122B85` (weights over the ring). Method, listing and tags: `docs/research/organ-farfisa-bass.md`.
The model works on a sparse word memory like `organ_post_ref.Mem`; it reproduces every word the
routine writes, checked against the SHARC oracle by `tools/organ_farfisa_oracle.py verify`.

    LFO (lane l)    ph_l += rate          (DM(0xC27D8 + l), rate = DM(0xC27D6); depth = DM(0xC27D7))
                    tri = 2 |ph_l / 2^31| - 1;  lfo = A tri + B tri^3 + C tri^5  (DM(0xC27DA..DF))
                    m_l = 1 + lfo * depth / 2^31
    step multiplier K_l = fix(fine * m_l * 2^31)   (fine = DM(0xC27D4) / 2^31, lane 0 for both)
    pair j (j < 6), lane l, frame f in {0, 1}:
        phase += ssf(K_l, inc)                                    (inc, phase: the stream `S`)
        row r:  u = (|phase_r / 2^31| + off_r) slope_r            (table `T`, 4 pair words per row)
                y = max(b_r clip(u) + a_r prev, clip(u))           -> ring[f][12 r + 2 j + l]
                (prev = the same ring word of the other frame: ring 1 for f = 0, ring 0 for f = 1)
                phase_(r+1) = 2 phase_r + S_r                     (wraps at 32 bits)
"""

from __future__ import annotations

import math
import random
import struct

from sharc40 import ZERO, clip, f32_const, fadd, fix, float_int, fmul, ld, ssf, st, u32

LFO_RATE, LFO_DEPTH, LFO_PHASE, FINE = 0xC27D6, 0xC27D7, 0xC27D8, 0xC27D4
LFO_COEF = 0xC27DA  # A, B, C pairs: DM(0xC27DA + 2 k + lane)
PTRS = 0xC27D0  # i2 (T table), i3 (row counts), i4 (stream S), i5 (ring) as long words
RING = (0xC2B52, 0xC2BB4)
ROW_STRIDE = 12  # ring words per row (12 oscillators)
PAIRS = 6
ONE = f32_const(0x3F800000)
SCALE = -31


class Mem(dict):
    """Sparse DM image: missing words read as zero."""

    def __missing__(self, key: int) -> int:
        return 0


def f2u(x: float) -> int:
    """Bit pattern of a float32."""
    return struct.unpack("<I", struct.pack("<f", x))[0]


def lfo_multiplier(m: Mem) -> list[int]:
    """Advance both lane LFOs and return the step multipliers K_l (q31 words)."""
    rate, depth = m[LFO_RATE], m[LFO_DEPTH]
    out = []
    ks = []
    for lane in (0, 1):
        ph = u32(m[LFO_PHASE + lane] + rate)  # lane 1 also uses lane 0's rate (swap)
        m[LFO_PHASE + lane] = ph
        tri = fadd(fadd(abs(float_int(ph, SCALE)), abs(float_int(ph, SCALE))), -ONE)
        t2 = fmul(tri, tri)
        t3 = fmul(tri, t2)
        t5 = fmul(t2, t3)
        a, b, c = (ld(m[LFO_COEF + 2 * k + lane]) for k in range(3))
        lfo = fadd(fadd(fmul(tri, a), fmul(t3, b)), fmul(t5, c))
        ks.append(fadd(fmul(lfo, float_int(depth, SCALE)), ONE))
    fine = float_int(m[FINE], SCALE)  # both lanes use DM(0xC27D4) (swap of the float copy)
    for lane in (0, 1):
        out.append(fix(fmul(fine, ks[lane]), 31))
    return out


def frame(m: Mem, k: list[int], ring: int, other: int) -> None:
    """One frame: 6 oscillator pairs into `ring`; `other` is the other frame's ring (feedback)."""
    i2, i3, i4 = ld_ptr(m, 0), ld_ptr(m, 1), ld_ptr(m, 2)
    i5 = ring
    for _ in range(PAIRS):
        base = i5
        count = m[i3]  # lane 0 count (the loop counter is the PEx register)
        i3 += 1
        inc = (m[i4], m[i4 + 1])
        ph = tuple(u32(m[i4 + 2 + lane] + ssf(k[lane], inc[lane])) for lane in (0, 1))
        for lane in (0, 1):
            m[i4 + 2 + lane] = ph[lane]
        i4 += 4
        t_row = i2
        s_ptr = i4
        for r in range(count):
            for lane in (0, 1):
                off, slope, a, b = (ld(m[t_row + 2 * q + lane]) for q in range(4))
                u = fmul(fadd(abs(float_int(ph[lane], SCALE)), off), slope)
                cl = clip(u, ONE)
                prev = ld(m[other + (base - ring) + ROW_STRIDE * r + lane])
                y = fadd(fmul(cl, b), fmul(prev, a))
                y = y if y > cl else cl
                m[base + ROW_STRIDE * r + lane] = st(y)
            t_row += 8
            # every row loads one stream pair (the last one is loaded but its phase is unused)
            s = (m[s_ptr], m[s_ptr + 1])
            ph = tuple(u32(2 * ph[lane] + s[lane]) for lane in (0, 1))
            s_ptr += 2
        i2, i4 = t_row, s_ptr
        i5 = base + 2


def ld_ptr(m: Mem, n: int) -> int:
    """Pointer n (0 = T table, 1 = row counts, 2 = stream, 3 = ring) at 0xC27D0."""
    return m[PTRS + n]


def generator(m: Mem) -> None:
    """The complete routine `0x122A71`: LFO, then frame 0 into ring 0 and frame 1 into ring 1."""
    k = lfo_multiplier(m)
    frame(m, k, RING[0], RING[1])
    frame(m, k, RING[1], RING[0])


def stream_words(counts: list[int]) -> list[int]:
    """Words of the stream `S` per pair: inc (2), phase (2), then 2 x count offsets."""
    return [4 + 2 * c for c in counts]


def synthetic_inputs(seed: int) -> dict[int, int]:
    """A fully synthetic input scenario (random state, tables, counts), as DM pokes."""
    rng = random.Random(seed)
    uni = lambda lo, hi: f2u(rng.uniform(lo, hi))  # noqa: E731
    p: dict[int, int] = {}
    counts = [rng.randrange(2, 9) for _ in range(PAIRS)]
    t_base, c_base, s_base = 0xC2950, 0xC2C1A, 0xC2AD8
    p[PTRS], p[PTRS + 1], p[PTRS + 2], p[PTRS + 3] = t_base, c_base, s_base, RING[0]
    p[LFO_RATE], p[LFO_DEPTH] = rng.randrange(0, 2**24), rng.randrange(0, 2**28)
    p[LFO_PHASE], p[LFO_PHASE + 1] = rng.getrandbits(32), rng.getrandbits(32)
    p[FINE], p[FINE + 1] = rng.randrange(2**28, 2**30), rng.randrange(2**28, 2**30)
    for q in range(6):
        p[LFO_COEF + q] = uni(-2, 2)
    s, t = s_base, t_base
    for j, c in enumerate(counts):
        p[c_base + j] = c
        for w in range(2):
            p[s + w] = rng.randrange(0x100000, 0x600000)
            p[s + 2 + w] = rng.getrandbits(32)
        s += 4
        for _ in range(c):
            for w in range(2):
                p[s + w] = rng.getrandbits(32)
            s += 2
            for q in range(8):
                p[t + q] = uni(-3, 3) if q < 2 else (uni(-1500, 1500) if q < 4 else uni(0, 1))
            t += 8
    p[c_base + PAIRS] = 0
    return p


# Closed-form tables: EMPIRICAL fits of the 108 rows of the pass image
# (`organ_farfisa_oracle.py tables`).
INC0 = 3_236_192  # stream word of oscillator 0
PHASE_OFFSET = 0xAAAAAC11
EDGE_OFFSET_WORD = 0xBF2AAAA7
SLOPE0 = -1327.1669
DECAY_C0 = 0.0047343
LAST_ROW_EXPONENT = 11
IMAGE_COUNTS = (9, 8, 8, 8, 8, 8)


def row_constants(osc: int, row: int) -> tuple[int, int, int, int]:
    """Float words (offset, slope, a, b) of row `row` of oscillator `osc` (closed form)."""
    semitone = 2 ** (osc / 12)
    a = 0.0
    if row <= 6:
        a = math.exp(-DECAY_C0 * 2**row * semitone)
    elif row == 7:
        a = math.exp(-DECAY_C0 * 2**LAST_ROW_EXPONENT * semitone)
    slope = SLOPE0 * 2.0**-row / semitone
    return EDGE_OFFSET_WORD, f2u(slope), f2u(a), f2u(1.0 - a)


def closed_form_inputs(
    counts: tuple[int, ...] = IMAGE_COUNTS, rate: int = 0, depth: int = 0, fine: int = 0x40000000
) -> dict[int, int]:
    """Pokes of a complete generator state from the closed-form tables (phases and rings zero)."""
    p: dict[int, int] = {}
    t, c, s = 0xC2950, 0xC2C1A, 0xC2AD8
    p[PTRS], p[PTRS + 1], p[PTRS + 2], p[PTRS + 3] = t, c, s, RING[0]
    p[LFO_RATE], p[LFO_DEPTH], p[FINE], p[FINE + 1] = rate, depth, fine, fine
    for k, v in enumerate((0x3FC90069, 0x3FC90069, 0xBF245655, 0xBF245655, 0x3D92AA8A, 0x3D92AA8A)):
        p[LFO_COEF + k] = v
    for j, n in enumerate(counts):
        p[c + j] = n
        for lane in (0, 1):
            osc = 2 * j + lane
            p[s + lane] = round(INC0 * 2 ** (osc / 12))
            p[s + 2 + lane] = 0
        s += 4
        for r in range(n):
            for lane in (0, 1):
                p[s + lane] = PHASE_OFFSET
            s += 2
            for lane in (0, 1):
                for q, w in enumerate(row_constants(2 * j + lane, r)):
                    p[t + 2 * q + lane] = w
            t += 8
    return p


def self_check() -> bool:
    """Tiny smoke test: zero memory runs without error and writes only the LFO phases."""
    m = Mem()
    m[PTRS + 3] = RING[0]
    lfo_multiplier(m)
    return m[LFO_PHASE] == 0 and ZERO == 0


if __name__ == "__main__":
    print(self_check())
