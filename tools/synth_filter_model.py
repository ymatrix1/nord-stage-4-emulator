#!/usr/bin/env python3
"""Bit-exact Python model of the Core2 synth filter stage `0x12566E` (word 1 of a synth strip).

Written from the listing of ORG core 2 (`0x12566E..0x1257B1`, 143 instructions) and checked against
the SHARC oracle (`tools/synth_stage_oracle.py`, `sharc-org` passes): the output words are equal for
every sample of the check runs, see `docs/research/synth-stage-equations.md`.

The stage is a **two-section, four-pole low pass**. Per call it filters 4 samples (the SIMD pair
PEx / PEy runs two independent channels; this model is one lane). Parameters are three frame words:

    W28 = DM(I3 - 28)  resonance index word: idx = W28 >> 24 (0..127), fr = (W28 & 0xFFFFFF) / 2^24
    R   = DM(I3 - 30)  cutoff coefficient (a float, the tan-like frequency warp of the pitch table)
    L   = DM(I5 - 28)  = 1.0, the clip limit; H = DM(I5 - 30) = 0.5

    cin = A[idx] + (A[idx+1] - A[idx]) fr          input gain,   table A at DM(0xB0D0F)
    P   = B[idx] + (B[idx+1] - B[idx]) fr          damping 1/Q,  table B at DM(0xB1015)
    N   = 1 / sqrt(L + R P + R^2)                  (RSQRTS seed + 2 Newton steps)
    g   = 2 R N,   k = P N

and per sample, with the input x, the previous scaled input b, the states a (clipped, "band") and
c ("low"):

    section 1:  u = cin x                      section 2:  u = out of section 1
                m = (u + b) / 2                            (same, with its own a, b, c, without cin)
                c' = c + g a
                v = m - c' - k a
                a' = clip(a + g v, +-L)
                b' = u
                out = (c' + c) / 2

Arithmetic follows `tools/sharc40.py` (40-bit registers, 32-bit significand, truncating stores).
"""

from __future__ import annotations

from fractions import Fraction

import sharc40 as s

# VERIFIED: seed ROM of RSQRTS, `crates/nord-sharc/src/core/float_seeds.rs` (`RSQRTS_SEED`, ADI
# simulator measurement, 5,644 inputs, max err 0), indexed by the exponent LSB and 6 fraction MSBs.
RSQRTS_SEED = (
    (0xFF, 0xFC, 0xF8, 0xF4, 0xF0, 0xEC, 0xE8, 0xE6, 0xE2, 0xDE, 0xDC, 0xD8, 0xD4, 0xD2, 0xCE, 0xCC)
    + (
        0xC8,
        0xC6,
        0xC4,
        0xC0,
        0xBE,
        0xBC,
        0xB8,
        0xB6,
        0xB4,
        0xB2,
        0xAE,
        0xAC,
        0xAA,
        0xA8,
        0xA6,
        0xA4,
    )
    + (
        0xA2,
        0x9E,
        0x9C,
        0x9A,
        0x98,
        0x96,
        0x94,
        0x92,
        0x90,
        0x8E,
        0x8C,
        0x8A,
        0x8A,
        0x88,
        0x86,
        0x84,
    )
    + (
        0x82,
        0x80,
        0x7E,
        0x7C,
        0x7C,
        0x7A,
        0x78,
        0x76,
        0x74,
        0x74,
        0x72,
        0x70,
        0x6E,
        0x6E,
        0x6C,
        0x6A,
    ),
    (0x6A, 0x66, 0x64, 0x60, 0x5E, 0x5C, 0x5A, 0x56, 0x54, 0x52, 0x50, 0x4E, 0x4C, 0x4A, 0x46, 0x44)
    + (
        0x42,
        0x40,
        0x3E,
        0x3C,
        0x3C,
        0x3A,
        0x38,
        0x36,
        0x34,
        0x32,
        0x30,
        0x2E,
        0x2C,
        0x2C,
        0x2A,
        0x28,
    )
    + (
        0x26,
        0x26,
        0x24,
        0x22,
        0x20,
        0x20,
        0x1E,
        0x1C,
        0x1C,
        0x1A,
        0x18,
        0x16,
        0x16,
        0x14,
        0x14,
        0x12,
    )
    + (
        0x10,
        0x10,
        0x0E,
        0x0E,
        0x0C,
        0x0A,
        0x0A,
        0x08,
        0x08,
        0x06,
        0x06,
        0x04,
        0x04,
        0x02,
        0x02,
        0x00,
    ),
)

