#!/usr/bin/env python3
"""Bit-exact Python models of the PNO Core1 post stage `0x12272D` and output stage `0x1229B3`.

Method, equations and tags: docs/research/organ-post-stage.md. The models work on a sparse word
memory (`Mem`, a dict of DM address -> u32) and reproduce every word the routines write, checked
against the SHARC oracle by `tools/organ_post_oracle.py verify` (random scenarios, 0 differences).
Lane notation: SIMD pair `p` holds element `2p` in PEx and `2p + 1` in PEy.

    post stage (per call: two frames f = 0, 1, then the sine bank for both frames)
      LFO       phase_l += 0x905A3 (lane l = DM(0xE21F0 + l));  tri = clip(4 (|phase| - 0.5), 1)
                tri' = clip(tri - 0.08, 0.91) + 0.08;   mod = tri' * depth + 2 (fine - 0.5)
                m_l = 0.5 + mod_l / 2     (fixed point; DM(0xE20E4 + l) = mod_l)
      note k    ph_k += m_(k mod 2) * rate_k      (84 semitone oscillators, rate 2^(1/12) ladder)
                x = clip((|ph_k| - 0.5) * coef_k, 1);  out_k = T[fix(x * 2^9)]   (waveshaper table)
                k in 12..47: out_k *= (out_k > 0 ? 1 : 0.9 + 0.2 ph_k)
      smooth    Y_f[k] = A_f[k] cA + A_(f-1)[k] cB + Y_(f-1)[k] cC     (per note, 3 coefficients)
      sines     36 polynomial sine oscillators, 3 octaves, summed per pitch class in 12 buckets
    output stage (per call: two frames)
      T    = sum of 96 words (84 notes + 12 buckets) x weights (two interleaved lane sums)
      then three 2-tap sections, two smoothed gains, out = (s3 g1 + s1 g2) 0.090075 gain -> I0
"""

from __future__ import annotations

import random
import struct
from fractions import Fraction

from sharc40 import (
    ZERO,
    clip,
    f32_const,
    fadd,
    fix,
    float_int,
    fmul,
    fsub,
    ld,
    s32,
    ssf,
    ssfr,
    st,
    u32,
)

LFO, OFFSET, DEPTH, MODW = 0xE21F0, 0xE20E0, 0xE20E3, 0xE20E4
STATE, RATE, COEF, TABLE, NOTES = 0xE2B14, 0xE22B2, 0x95CC6, 0xE2512, 84
OUT_A, OUT_B, Y0, Y1, FILT = 0xE21F2, 0xE2246, 0xE2110, 0xE2170, 0x95D26
SINE_PH, LADDER, GAINS, SINES = 0xE2B98, 0xE2C8E, 0xE2BBC, 36
BASE_IN, BASE_OUT, CONSTS, BUCKET_INDEX = 0xE2BE0, 0xE2CB2, 0xE2CB4, 0x95E22
BUCKET_STRIDE = 0x60
LFO_STEP = 0x905A3
RATE_BASE = 0x62C2E7  # DM(0xE22B2): first of 84 rates, each next x 2^(1/12) (oracle dump)
SCRATCH = 0xE2D20
# Bucket pair index table (DM 0x95E22, one word per pair): pairs 16 and 17 add to buckets 0..3.
BUCKET_PAIRS = (0, 2, 4, 6, 8, 10, 0, 2, 4, 6, 8, 10, 0, 2, 4, 6, 0, 2)

HALF, TWO, ONE = f32_const(0x3F000000), f32_const(0x40000000), f32_const(0x3F800000)
K008, K091 = f32_const(0x3DA3D70A), f32_const(0x3F68F5C3)
K02, K09 = f32_const(0x3E4CCCCD), f32_const(0x3F666666)
K001, K009008 = f32_const(0x3A83126F), f32_const(0x3DB8793E)


class Mem(dict):
    """Sparse DM image: missing words read as zero."""

    def __missing__(self, key: int) -> int:
        return 0


def f2u(x: float) -> int:
    """Bit pattern of a float32."""
    return struct.unpack("<I", struct.pack("<f", x))[0]


def lfo_mod(m: Mem) -> list[int]:
    """Advance both lane LFOs; store `mod` at DM(0xE20E4 + lane); return the pitch factors m."""
    out = []
    for lane in (0, 1):
        ph = u32(m[LFO + lane] + LFO_STEP)
        m[LFO + lane] = ph
        x = fmul(fmul(fsub(abs(float_int(ph, -31)), HALF), TWO), TWO)
        x = fadd(clip(fsub(clip(x, ONE), K008), K091), K008)
        mod = u32(ssf(fix(x, 31), m[DEPTH]) + 2 * u32(m[OFFSET] - 0x40000000))
        m[MODW + lane] = mod
        out.append(u32(0x40000000 + ssf(0x40000000, mod)))
    return out


