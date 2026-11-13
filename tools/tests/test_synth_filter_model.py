import math
import os
import struct
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import sharc40 as s  # noqa: E402
import synth_filter_model as m  # noqa: E402

# Four table words of the ORG core 2 image around index 64 (A: input gain 0.99, 0.9745; B: damping
# 0.70711, 0.69606); only these four floats are needed for the golden row below.
A64, A65, B64, B65 = 0x3F7D70A4, 0x3F797ADF, 0x3F3504EF, 0x3F3230DF


def tables():
    """Synthetic 130-entry tables: gain 1 everywhere, damping 0.5, except entries 64 and 65."""
    a = [0x3F800000] * 130
    b = [0x3F000000] * 130
    a[64], a[65], b[64], b[65] = A64, A65, B64, B65
    return a, b


def f2u(x):
    return struct.unpack("<I", struct.pack("<f", x))[0]


def test_rsqrts_seed_is_within_a_sixteenth():
    for x in (1.0, 1.3, 1.9999, 2.0, 3.5, 0.37, 7.7):
        q = m.rsqrts(s.Fraction(x))
        assert abs(float(q) * math.sqrt(x) - 1.0) < 1.0 / 16.0


def test_split_word_is_index_and_24_bit_fraction():
    assert m.split_word(0x40000000) == (64, 0)
    idx, fr = m.split_word(0x40800000)
    assert idx == 64 and fr == s.Fraction(1, 2)
    assert m.split_word(0x7FFFFFFF)[0] == 127


def test_coefficients_equal_the_oracle_row_for_index_64():
    # oracle (`sharc-org`, frame R = 0.3, W28 = 0x40000000): cin, g, k words
    a, b = tables()
    assert m.coefficients(0x40000000, 0x3E99999A, a, b) == (0x3F7D70A4, 0x3F069B14, 0x3F1EA266)


def test_coefficients_follow_the_closed_form():
    a, b = tables()
    r, p = 0.3, 0.5
    n = 1.0 / math.sqrt(1.0 + r * p + r * r)
    cin, g, k = (float(s.ld(w)) for w in m.coefficients(0x10000000, f2u(r), a, b))
    assert cin == 1.0
    assert abs(g - 2 * r * n) < 1e-6 and abs(k - p * n) < 1e-6


def test_impulse_golden_words_from_the_oracle():
    a, b = tables()
    co = m.coefficients(0x40000000, 0x3E99999A, a, b)
    lane = m.Lane()
    y = m.process(lane, co, [0x3F800000] + [0] * 7)
    # first eight output words of `sharc-org` for the same frame (no direct feed through)
    assert y == [0, 0, 0x3B9AF973, 0x3D03A3B2, 0x3DD22068, 0x3E558729, 0x3E9D5340, 0x3EB2C886]


def test_dc_gain_is_the_input_gain():
    a, b = tables()
    co = m.coefficients(0x40000000, 0x3E99999A, a, b)
    lane = m.Lane()
    one = 0x3F800000
    y = m.process(lane, co, [one] * 600)
    assert abs(float(s.ld(y[-1])) - float(s.ld(co[0]))) < 1e-4


def test_clip_limits_the_band_state():
    a, b = tables()
    co = m.coefficients(0x7F000000, 0x3F800000, a, b)  # near self oscillation, R = 1
    lane = m.Lane()
    y = m.process(lane, co, [0x41200000] * 200)  # 10.0 step
    assert all(abs(float(s.ld(w))) < 50.0 for w in y)
    assert abs(float(s.ld(lane.a1))) <= 1.0 and abs(float(s.ld(lane.a2))) <= 1.0


def test_band_pass_and_high_pass_stages_match_the_oracle_impulse_words():
    a, b = tables()  # table C of `0x125913` equals table A at indices 64 and 65 (0.99, 0.9745)
    co = m.coefficients(0x40000000, 0x3E99999A, a, b)
    imp = [0x3F800000] + [0] * 7
    assert m.process_bp(m.Lane(), co, imp) == [
        0x3D8C230E,
        0x3E43DED8,
        0x3E300A5D,
        0xBCB4C2FE,
        0xBE6D1417,
        0xBEACB125,
        0xBE98A25C,
        0xBE1571AF,
    ]
    assert m.process_hp(m.Lane(), co, imp) == [
        0x3F15794D,
        0xBF340DC7,
        0xBEC81292,
        0xBBA7E32B,
        0x3E89B484,
        0x3EB438AA,
        0x3E871768,
        0x3DAD12F1,
    ]


def test_hp_scale_is_one_minus_g2_over_4_minus_gk_over_2():
    g_w, k_w = 0x3F069B14, 0x3F1EA266
    g, k = float(s.ld(g_w)), float(s.ld(k_w))
    assert abs(float(m.hp_scale(g_w, k_w)) - (1 - g * g / 4 - g * k / 2)) < 1e-6


def test_closed_form_response_matches_the_oracle_impulse_spectrum():
    # oracle (R = 0.3, index 64, P = 0.70711): peak 2.2629 / cin 0.99 at 0.0809, -3 dB at 0.1199
    f = m.lp4_features(0.3, 0.70711)
    assert abs(f["peak_gain"] - 2.2629 / 0.99) < 2e-3
    assert abs(f["peak_f"] - 0.0809) < 2e-4 and abs(f["f_3db"] - 0.1199) < 2e-4
    assert abs(f["nominal"] - math.atan(0.3) / math.pi) < 1e-12
    assert abs(m.lp4_gain(0.3, 0.70711, 0.0) - 1.0) < 1e-9  # DC gain is cin


def test_resonance_is_one_over_p():
    g, k = m.section_gk(0.3, 0.0)  # P = 0: undamped, the peak is huge
    assert k == 0.0 and m.lp4_features(0.3, 0.0)["peak_gain"] > 1e6
    assert m.lp4_features(0.3, 2.0)["peak_gain"] <= 1.0 + 1e-9  # P = 2: critically damped