ONE = Fraction(1)
HALF = Fraction(1, 2)
THREE = Fraction(3)  # image constant 0x40400000 (3.0), register r8 of the Newton steps


def exp_of(q: Fraction) -> int:
    """Unbiased exponent of a positive rational."""
    e = q.numerator.bit_length() - q.denominator.bit_length()
    if Fraction(2) ** e > q:
        e -= 1
    if Fraction(2) ** (e + 1) <= q:
        e += 1
    return e


def rsqrts(x: Fraction) -> Fraction:
    """`Fn = RSQRTS Fx` for a positive normal x: `2^(-k-1) (1 + seed/256)`, `k = floor(exp / 2)`."""
    e = exp_of(x)
    k, odd = e // 2, e % 2
    idx = int((x / Fraction(2) ** e - 1) * 64)
    return Fraction(2) ** (-k - 1) * (1 + Fraction(RSQRTS_SEED[odd][idx], 256))


def favg(a: Fraction, b: Fraction) -> Fraction:
    """`Fn = (Fx + Fy) / 2`."""
    return s.rnd40((a + b) / 2)


def interp(t0: int, t1: int, fr: Fraction) -> Fraction:
    """`f9 = t1 - t0; f9 = f9 fr; f8 = t0 + f9` on two table words (40-bit register result)."""
    a, b = s.ld(t0), s.ld(t1)
    return s.fadd(a, s.fmul(s.fsub(b, a), fr))


def split_word(w28: int) -> tuple[int, Fraction]:
    """Table index (byte 3) and fraction (`FDEP 7:24`, `FLOAT BY -31`) of a table position word."""
    return (w28 >> 24) & 0xFF, Fraction(w28 & 0xFFFFFF, 1 << 24)


class Coefs:
    """Frame words written by the prologue (`DM(I3 - 26)`, `-22`, `-20`) and the 40-bit values."""

    def __init__(self, cin: int, g: int, k: int) -> None:
        self.words = (cin, g, k)


def coefficients(
    w28: int, r_word: int, table_a: list[int], table_b: list[int]
) -> tuple[int, int, int]:
    """Prologue: `(cin, g, k)` as memory words from the position word, R and the two tables."""
    idx, fr = split_word(w28)
    cin40 = interp(table_a[idx], table_a[idx + 1], fr)
    p40 = interp(table_b[idx], table_b[idx + 1], fr)
    r = s.ld(r_word)
    f2 = s.fmul(r, r)
    f3 = s.fmul(r, p40)
    f0 = s.fadd(s.fadd(ONE, f3), f2)
    f4 = rsqrts(f0)
    for _ in range(2):
        f12 = s.fmul(s.fmul(f4, f4), f0)
        f4, f12 = s.fmul(HALF, f4), s.fsub(THREE, f12)
        f4 = s.fmul(f4, f12)
    rn = s.fmul(r, f4)
    return s.st(cin40), s.st(s.fadd(rn, rn)), s.st(s.fmul(p40, f4))


class Lane:
    """One SIMD lane: the six state words of the two sections (`I3 - 10 .. I3`)."""

    def __init__(self) -> None:
        self.a1 = self.b1 = self.c1 = self.a2 = self.b2 = self.c2 = 0  # memory words


def section(u: Fraction, a_w: int, b_w: int, c_w: int, g: Fraction, k: Fraction, limit: Fraction):
    """One section; returns (out, a', b', c') with the states as memory words."""
    a, b, c = s.ld(a_w), s.ld(b_w), s.ld(c_w)
    ga = s.fmul(a, g)
    mid = favg(u, b)
    c_new = s.fadd(c, ga)
    ka = s.fmul(a, k)
    v = s.fsub(s.fsub(mid, c_new), ka)
    a_new = s.clip(s.fadd(s.fmul(v, g), a), limit)
    out = favg(c_new, c)
    return out, s.st(a_new), s.st(u), s.st(c_new)


def process(
    lane: Lane, coefs: tuple[int, int, int], xs: list[int], limit_word: int = 0x3F800000
) -> list[int]:
    """Runs the samples (memory words) through the lane; returns the output words."""
    cin, g, k = (s.ld(w) for w in coefs)
    limit = s.ld(limit_word)
    out: list[int] = []
    for xw in xs:
        u1 = s.fmul(s.ld(xw), cin)
        y1, lane.a1, lane.b1, lane.c1 = section(u1, lane.a1, lane.b1, lane.c1, g, k, limit)
        y2, lane.a2, lane.b2, lane.c2 = section(y1, lane.a2, lane.b2, lane.c2, g, k, limit)
        out.append(s.st(y2))
    return out
