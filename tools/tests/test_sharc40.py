"""Unit tests for tools/sharc40.py (exact 40-bit SHARC float and fractional arithmetic)."""

from __future__ import annotations

import struct
from fractions import Fraction

import sharc40 as s


def bits(x: float) -> int:
    return struct.unpack("<I", struct.pack("<f", x))[0]


def test_integer_helpers_wrap_and_sign_extend():
    assert s.s32(0xFFFFFFFF) == -1
    assert s.s32(0x7FFFFFFF) == 0x7FFFFFFF
    assert s.u32(-1) == 0xFFFFFFFF
    assert s.u32(1 << 32) == 0


def test_rounding_keeps_a_32_bit_significand_nearest_even():
    third = s.rnd40(Fraction(1, 3))
    assert third != Fraction(1, 3)
    assert abs(third - Fraction(1, 3)) < Fraction(1, 2**33)  # error below half an ulp of 2^-2
    assert s.rnd40(Fraction(3)) == 3  # exact values are kept
    # 1 + 2^-32 is a tie between 1 and 1 + 2^-31: nearest even is 1
    assert s.rnd40(1 + Fraction(1, 2**32)) == 1
    assert s.rnd40(1 + Fraction(3, 2**32)) == 1 + Fraction(1, 2**30)
    assert s.rnd40(Fraction(-5, 3)) == -s.rnd40(Fraction(5, 3))


def test_denormals_flush_to_zero():
    assert s.rnd40(Fraction(1, 2**127)) == 0
    assert s.ld(0x00400000) == 0  # a denormal word loads as zero


def test_load_and_store_use_the_top_32_bits_only():
    for x in (1.0, -0.5, 3.1415927, 1e-20, -123456.0):
        assert s.st(s.ld(bits(x))) == bits(x)
    # a 32-bit significand result is truncated, not rounded, when stored
    x = s.rnd40(Fraction(1) + Fraction(0xFF, 2**31))  # extension bits all ones
    assert s.st(x) == bits(1.0)
    assert s.st(-x) == bits(-1.0)
    assert s.st(Fraction(0)) == 0


def test_float_arithmetic_rounds_after_each_operation():
    a, b = s.ld(bits(1.0)), s.ld(bits(3.0))
    third = s.fmul(a, s.rnd40(Fraction(1, 3)))
    assert third == s.rnd40(Fraction(1, 3))
    assert s.fadd(a, b) == 4
    assert s.fsub(a, b) == -2
    # the product of the rounded 1/3 and 3 is within one 40-bit ulp of 1; a store truncates it
    t = s.rnd40(Fraction(1, 3))
    assert abs(s.fmul(t, b) - 1) <= Fraction(1, 2**31)
    assert s.st(s.fmul(t, b)) in (bits(1.0), bits(1.0) - 1)


def test_float_int_is_exact_and_fix_rounds_half_even():
    assert s.float_int(0x40000000, -31) == Fraction(1, 2)
    assert s.float_int(0x80000000, -31) == -1
    assert s.fix(Fraction(5, 2), 0) == 2  # tie -> even
    assert s.fix(Fraction(7, 2), 0) == 4
    assert s.s32(s.fix(Fraction(-3, 2), 0)) == -2
    assert s.fix(Fraction(1, 2), 9) == 256


def test_clip_limits_to_the_magnitude_of_the_second_operand():
    one = Fraction(1)
    assert s.clip(Fraction(3), one) == 1
    assert s.clip(Fraction(-3), -one) == -1
    assert s.clip(Fraction(1, 4), one) == Fraction(1, 4)


def test_fractional_multiply_truncates_or_rounds():
    half = 0x40000000
    assert s.ssf(half, half) == 0x20000000  # 0.5 * 0.5 = 0.25
    assert s.ssf(0xC0000000, half) == 0xE0000000  # -0.5 * 0.5 = -0.25
    # 3 * 0x40000000 * 2 / 2^32 = 1.5: floor 1, round-half-even 2
    assert s.ssf(3, half) == 1
    assert s.ssfr(3, half) == 2
    # exactly 2.5 -> 2 (tie to even); exactly 3.5 -> 4
    assert s.ssfr(5, half) == 2
    assert s.ssfr(7, half) == 4
    assert s.ssf(0xFFFFFFFF, half) == 0xFFFFFFFF  # -1 * 0.5 = -0.5 ulp floors to -1


def test_float_constants_decode_the_immediates_of_the_listing():
    assert s.f32_const(0x3F000000) == Fraction(1, 2)
    assert s.f32_const(0x40000000) == 2
