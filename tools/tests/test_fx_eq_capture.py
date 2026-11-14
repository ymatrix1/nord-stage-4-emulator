"""Unit tests for tools/fx_eq_capture.py (pure helpers, no firmware)."""

from __future__ import annotations

from types import SimpleNamespace

import fx_eq_capture as eq


def test_joint_grid_is_frequency_major_and_complete() -> None:
    grid = eq.joint_grid()
    assert len(grid) == len(eq.JOINT_FREQS) * len(eq.JOINT_GAINS)
    assert grid[0] == (eq.JOINT_FREQS[0], eq.JOINT_GAINS[0])
    assert grid[len(eq.JOINT_GAINS)][0] == eq.JOINT_FREQS[1]


def test_last_words_keeps_the_latest_word_per_address() -> None:
    msgs = [
        SimpleNamespace(dm_word=0x10, word2=1),
        SimpleNamespace(dm_word=0x11, word2=2),
        SimpleNamespace(dm_word=0x10, word2=3),
    ]
    assert eq.last_words(msgs) == {0x10: 3, 0x11: 2}


def test_rows_for_formats_and_sorts_the_words() -> None:
    rows = eq.rows_for(4, "piano", "mid", -30, 64, {0x20: 0x3F800000, 0x10: 0})
    assert [r["dm_word"] for r in rows] == ["0x10", "0x20"]
    assert rows[1]["word"] == "0x3f800000"
    assert rows[0]["mode"] == "mid" and rows[0]["gain"] == -30


def test_grids_cover_the_documented_ranges() -> None:
    assert min(eq.GAIN_RANGE) == -60 and max(eq.GAIN_RANGE) == 60
    assert len(eq.FREQ_RANGE) == 128
    assert set(eq.IDS) == {"treble", "bass", "mid", "freq"}
