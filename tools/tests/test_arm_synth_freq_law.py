"""Tests for the recorded knob -> cutoff word law (no firmware needed)."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import arm_synth_freq_law as law  # noqa: E402

DATA = Path(__file__).resolve().parents[2] / "docs/research/data/synth-filter-freq-law.csv"


def test_word_index_is_the_signed_top_byte() -> None:
    assert law.word_index(0xCC000000) == -52
    assert law.word_index(0x0C000000) == 12
    assert law.word_index(0x6A000000) == 106


def test_recorded_law_is_knob_minus_52_up_to_117() -> None:
    rec = law.read_law(DATA.read_text(encoding="utf-8"))
    assert sorted(rec) == list(range(128))
    for knob in range(118):
        assert rec[knob] == ((knob - law.KNOB_OFFSET) & 0xFF) << 24
    assert rec[127] == 0x6A000000  # clipped at the pass limit


def test_law_csv_round_trips() -> None:
    rows = [(0, 0xCC000000), (1, 0xCD000000)]
    assert law.read_law(law.law_csv(rows)) == dict(rows)
