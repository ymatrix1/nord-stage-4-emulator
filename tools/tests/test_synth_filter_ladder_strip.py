"""Tests for the end-to-end Ladder24 cutoff check (recorded cases, no firmware needed)."""

from __future__ import annotations

import json
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import synth_filter_ladder_strip as strip  # noqa: E402

CASES = Path(__file__).resolve().parents[2] / "docs/research/data/synth-ladder-strip-cases.json"


def _load() -> list[dict]:
    return json.loads(CASES.read_text(encoding="utf-8"))["cases"]


def test_recorded_cases_match_the_model_on_the_pass_control_value() -> None:
    cases = _load()
    assert [c["word"] for c in cases] == list(strip.WORDS)
    assert strip.mismatches(cases) == 0


def test_recorded_cutoff_words_are_the_strip_table_values() -> None:
    """DM(-30) is `R` of the strip table: 0.36083 at word 0, rising with the word."""
    rs = [struct.unpack("<f", struct.pack("<I", c["lanes"][0]["r"]))[0] for c in _load()]
    assert abs(rs[0] - 0.36083) < 1e-5
    assert rs == sorted(rs)
    assert all(c["lanes"][0]["r"] == c["lanes"][1]["r"] for c in _load())


def test_a_wrong_coefficient_word_is_counted() -> None:
    cases = _load()
    cases[0]["lanes"][1]["coefs"][2] ^= 1
    assert strip.mismatches(cases) == 1
