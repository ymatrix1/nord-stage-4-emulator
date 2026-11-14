"""Unit tests for tools/arm_program_routing.py (decode side; the run needs the firmware)."""

from __future__ import annotations

import arm_program_routing as apr


def put(buf: bytearray, bit: int, width: int, value: int) -> None:
    for i in range(width):
        b = (value >> (width - 1 - i)) & 1
        pos = bit + i
        mask = 0x80 >> (pos % 8)
        if b:
            buf[pos // 8] |= mask
        else:
            buf[pos // 8] &= ~mask


def test_read_bits_is_msb_first() -> None:
    assert apr.read_bits(bytes([0b1010_0000]), 0, 4) == 0b1010
    assert apr.read_bits(bytes([0x00, 0xFF]), 4, 8) == 0b0000_1111


def test_scene_views_decode_section_and_layer_enables() -> None:
    payload = bytearray(820)
    # B3 Chords n Solo: scene I organ section on, layers A + B; scene II everything off.
    put(payload, apr.SECTION_ENABLE_I, 3, 0b100)
    put(payload, apr.ORGAN * 8 + 9, 2, 0b11)
    put(payload, apr.PIANO * 8 + 9, 2, 0b11)
    views = apr.scene_views(bytes(payload))
    assert views[0]["organ"] == (1, 0b11)
    assert views[0]["piano"] == (0, 0b11)
    assert views[0]["synth"] == (0, 0)
    assert views[1]["organ"] == (0, 0)


def test_synth_has_three_layer_bits_in_both_scenes() -> None:
    payload = bytearray(820)
    put(payload, apr.SECTION_ENABLE_II, 3, 0b001)
    put(payload, apr.SYNTH * 8 + 15, 3, 0b101)
    assert apr.scene_views(bytes(payload))[1]["synth"] == (1, 0b101)


def test_expected_zone_bytes_is_section_and_layer_bit() -> None:
    assert apr.expected_zone_bytes(1, 0b11, 2) == [1, 1]
    assert apr.expected_zone_bytes(1, 0b01, 2) == [1, 0]
    assert apr.expected_zone_bytes(1, 0b10, 2) == [0, 1]
    assert apr.expected_zone_bytes(0, 0b11, 2) == [0, 0]
    assert apr.expected_zone_bytes(1, 0b101, 3) == [1, 0, 1]
