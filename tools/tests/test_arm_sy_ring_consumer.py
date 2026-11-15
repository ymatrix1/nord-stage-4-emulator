"""Unit tests for tools/arm_sy_ring_consumer.py (pure helpers, no firmware needed)."""

from __future__ import annotations

from dataclasses import dataclass

import arm_sy_ring_consumer as rc


@dataclass
class _Msg:
    chip: str
    core: int
    dm_word: int
    word2: int


def test_in_window_bounds() -> None:
    assert rc.in_window(0x93500)
    assert rc.in_window(0x9354A)
    assert rc.in_window(0x9359F)
    assert not rc.in_window(0x934FF)
    assert not rc.in_window(0x935A0)


def test_filter_sy_smp_messages_keeps_only_sy_and_smp() -> None:
    msgs = [
        _Msg("ORG", 2, 0xC169A, 0x3F80_0000),
        _Msg("SY", 0, 0x9354A, 0x1234),
        _Msg("FX", 1, 0xC0000, 0),
        _Msg("SMP", 0, 0x92196, 0xABCD),
    ]
    rows = rc.filter_sy_smp_messages(msgs)
    assert rows == [
        ("SY", 0, 0x9354A, 0x1234),
        ("SMP", 0, 0x92196, 0xABCD),
    ]
    assert rc.count_window_hits(rows) == [("SY", 0, 0x9354A, 0x1234)]


def test_format_message_shows_chip_core_addr_and_value() -> None:
    assert rc.format_message("SY", 0, 0x9354A, 0x1234) == "  SY0 0x9354a <- 0x00001234"
