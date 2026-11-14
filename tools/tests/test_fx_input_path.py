"""Tests for tools/fx_input_path.py (script and dump parsing, no oracle needed)."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import fx_input_path as t  # noqa: E402


def test_float_bits_round_trip():
    assert t.f32_bits(0.25) == 0x3E800000
    assert t.bits_f32(0x3E3F1412) == t.bits_f32(t.f32_bits(0.1866))
    assert abs(t.bits_f32(0x3E3F1412) - 0.1866) < 1e-7


def test_script_lines_poke_every_pass():
    text = t.script_lines(2, [(0xC0234, 0.25), (0xB6518, 1.0)])
    assert text.splitlines() == [
        f"0 {0xC0234} {0x3E800000}",
        f"0 {0xB6518} {0x3F800000}",
        f"1 {0xC0234} {0x3E800000}",
        f"1 {0xB6518} {0x3F800000}",
    ]


def test_parse_dump_reads_hex_words_by_address():
    got = t.parse_dump("booted\nDM(0xc04d8..): bd3ec000 80000000\nDM(0xc0430..): 40000000\n")
    assert got == {0xC04D8: [0xBD3EC000, 0x80000000], 0xC0430: [0x40000000]}


def test_gain_is_the_output_over_the_input():
    assert abs(t.gain(-0.04655, 0.25) + 0.1862) < 1e-9
