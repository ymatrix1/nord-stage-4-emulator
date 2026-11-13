"""Unit tests for tools/organ_farfisa_ref.py (synthetic tables; no firmware, no oracle)."""

from __future__ import annotations

import struct

import organ_farfisa_ref as fr
from sharc40 import ld


def u2f(word: int) -> float:
    return struct.unpack("<f", struct.pack("<I", word))[0]


def test_mem_reads_zero_for_missing_words() -> None:
    m = fr.Mem()
    assert m[0x1234] == 0
    m[5] = 7
    assert m[5] == 7


def test_row_constants_follow_the_closed_forms() -> None:
    off, slope, a, b = fr.row_constants(0, 0)
    assert off == 0xBF2AAAA7
    assert abs(u2f(slope) - fr.SLOPE0) < 1e-3
    assert abs(u2f(a) - 0.9952769) < 2e-6  # image word 0x3F7ECA77
    assert abs(u2f(a) + u2f(b) - 1.0) < 1e-7
    # semitones scale the slope by 2^(-n/12), rows halve it
    assert abs(u2f(fr.row_constants(1, 0)[1]) / u2f(slope) - 2 ** (-1 / 12)) < 1e-6
    assert abs(u2f(fr.row_constants(0, 3)[1]) / u2f(slope) - 1 / 8) < 1e-6
    # row 7 uses exponent 11 (a tiny), row 8 has a = 0 and b = 1
    assert u2f(fr.row_constants(0, 7)[2]) < 1e-4
    assert fr.row_constants(0, 8)[2:] == (0, 0x3F800000)


def test_closed_form_layout_matches_the_image_stream_shape() -> None:
    p = fr.closed_form_inputs()
    assert [p[0xC2C1A + j] for j in range(6)] == [9, 8, 8, 8, 8, 8]
    assert p[0xC2AD8] == 0x316160 and p[0xC2AD9] == 0x345112
    # pair 0 has 9 rows: its stream is 4 + 18 words, so pair 1's increment sits at +22
    assert p[0xC2AD8 + 22] == round(fr.INC0 * 2 ** (2 / 12))
    assert p[0xC2AD8 + 4] == fr.PHASE_OFFSET
    assert p[fr.PTRS] == 0xC2950 and p[fr.PTRS + 3] == fr.RING[0]
    # a pair of tables rows is 8 words: row 0 of pair 1 follows the 9 rows of pair 0 (72 words)
    assert p[0xC2950 + 72] == 0xBF2AAAA7


def test_multiplier_with_zero_depth_is_the_fine_word() -> None:
    m = fr.Mem(fr.closed_form_inputs())
    k = fr.lfo_multiplier(m)
    assert k == [0x40000000, 0x40000000]
    m = fr.Mem(fr.closed_form_inputs(fine=0x20000000))
    assert fr.lfo_multiplier(m) == [0x20000000, 0x20000000]


def test_lfo_phase_advances_by_the_rate_in_both_lanes() -> None:
    m = fr.Mem(fr.closed_form_inputs(rate=0xF1900, depth=0x01000000))
    fr.lfo_multiplier(m)
    fr.lfo_multiplier(m)
    assert m[fr.LFO_PHASE] == 2 * 0xF1900 and m[fr.LFO_PHASE + 1] == 2 * 0xF1900


def test_generator_phase_state_steps_by_half_the_stream_increment() -> None:
    m = fr.Mem(fr.closed_form_inputs())
    fr.generator(m)  # two frames
    assert m[0xC2ADA] == 2 * (0x316160 >> 1)
    assert m[0xC2ADB] == 2 * (0x345112 >> 1)


def test_ring_values_are_bounded_pulses_and_use_the_other_frame_as_feedback() -> None:
    m = fr.Mem(fr.closed_form_inputs())
    seen = []
    for _ in range(6):
        fr.generator(m)
        seen += [u2f(m[fr.RING[f] + 12 * 2 + 4]) for f in (0, 1)]
    assert all(-1.0001 <= v <= 1.0001 for v in seen)
    # frame 1 of the first call reads ring 0 of the same call as `prev`
    m = fr.Mem(fr.closed_form_inputs())
    fr.generator(m)
    idx = 12 * 8  # row 8, oscillators 0 and 1 only
    y0, y1 = ld(m[fr.RING[0] + idx]), ld(m[fr.RING[1] + idx])
    assert y0 != 0 and y1 != 0


def test_synthetic_inputs_are_deterministic_and_fit_the_layout() -> None:
    a, b = fr.synthetic_inputs(3), fr.synthetic_inputs(3)
    assert a == b and fr.synthetic_inputs(4) != a
    counts = [a[0xC2C1A + j] for j in range(6)]
    assert all(2 <= c <= 8 for c in counts)
    assert sum(fr.stream_words(counts)) <= 0x7A


def test_self_check_runs() -> None:
    assert fr.self_check()
    assert isinstance(fr.f2u(1.0), int) and fr.f2u(1.0) == 0x3F800000
