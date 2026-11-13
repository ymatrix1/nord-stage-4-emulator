"""Tests for tools/synth_wave_probe.py (pure helpers only; the grid run needs the ARM emulator)."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import synth_wave_probe as p  # noqa: E402


def test_fold_words_keeps_the_last_strip_word_and_skips_others() -> None:
    msgs = [(0xE0000, 0x061215FB), (0xE0001, 0x0612566E), (0xE0000, 0x06121183), (0xB00E6, 5)]
    assert p.fold_words(msgs) == {"w0": "0x06121183", "w1": "0x0612566e"}


def test_distinct_values_and_csv() -> None:
    rows = [
        {"mode": 0, "bank": 0, "cat": 0, "wave": 0, "w0": "0x061215fb"},
        {"mode": 1, "bank": 0, "cat": 3, "wave": 6, "w0": "0x061215fb", "w59": "0x02000000"},
        {"mode": 1, "bank": 1, "cat": 3, "wave": 6},
    ]
    assert p.distinct_values(rows) == {"0x061215fb"}
    text = p.rows_csv(rows)
    assert text.splitlines()[0] == "mode,bank,cat,wave,w0,w1,w59,w60"
    assert text.splitlines()[2].endswith("0x061215fb,,0x02000000,")
