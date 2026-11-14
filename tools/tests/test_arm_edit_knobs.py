"""Tests for the Q16.16 knob capture helpers (no firmware needed)."""

from __future__ import annotations

import sys
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import arm_edit_capture as cap  # noqa: E402
import arm_edit_knobs as knobs  # noqa: E402


def msg(chip: str, core: int, dm: int, word: int) -> SimpleNamespace:
    return SimpleNamespace(chip=chip, core=core, path=2, dm_word=dm, word2=word)


def test_sample_values_are_dense_up_to_128_steps() -> None:
    assert knobs.sample_values(0, 127) == list(range(128))
    assert knobs.sample_values(-60, 60) == list(range(-60, 61))
    assert knobs.sample_values(0, 16383) == [0, 2047, 4095, 6143, 8191, 10239, 12287, 14335, 16383]


def test_small_ranges_are_tried_in_both_encodings() -> None:
    assert knobs.modes_for(cap.Param(1, "a", 0, 5)) == ["auto", "q16"]
    assert knobs.modes_for(cap.Param(1, "a", 0, 127)) == ["auto"]


def test_last_word_wins_and_equal_destinations_merge() -> None:
    msgs = [
        msg("ORG", 2, 0xE02EF, 1),
        msg("ORG", 2, 0xE02EF, 2),
        msg("PNO", 2, 0xE02EF, 2),
        msg("PNO", 1, 0xE0001, 7),
    ]
    words = knobs.last_words(msgs)
    assert words[(0xE02EF, "ORG/2/2")] == 2
    rows = knobs.fold_destinations(words)
    assert (0xE02EF, 2, "ORG/2/2 PNO/2/2") in rows
    assert (0xE0001, 7, "PNO/1/2") in rows


def test_shards_partition_the_items() -> None:
    items = list(range(10))
    parts = [knobs.shard_of(items, i, 3) for i in range(3)]
    assert sorted(x for p in parts for x in p) == items
    assert knobs.parse_shard("2/8") == (2, 8)
    for bad in ("8/8", "-1/4"):
        try:
            knobs.parse_shard(bad)
        except ValueError:
            continue
        raise AssertionError(bad)


def test_merge_sorts_rows_and_keeps_one_header() -> None:
    row = {k: "0" for k in knobs.FIELDS}
    a = dict(row, id="0x135", value="2", dm_word="0xe02ef", mode="auto")
    b = dict(row, id="0x133", value="10", dm_word="0xe0001", mode="auto")
    c = dict(row, id="0x133", value="9", dm_word="0xe0001", mode="auto")
    text = knobs.merge([knobs.csv_text([a]), knobs.csv_text([b, c])])
    lines = text.strip().splitlines()
    assert lines[0].startswith("id,name")
    keys = [ln.split(",")[0] + ":" + ln.split(",")[5] for ln in lines[1:]]
    assert keys == ["0x133:9", "0x133:10", "0x135:2"]
