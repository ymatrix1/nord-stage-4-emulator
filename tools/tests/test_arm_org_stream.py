"""Unit tests for tools/arm_org_stream.py (message selection, pass mapping, timing statistics)."""

from __future__ import annotations

import arm_emu as ae
import arm_org_stream as aos
import pytest

CORE2_BASE = 0x28800000
CORE1_BASE = 0x28000000


def msg(comm: int, dm_word: int, value: int, core: int = 2, path: int = 2) -> ae.Message:
    base = CORE2_BASE if core == 2 else CORE1_BASE
    word1 = base + 4 * dm_word if path == 2 else dm_word
    return ae.Message(path=path, comm=comm, word1=word1, word2=value, pc=0xC0001000)


def test_pass_period_and_passes_per_tick() -> None:
    assert aos.pass_period_s() * 1e6 == pytest.approx(46.08, abs=0.01)
    assert aos.passes_per_tick() == pytest.approx(21.70, abs=0.01)


def test_tick_to_pass_floors_and_offsets() -> None:
    assert aos.tick_to_pass(0) == 0
    assert aos.tick_to_pass(1) == 21
    assert aos.tick_to_pass(10, start=100) == 100 + 217


def test_select_keeps_only_org_core2_path2_in_send_order() -> None:
    rows = [
        (0, msg(2, 0x10, 1)),
        (1, msg(1, 0x11, 2)),  # PNO
        (2, msg(2, 0x12, 3, core=1)),  # ORG core 1
        (3, msg(2, 0x13, 4, path=1)),  # path 1
        (4, msg(2, 0x14, 5)),
    ]
    got = aos.select(rows)
    assert [(t, m.dm_word) for t, m in got] == [(0, 0x10), (4, 0x14)]


def test_script_text_has_pass_word_value_and_comment() -> None:
    text = aos.script_text([(2, msg(2, 0xE0000, 0x120CD0))], start_pass=5)
    assert (
        text
        == f"{5 + 43} 0xe0000 0x00120cd0  # tick 2 {CORE2_BASE + 4 * 0xE0000:#010x} pc 0xc0001000\n"
    )
    assert aos.script_text([]) == ""


def test_timing_stats_drop_warmup_and_reject_empty() -> None:
    s = aos.timing_stats([9.0, 1.0, 2.0, 3.0], warmup=1)
    assert (s["n"], s["mean"], s["median"], s["worst"]) == (3, 2.0, 2.0, 3.0)
    with pytest.raises(ValueError):
        aos.timing_stats([1.0], warmup=1)


def test_per_pass_divides_by_passes_per_tick() -> None:
    assert aos.per_pass(0.0217) == pytest.approx(0.0217 / aos.passes_per_tick())


def test_count_by_target_names_cores_and_path1() -> None:
    rows = [
        (0, msg(2, 1, 1)),
        (0, msg(2, 2, 2)),
        (0, msg(1, 3, 3, core=1)),
        (0, msg(3, 4, 4, path=1)),
    ]
    assert aos.count_by_target(rows) == {"ORG core 2": 2, "PNO core 1": 1, "SY path 1": 1}


def test_targets_cover_every_path2_receiver() -> None:
    targets = [(t[1], t[2]) for t in aos.TARGETS]
    assert targets[:2] == [("ORG", 2), ("PNO", 1)]
    assert set(targets) == {(c, n) for c in ("ORG", "PNO", "FX") for n in (1, 2)}
    rows = [(0, msg(1, 5, 9, core=1)), (1, msg(2, 6, 7))]
    assert [m.dm_word for _t, m in aos.select(rows, "PNO", 1)] == [5]
