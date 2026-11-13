"""Unit tests for tools/organ_b3_proc.py (synthetic words and lanes; no oracle)."""

from __future__ import annotations

import struct

import numpy as np
import organ_b3_proc as p
import pytest


def bits(v: float) -> int:
    return struct.unpack("<I", struct.pack("<f", v))[0]


def words(notes: dict[int, tuple[float, float]]) -> dict[int, int]:
    out = {}
    for n, (a, w) in notes.items():
        out[p.A_BASE + n - p.FIRST_NOTE] = bits(a)
        out[p.W_BASE + n - p.FIRST_NOTE] = bits(w)
    return out


def test_final_words_keep_the_last_write_and_ignore_comments() -> None:
    s = "0 0x10 0x1 # a\n0 0x10 0x2\n\n3 0x11 0x5\n"
    assert p.final_words(s) == {0x10: 2, 0x11: 5}


def test_note_gain_and_active_notes() -> None:
    w = words({60: (0.3, 0.03), 72: (0.4, 0.0)})
    assert p.note_gain(w, 60) == pytest.approx(0.009, rel=1e-6)
    assert p.active_notes(w) == [60]


def test_clean_subset_drops_notes_whose_lines_collide() -> None:
    # 48 is the octave of 60: h2 of 48 is the fundamental of 60
    assert p.clean_subset([48, 60, 64], 16384) == [48, 64]
    assert p.clean_subset([60], 16384) == [60]


def test_override_zeroes_everything_but_the_kept_notes() -> None:
    w = words({60: (0.3, 0.03), 64: (0.3, 0.02)})
    lines = p.override(w, [60]).splitlines()
    assert len(lines) == p.W_COUNT
    vals = {int(ln.split()[1], 16): int(ln.split()[2], 16) for ln in lines}
    assert vals[p.W_BASE + 60 - 36] == bits(0.03)
    assert vals[p.W_BASE + 64 - 36] == 0


def test_measure_recovers_a_known_gain_per_harmonic() -> None:
    w = words({60: (0.3, 0.03)})
    n = 24000
    t = np.arange(n + 4000)
    x = np.zeros(len(t))
    for h in (1, 2):
        pred = 0.3 * 0.03 * p.AMP[2][h - 1]
        x += 0.5 * pred * np.sin(2 * np.pi * p.line_cycles(60, h) * t)  # -6.02 dB
    rows = p.measure(w, [60], x, 4000, 16384)
    assert [r[1] for r in rows][:2] == [1, 2]
    assert rows[0][5] == pytest.approx(-6.02, abs=0.1)


def test_merge_table_bins_drops_weak_lines_and_rejects_outliers() -> None:
    f = [100.0 * 2 ** (i / 6) for i in range(8)]  # half-semitone... one bin per line
    rows = [(0, 0, hz, 1e-3, 0, -2.0 * i) for i, hz in enumerate(f)]
    rows[3] = (0, 0, f[3], 1e-3, 0, +9.0)  # outlier
    rows.append((0, 0, 500.0, 1e-6, 0, 50.0))  # too weak
    out = p.merge_table(rows)
    assert len(out) == 7 and all(abs(g) < 15 for _, g in out)
    assert all(hz != f[3] for hz, _ in out)


def test_wheel_classes_follow_the_firmware_tables() -> None:
    assert [p.wheel_class(n) for n in (36, 39, 40, 47, 48, 112)] == [0, 0, 1, 1, 2, 2]
