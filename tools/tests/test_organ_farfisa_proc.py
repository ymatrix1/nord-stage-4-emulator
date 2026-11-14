"""Unit tests for tools/organ_farfisa_proc.py (synthetic signals; no oracle)."""

from __future__ import annotations

import numpy as np
import organ_farfisa_proc as fp


def test_line_gains_recover_a_known_gain_at_the_harmonics() -> None:
    n = 8192
    f0_bins = 40  # exact bin so the window leaks nothing
    t = np.arange(n)
    ring = np.sin(2 * np.pi * f0_bins * t / n) + 0.5 * np.sin(2 * np.pi * 2 * f0_bins * t / n)
    out = 2.0 * np.sin(2 * np.pi * f0_bins * t / n) + 0.5 * np.sin(2 * np.pi * 2 * f0_bins * t / n)
    rows = fp.line_gains(ring, out)
    assert len(rows) == 2
    assert abs(rows[0][1] - 20 * np.log10(2.0)) < 0.05
    assert abs(rows[1][1]) < 0.05
    assert abs(rows[0][0] - f0_bins * fp.RATE / n) < 1e-6


def test_bin_table_takes_medians_per_sixth_octave_and_reports_spread() -> None:
    rows = [(100.0, -1.0), (100.5, -3.0), (101.0, -2.0), (400.0, 5.0)]
    table = fp.bin_table(rows)
    assert len(table) == 2
    assert abs(table[0][1] + 2.0) < 1e-9 and table[0][2] > 0
    assert table[1][1] == 5.0 and table[1][2] == 0.0


def test_rust_table_formats_both_sums() -> None:
    rows = [
        {"sum": "0", "hz": "15.89", "gain_db": "-8.63", "spread_db": "0"},
        {"sum": "1", "hz": "26.49", "gain_db": "-7.40", "spread_db": "0"},
    ]
    text = fp.rust_table(rows)
    assert "SUM0_DB: [(f32, f32); 1]" in text and "(15.89, -8.63)," in text
    assert "SUM1_DB: [(f32, f32); 1]" in text and "(26.49, -7.40)," in text


def test_f32_bits_of_one() -> None:
    assert fp.f32_bits(1.0) == 0x3F800000
