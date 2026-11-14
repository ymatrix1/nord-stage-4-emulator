"""Unit tests for tools/arm_zone_programs.py (decode and model side; the run needs the firmware)."""

from __future__ import annotations

import arm_zone_map as zm
import arm_zone_programs as zp


def put(buf: bytearray, bit: int, width: int, value: int) -> None:
    for i in range(width):
        b = (value >> (width - 1 - i)) & 1
        pos = bit + i
        mask = 0x80 >> (pos % 8)
        if b:
            buf[pos // 8] |= mask
        else:
            buf[pos // 8] &= ~mask


def b3_chords_n_solo() -> bytes:
    payload = bytearray(820)
    put(payload, 8, 1, 1)  # split enabled
    put(payload, 10, 1, 1)  # Mid point
    for i, v in enumerate((1, 4, 7)):
        put(payload, 12 + 4 * i, 4, v)
    sec, blk, fld = zp.CODE_FIELDS[1]
    put(payload, sec * 8 + blk + fld, 4, 6)
    return bytes(payload)


def test_decode_reads_split_state_and_layer_codes() -> None:
    state = zp.decode_zone_state(b3_chords_n_solo())
    assert state["enabled"] is True
    assert state["flags"] == (False, True, False)
    assert state["pos"] == (1, 4, 7)
    assert state["widths"] == (0, 0, 0)
    assert state["codes"] == (0, 6, 0, 0, 0, 0, 0)


def test_model_ranges_split_the_organ_layers_at_the_mid_key() -> None:
    ranges = zp.model_ranges(zp.decode_zone_state(b3_chords_n_solo()))
    assert (ranges[0][0][0], ranges[0][1][0]) == (21, 59)
    assert (ranges[1][0][0], ranges[1][1][0]) == (60, 108)


def test_compare_accepts_a_matching_zone_object_and_rejects_a_wrong_one() -> None:
    want = zm.zone_range(6, True, (False, True, False), (1, 4, 7), (0, 1, 0))
    gains = [
        (zm.bound_gain(want[0], False, n), zm.bound_gain(want[1], True, n)) for n in range(21, 109)
    ]
    got = (
        (want[0][0], zm.FADE_KEYS[want[0][1]], 0, want[0][2]),
        (want[1][0], zm.FADE_KEYS[want[1][1]], 0, want[1][2]),
        gains,
    )
    assert zp.compare(want, got)
    bad = (got[0], (got[1][0] + 1, *got[1][1:]), gains)
    assert not zp.compare(want, bad)


def test_layer_families_cover_the_seven_layers() -> None:
    assert len(zp.LAYER_FAMILY) == len(zp.CODE_FIELDS) == 7
    assert set(zp.LAYER_FAMILY) == set(zp.KBZONE_ID)
