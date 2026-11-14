#!/usr/bin/env python3
"""Exact SHARC arithmetic used by the Python models of the PNO Core1 routines.

Floats are exact rationals rounded after every operation to the 32-bit significand of the 40-bit
register format (`MODE1 RND32 = 0`, the state the firmware runs in), denormals flushed; a store to
memory keeps the top 32 bits (23 fraction bits, truncated), a load puts zeros in the extension.
Fixed point is the SSF/SSFR fractional multiply and 32-bit integer wrap. Provenance (all VERIFIED
by `tools/organ_post_oracle.py verify`, bit-exact on the 0x12272D and 0x1229B3 routines against
the `nord-sharc` oracle): `docs/research/organ-post-stage.md` section 2.
"""

from __future__ import annotations

from fractions import Fraction

ZERO = Fraction(0)
TINY = Fraction(2) ** -126


def s32(u: int) -> int:
    """Signed value of a 32-bit word."""
    u &= 0xFFFFFFFF
    return u - (1 << 32) if u >> 31 else u


def u32(i: int) -> int:
    """32-bit wrap."""
    return int(i) & 0xFFFFFFFF


def _ilog2(x: Fraction) -> int:
    """floor(log2(x)) of a positive rational."""
    e = x.numerator.bit_length() - x.denominator.bit_length()
    return e - 1 if Fraction(2) ** e > x else e


def _nearest_even(q: Fraction) -> int:
    fl = q.numerator // q.denominator
    rem = q - fl
    return fl + 1 if rem > Fraction(1, 2) or (rem == Fraction(1, 2) and fl & 1) else fl


def rnd40(x: Fraction) -> Fraction:
    """Round to a 32-bit significand (nearest even); results below 2^-126 flush to zero."""
    if x == 0:
        return ZERO
    a = abs(x)
    e = _ilog2(a)
    r = _nearest_even(a / Fraction(2) ** (e - 31)) * Fraction(2) ** (e - 31)
    return ZERO if r < TINY else (-r if x < 0 else r)


def ld(word: int) -> Fraction:
    """A memory word as a register float (denormals are zero)."""
    word &= 0xFFFFFFFF
    e = (word >> 23) & 0xFF
    if e == 0:
        return ZERO
    v = Fraction((1 << 23) | (word & 0x7FFFFF), 1 << 23) * Fraction(2) ** (e - 127)
    return -v if word >> 31 else v


def st(x: Fraction) -> int:
    """A register float as a memory word: the top 32 bits of the 40-bit value (truncation)."""
    if x == 0:
        return 0
    e = _ilog2(abs(x))
    m = int(abs(x) / Fraction(2) ** (e - 23))
    return (1 << 31 if x < 0 else 0) | ((e + 127) << 23) | (m & 0x7FFFFF)


def fadd(a: Fraction, b: Fraction) -> Fraction:
    return rnd40(a + b)


def fsub(a: Fraction, b: Fraction) -> Fraction:
    return rnd40(a - b)


def fmul(a: Fraction, b: Fraction) -> Fraction:
    return rnd40(a * b)


def float_int(word: int, scale: int) -> Fraction:
    """`FLOAT Rn BY scale`: a 32-bit integer is exact in the 40-bit format."""
    return Fraction(s32(word)) * Fraction(2) ** scale


def fix(x: Fraction, scale: int) -> int:
    """`FIX Fx BY scale`, round to nearest even (overflow is not modelled)."""
    return u32(_nearest_even(x * Fraction(2) ** scale))


def clip(x: Fraction, limit: Fraction) -> Fraction:
    """`CLIP Fx BY Fy`: x limited to +-|y|."""
    limit = abs(limit)
    return limit if x > limit else (-limit if x < -limit else x)


def ssf(a: int, b: int) -> int:
    """Signed fractional multiply, truncated (floor of 2ab / 2^32)."""
    return u32((s32(a) * s32(b) * 2) >> 32)


def ssfr(a: int, b: int) -> int:
    """Signed fractional multiply, rounded half to even at bit 32."""
    p = s32(a) * s32(b) * 2
    r = p + 0x80000000
    if p & 0xFFFFFFFF == 0x80000000:
        r &= ~(1 << 32)
    return u32(r >> 32)


def f32_const(bits: int) -> Fraction:
    """A float32 immediate."""
    return ld(bits)
