"""Unit tests for tools/organ_bass_gen.py (generator weight rule; no ARM harness)."""

from __future__ import annotations

from collections import Counter
from pathlib import Path

import organ_bass_gen as bg

CSV = Path(__file__).resolve().parents[2] / "docs" / "research" / "data" / "bass-gen-weights.csv"


def test_note_of_maps_the_generator_weight_words() -> None:
    assert bg.note_of(0x95122) == 36
    assert bg.note_of(0x9513A) == 60
    assert bg.note_of(0x95170) == 114
    assert bg.note_of(0x95171) is None
    assert bg.note_of(0x951F1) is None


def test_table_is_indexed_by_the_number_of_earlier_writes() -> None:
    per_key = {
        60: [(79, 0x3EE6A0C0), (79, 0x3EB4D5F0)],
        67: [(79, 0x3E94D400), (79, 0x3E78C000)],
    }
    table, counts = bg.build_tables(per_key)
    assert table[79] == [0x3EE6A0C0, 0x3EB4D5F0, 0x3E94D400, 0x3E78C000]
    assert counts[60] == Counter({79: 2})
    assert bg.replay([60], table, counts) == {79: 0x3EB4D5F0}
    assert bg.replay([67, 60], table, counts) == bg.replay([60, 67], table, counts)
    assert bg.replay([60, 67], table, counts) == {79: 0x3E78C000}


def test_csv_round_trips() -> None:
    per_key = {60: [(79, 0x3EE6A0C0)], 64: [(79, 0x3EB4D5F0), (83, 0x3F000000)]}
    table, counts = bg.build_tables(per_key)
    back = bg.parse_csv("\n".join(bg.csv_lines(table, counts)))
    assert back[0] == table
    assert back[1] == counts


def test_committed_table_reproduces_arm_chord_words() -> None:
    table, counts = bg.parse_csv(CSV.read_text())
    got = {n: bg.f32(b) for n, b in bg.replay([60, 64, 67], table, counts).items()}
    # ARM words after the key-down of 60, 64, 67 (real firmware, `cap2.py` replay of 297 writes)
    assert abs(got[79] - 0.24357) < 5e-6
    assert abs(got[88] - 0.43671) < 5e-6
    assert abs(got[91] - 0.5252) < 5e-5
    assert abs(got[67] - 0.27843) < 5e-6
    # the first write of a wheel is its amplitude A
    assert abs(bg.f32(table[60][0]) - 0.29402) < 5e-6
    # more keys never raise a wheel weight
    for note, row in table.items():
        vals = [bg.f32(b) for b in row]
        assert vals == sorted(vals, reverse=True), note