def scanner(m: Mem, out: int) -> None:
    """One frame of the 84 note oscillators into `out` (84 words)."""
    pitch = lfo_mod(m)
    for k in range(NOTES):
        ph = u32(m[STATE + k] + ssfr(pitch[k & 1], m[RATE + k]))
        m[STATE + k] = ph
        x = fmul(fsub(abs(float_int(ph, -31)), HALF), ld(m[COEF + k]))
        m[out + k] = m[TABLE + s32(fix(clip(x, ONE), 9))]
    for k in range(12, 48):
        v = ld(m[out + k])
        gain = ONE if v > 0 else fadd(fmul(float_int(m[STATE + k], -31), K02), K09)
        m[out + k] = st(fmul(v, gain))


def smooth(m: Mem, a: int, b: int, c: int, dst: int) -> None:
    """`dst[k] = a[k] cA + b[k] cB + c[k] cC`; coefficients in blocks of 12: cB x4, cA x4, cC x4."""
    for k in range(NOTES):
        g, j = divmod(k, 4)
        cb, ca, cc = (ld(m[FILT + 12 * g + 4 * t + j]) for t in range(3))
        s = fadd(fmul(ld(m[a + k]), ca), fmul(ld(m[b + k]), cb))
        m[dst + k] = st(fadd(s, fmul(ld(m[c + k]), cc)))


def sines(m: Mem) -> None:
    """36 polynomial sine oscillators of both frames; buckets at 0xE2164 (f = 0) and 0xE21C4."""
    for lane in (0, 1):
        m[BASE_OUT + lane] = u32(m[BASE_IN + lane] + ssf(m[BASE_IN + lane], m[MODW + lane]))
    for rep in range(2):
        bucket = Y0 + NOTES + BUCKET_STRIDE * rep
        for b in range(12):
            m[bucket + b] = 0
        for j in range(SINES):
            lane = j & 1
            mul = ssf if (j >> 1) < 2 else ssfr  # the two prologue pairs use the unrounded product
            ph = u32(m[SINE_PH + j] + mul(m[BASE_OUT], m[LADDER + j]))  # lane 0 word: broadcast
            m[SINE_PH + j] = ph
            u = fadd(abs(float_int(ph, s32(m[CONSTS + lane]))), ld(m[CONSTS + 2 + lane]))
            u2 = fmul(u, u)
            c7, c5, c3, c1 = (ld(m[CONSTS + 4 + 2 * t + lane]) for t in range(4))
            p = fadd(fmul(c7, u2), c5)
            p = fadd(fmul(p, u2), c3)
            p = fadd(fmul(p, u2), c1)
            y = fmul(fmul(u, ld(m[GAINS + j])), p)
            at = bucket + m[BUCKET_INDEX + 2 * (j >> 1)] + lane
            m[at] = st(fadd(y, ld(m[at])))


def post_stage(m: Mem) -> None:
    """Routine 0x12272D (the saved I0 at DM(0xE210E) is not modelled)."""
    scanner(m, OUT_A)
    smooth(m, OUT_A, OUT_B, Y1, Y0)
    scanner(m, OUT_B)
    smooth(m, OUT_B, OUT_A, Y0, Y1)
    sines(m)


def section(m: Mem, p: int, t: Fraction) -> Fraction:
    """One state block `p` (8 words) fed with `t`; both lanes end with the same output."""
    x = fadd(fmul(t, ld(m[p + 6])), fmul(ld(m[p + 1]), ld(m[p + 4])))
    y = fadd(fmul(t, ld(m[p + 7])), fmul(ld(m[p + 2]), ld(m[p + 5])))
    m[p + 2] = m[p + 3] = st(t)
    out = fadd(x, y)
    m[p] = m[p + 1] = st(out)
    return out


def smooth_gain(m: Mem, target: int, state: int) -> Fraction:
    """`state += 0.001 (target - state)` (stored in both lane words)."""
    cur = ld(m[state])
    new = fadd(cur, fmul(fsub(ld(m[target]), cur), K001))
    m[state] = m[state + 1] = st(new)
    return new


