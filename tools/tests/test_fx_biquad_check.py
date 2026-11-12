"""Tests for tools/fx_biquad_check.py (reference model, dump parser, comparison)."""

import math
import sys
from fractions import Fraction
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import fx_biquad_check as fx  # noqa: E402

COEF_WORDS = [0x3E95F619, 0x3F15F619, 0x3E95F612, 0, 0x3E2FB0D5]
COEF = tuple(fx.from_bits(w) for w in COEF_WORDS)


def test_rounding_helpers_follow_the_register_and_store_formats():
    third = Fraction(1, 3)
    e = fx.ext40(third)
    s = fx.store32(third)
    assert e != third and s != third
    assert abs(e - third) <= abs(s - third)  # 32-bit register value is closer than the stored word
    assert s <= third  # truncation toward zero
    assert fx.store32(-third) == -s
    assert fx.store32(Fraction(0)) == 0
    assert fx.word(fx.store32(Fraction(1, 4))) == 0x3E800000
    assert fx.from_bits(0x3F800000) == 1


def test_round_mantissa_ties_go_to_even():
    # 1 + 2^-24 is exactly between two 24-bit values; 1 is even, 1 + 2^-23 is odd;
    # 1 + 3 * 2^-24 sits between 1 + 2^-23 (odd) and 1 + 2^-22 (even)
    q = 1 + Fraction(1, 2**24)
    assert fx.round_mantissa(q, 24, True) == 1
    q = 1 + Fraction(3, 2**24)
    assert fx.round_mantissa(q, 24, True) == 1 + Fraction(2, 2**23)
    assert fx.round_mantissa(q, 24, False) == 1 + Fraction(1, 2**23)


def test_coefficients_are_the_butterworth_lowpass_at_a_quarter_of_the_rate():
    b0, b1, b2, a1, a2 = (float(c) for c in COEF)
    assert abs(b0 - (2 - math.sqrt(2)) / 2) < 1e-6
    assert abs(a2 - (2 - math.sqrt(2)) / (2 + math.sqrt(2))) < 1e-6
    assert abs(b1 - 2 * b0) < 1e-6 and a1 == 0.0
    assert abs((b0 + b1 + b2) / (1 + a2) - 1.0) < 1e-6  # unity gain at DC
    assert abs((b0 - b1 + b2) / (1 + a2)) < 1e-6  # zero at Nyquist
    # -3 dB at a quarter of the sample rate (z = j)
    z = 1j
    h = (b0 + b1 / z + b2 / z**2) / (1 + a2 / z**2)
    assert abs(abs(h) - math.sqrt(0.5)) < 1e-3


def test_one_stage_impulse_response_matches_the_difference_equation():
    st = [Fraction(0)] * 4
    zero = (Fraction(0), Fraction(0))
    out = fx.reference_cascade([(Fraction(1), Fraction(0))] + [zero] * 5, COEF, st)
    left = [float(o[0]) for o in out]
    assert abs(left[0] - float(COEF[0])) < 1e-7
    # y1 = b1 - a1 y0 = b1; y2 = b2 - a2 y0 (a1 = 0)
    assert abs(left[1] - float(COEF[1])) < 1e-6
    assert abs(left[2] - (float(COEF[2]) - float(COEF[4]) * left[0])) < 1e-6
    assert all(o[1] == 0 for o in out)


def test_state_carries_between_blocks():
    f = Fraction
    frames = [(f(1), f(1, 2))] + [(f(0), f(0))] * 7
    st_a = [f(0)] * 4
    whole = fx.reference_cascade(frames, COEF, st_a)
    st_b = [f(0)] * 4
    first = fx.reference_cascade(frames[:4], COEF, st_b)
    second = fx.reference_cascade(frames[4:], COEF, st_b)
    # carrying the stored (truncated) state differs from the register state by at most an ulp
    for got, want in zip(first + second, whole):
        assert abs(float(got[0] - want[0])) < 1e-7
        assert abs(float(got[1] - want[1])) < 1e-7


def test_three_stages_cascade_in_order():
    one = float(fx.reference_cascade([(Fraction(1), Fraction(0))], COEF, [Fraction(0)] * 4)[0][0])
    three = float(
        fx.reference_cascade([(Fraction(1), Fraction(0))], COEF, [Fraction(0)] * 12)[0][0]
    )
    assert abs(three - one**3) < 1e-6


def synthetic_dump(blocks, states, flip=None):
    st = list(states)
    lines = []
    for k, block in enumerate(blocks):
        out = fx.reference_cascade(block, COEF, st)
        lines.append(f"call {k}: 87 steps, pc 0x1210b0")
        for i, v in enumerate(v for frame in out for v in frame):
            lines.append(f"dump {fx.OUT_ADDR + i:x} {fx.word(v):08x} 0")
        for i, v in enumerate(st):
            lines.append(f"dump {fx.STATE_ADDR + i:x} {fx.word(v):08x} 0")
        for i, w in enumerate(COEF_WORDS):
            lines.append(f"dump {fx.COEF_ADDR + i:x} {w:08x} 0")
    return fx.parse_dumps("booted 1 steps\n" + "\n".join(lines))


def test_parse_and_compare_agree_on_a_reference_dump():
    blocks, states = fx.scenario()
    calls = synthetic_dump(blocks, states)
    assert len(calls) == 2
    r = fx.compare(calls, blocks, states)
    assert r["out_same"] == r["out_total"] == 16
    assert r["st_same"] == r["st_total"] == 24 and r["st_worst_ulp"] == 0
    calls[1][fx.STATE_ADDR] += 3
    calls[1][fx.OUT_ADDR] ^= 1
    r = fx.compare(calls, blocks, states)
    assert r["out_same"] == 15 and r["st_worst_ulp"] == 3 and r["st_same"] == 23
    assert fx.ulp_distance(0x3F800000, 0xBF800000) > fx.STATE_ULP_TOLERANCE


def test_oracle_command_line_names_the_routine_and_the_data():
    blocks, states = fx.scenario()
    args = fx.oracle_args("img.bin", blocks, states)
    assert args[0] == "img.bin"
    assert "0x120fdb" in args and "0x1210b0" in args
    assert args.count("--fill") == 3
    assert args[args.index("--calls") + 1] == "2"
    assert f"1:{fx.IN_ADDR:#x}=" in " ".join(args)
    assert fx.find_binary("x/sharc-fx") == Path("x/sharc-fx")
