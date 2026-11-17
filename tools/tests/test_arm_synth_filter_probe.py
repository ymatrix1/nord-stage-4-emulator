"""Tests for tools/arm_synth_filter_probe.py (pure helpers; capture needs the local firmware)."""

from pathlib import Path
from types import SimpleNamespace

import arm_synth_filter_probe as m
import pytest


def msg(dm: int, value: int, path: int = 2, chip: str = "ORG", core: int = 2) -> SimpleNamespace:
    return SimpleNamespace(path=path, chip=chip, core=core, dm_word=dm, word2=value)


def test_strip_word_maps_strips_and_common_blocks() -> None:
    assert m.strip_word(0xE0000) == ("strip", 0, 0)
    assert m.strip_word(0xE0000 + 62 * 2 + 1) == ("strip", 2, 1)
    assert m.strip_word(0xE02E8 + 32 + 7) == ("common", 1, 7)
    assert m.strip_word(0xE02E8 + 96) is None
    assert m.strip_word(0x1000) is None


def test_last_words_keeps_the_last_org_core2_value_only() -> None:
    rows = [
        (0, msg(0xE0001, 1)),
        (1, msg(0xE0001, 2)),
        (1, msg(0xE0002, 9, chip="PNO")),
        (2, msg(0xE0003, 5, path=1)),
        msg(0xE0004, 7),
    ]
    assert m.last_words(rows) == {0xE0001: 2, 0xE0004: 7}


def test_layer_words_names_strip_and_common_words() -> None:
    words = {0xE0000 + 1: 0x61, 0xE0000 + 62 * 6 + 3: 4, 0xE02E8 + 32 * 1 + 7: 0x3F000000}
    assert m.layer_words(words, 0) == {"s0w1": 0x61, "s6w3": 4}
    assert m.layer_words(words, 1) == {"c7": 0x3F000000}
    assert m.layer_words({}, 2) == {}


def test_diff_words_reports_old_and_new() -> None:
    base = {"s0w1": 1, "c1": 2}
    now = {"s0w1": 1, "c1": 3, "c7": 4}
    assert m.diff_words(base, now) == {"c1": (2, 3), "c7": (None, 4)}


def test_param_ids_reads_the_edit_table() -> None:
    table = m.param_ids(Path("docs/research/arm-edit-params.csv"))
    ident, lo, hi = table["Edt_Syn_kFltType"]
    assert isinstance(ident, int) and lo <= hi


def test_parse_point_and_errors() -> None:
    assert m.parse_point("A=1, B = 127") == [("A", 1), ("B", 127)]
    with pytest.raises(ValueError):
        m.parse_point("A")


def test_q16_and_words_json() -> None:
    assert m.q16(1) == 0x10000
    assert m.q16(127) == 0x7F0000
    assert m.words_json({"c1": 0x3F000000}) == {"c1": "0x3f000000"}