def output_stage(m: Mem, i0: int, i1: int) -> None:
    """Routine 0x1229B3 with I0 = output pointer, I1 = record (weights at DM(I1 - 0x1F))."""
    weights, g = m[i1 - 0x1F], m[i1 - 0x1E]
    src = Y0
    for _ in range(2):
        lane_sums = []
        for lane in (0, 1):
            even = odd = ZERO
            for n in range(48):
                prod = fmul(ld(m[src + 2 * n + lane]), ld(m[weights + 2 * n + lane]))
                if n % 2:
                    odd = fadd(odd, prod)
                else:
                    even = fadd(even, prod)
            lane_sums.append(fadd(even, odd))
        src += NOTES + 12
        t = fadd(lane_sums[1], lane_sums[0])
        out = t
        for k, state in enumerate((m[g + 8], m[g + 2], m[g + 3])):
            out = section(m, state, out)
            if k == 0:
                m[SCRATCH] = m[SCRATCH + 1] = st(out)
        g1, g2 = smooth_gain(m, g, g + 4), smooth_gain(m, g + 1, g + 6)
        y = fadd(fmul(out, g1), fmul(ld(m[SCRATCH]), g2))
        m[i0] = m[i0 + 1] = st(fmul(fmul(y, K009008), ld(m[g + 0xB])))
        i0 += 2


def synthetic_inputs(seed: int) -> dict[int, int]:
    """A fully synthetic input scenario (random states, tables and coefficients), as DM pokes.

    Covers every word the two routines read except the data constants that the models fix
    (float scale -31, fix scale 9, the 1.0/0.5 immediates live in the code).
    """
    rng = random.Random(seed)
    uni = lambda lo, hi: f2u(rng.uniform(lo, hi))  # noqa: E731
    p: dict[int, int] = {}
    p[LFO], p[LFO + 1] = rng.getrandbits(32), rng.getrandbits(32)
    p[OFFSET] = 0x40000000 + rng.randrange(-(2**24), 2**24)
    p[DEPTH] = rng.randrange(0, 2**26)
    for k in range(NOTES):
        p[STATE + k] = rng.getrandbits(32)
        p[RATE + k] = rng.randrange(0, 2**27)
        p[COEF + k] = uni(0.5, 150)
        for base in (OUT_A, OUT_B, Y0, Y1):
            p[base + k] = uni(-1, 1)
    for i in range(-512, 513):
        p[TABLE + i] = uni(-1, 1)
    for i in range(3 * 4 * (NOTES // 4)):
        p[FILT + i] = uni(-1, 1)
    for j in range(SINES):
        p[SINE_PH + j] = rng.getrandbits(32)
        p[LADDER + j] = rng.randrange(0, 2**30)
        p[GAINS + j] = uni(0, 1)
    for lane in (0, 1):
        p[BASE_IN + lane] = rng.randrange(0, 2**30)
        p[CONSTS + lane] = u32(rng.choice((-30, -31)))
        p[CONSTS + 2 + lane] = uni(-1.5, -0.5)
        for t in range(4):
            p[CONSTS + 4 + 2 * t + lane] = uni(-2, 2)
    for q, v in enumerate(BUCKET_PAIRS):
        p[BUCKET_INDEX + 2 * q], p[BUCKET_INDEX + 2 * q + 1] = v, 0
    return p


# Output-stage scratch layout used by the verification and chain scenarios (free DM, ASSUMED).
OUT_WEIGHTS, OUT_PARAMS, OUT_STATES, OUT_RECORD = (
    0xB1000,
    0xB1100,
    (0xB1200, 0xB1210, 0xB1220),
    0xB1320,
)


def output_inputs(seed: int) -> dict[int, int]:
    """Random scenario for the output stage: 192 data words, 96 weights, record, three states."""
    rng = random.Random(seed)
    uni = lambda lo, hi: f2u(rng.uniform(lo, hi))  # noqa: E731
    p = {Y0 + i: uni(-1, 1) for i in range(2 * (NOTES + 12))}
    p.update({OUT_WEIGHTS + i: uni(-0.05, 0.05) for i in range(NOTES + 12)})
    g = OUT_PARAMS
    p[OUT_RECORD - 0x1F], p[OUT_RECORD - 0x1E] = OUT_WEIGHTS, g
    p[g], p[g + 1] = uni(0, 1), uni(0, 1)
    p[g + 2], p[g + 3], p[g + 8] = OUT_STATES[1], OUT_STATES[2], OUT_STATES[0]
    p.update({g + o: uni(0, 1) for o in (4, 5, 6, 7)})
    p[g + 0xB] = uni(0.5, 2)
    for s in OUT_STATES:
        p.update({s + o: uni(-1, 1) for o in range(8)})
    return p
