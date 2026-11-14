"""Unit tests for tools/arm_sy_voice.py with a word-memory fake (no firmware needed)."""

from __future__ import annotations

import arm_sy_voice as sv
import pytest


class FakeEmu:
    """Word memory addressed by byte address."""

    def __init__(self, words: dict[int, int] | None = None) -> None:
        self.words = dict(words or {})

    def read32(self, address: int) -> int:
        return self.words.get(address, 0)

    def write32(self, address: int, value: int) -> None:
        self.words[address] = value & 0xFFFFFFFF


def queue_with(events: list[int], head: int = 0) -> FakeEmu:
    emu = FakeEmu({sv.HANDLER + sv.QUEUE_COUNT: len(events), sv.HANDLER + sv.QUEUE_HEAD: head})
    for i, event in enumerate(events):
        emu.write32(sv.HANDLER + sv.QUEUE_ITEMS + 4 * ((head + i) % sv.QUEUE_SIZE), event)
    return emu


def test_queued_events_lists_the_ring_head_first() -> None:
    emu = queue_with([0xC05697C4, 0xC05697D4])
    assert sv.queued_events(emu) == [0xC05697C4, 0xC05697D4]  # type: ignore[arg-type]


def test_queued_events_wraps_around_the_ring() -> None:
    emu = queue_with([11, 22], head=sv.QUEUE_SIZE - 1)
    assert sv.queued_events(emu) == [11, 22]  # type: ignore[arg-type]


def test_pop_event_advances_head_and_count() -> None:
    emu = queue_with([11, 22], head=sv.QUEUE_SIZE - 1)
    assert sv.pop_event(emu) == 11  # type: ignore[arg-type]
    assert emu.read32(sv.HANDLER + sv.QUEUE_HEAD) == 0
    assert emu.read32(sv.HANDLER + sv.QUEUE_COUNT) == 1
    assert sv.queued_events(emu) == [22]  # type: ignore[arg-type]


def test_ack_address_follows_the_listing_formula() -> None:
    emu = FakeEmu({0x1004: 3, 0x1008: 0x4})
    assert sv.ack_address(0x1000, 0xC05B0454, emu.read32) == (0xC05B0454 + 12, 4)


def test_decode_messages_splits_by_role() -> None:
    pairs = [
        (0x92414, 0x1C0794),
        (0x92420, 0x3F128F5C),
        (0x92421, 0xBEDAE148),
        (0x9240A, 0xC),
        (0x9240B, 0x2C60),
        (0x92413, 7),
        (0x92414, 0x1C07CD),
        (0x9243B, 0xE4C00),
        (0x921A6, 0x3C748500),
        (0xC1DCC, 0xB006200),
    ]
    out = sv.decode_messages(pairs)
    assert out.reset and out.prepared
    assert out.kernel_base == 0xE4C00 and out.kernel == 2
    assert out.pitch_bits == 0x3C748500
    assert out.gains == (0x3F128F5C, 0xBEDAE148)
    assert out.fields == {0x0A: 0xC, 0x0B: 0x2C60, 0x13: 7}


def test_decode_messages_without_a_kernel_write_has_no_kernel() -> None:
    out = sv.decode_messages([(0x92414, 0x1C07CD)])
    assert out.kernel is None and not out.reset and out.prepared


def test_kernel_of_a_foreign_base_is_rejected() -> None:
    out = sv.decode_messages([(sv.KERNEL_WORD, 0xE4001)])
    with pytest.raises(ValueError):
        _ = out.kernel


def test_signed_reads_two_complement_words() -> None:
    assert sv._signed(0xFFFFFB4E) == -1202
    assert sv._signed(60) == 60


def test_pitch_fields_and_layer_fields_match_the_listing() -> None:
    assert sv.PITCH_FIELDS["key"] == 0x08 and sv.PITCH_FIELDS["transpose"] == 0x18
    assert sv.LAYER_FIELDS == {"tune": 0x10, "fixed_key": 0x60, "mode": 0x6C}
