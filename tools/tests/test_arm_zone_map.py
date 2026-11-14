"""Unit tests for tools/arm_zone_map.py (the pure model; the ARM check needs the firmware)."""

from __future__ import annotations

import arm_zone_map as zm

ALL = (True, True, True)


def test_digest_is_pinned_for_the_rust_table() -> None:
    # `engine/zone_table.rs` pins the same value (`digest_matches_the_arm_checked_python_model`).
    assert zm.digest() == 0x32825B100986BEB1


def test_case_table_shape_and_selector_column() -> None:
    assert len(zm.CASE_TABLE) == 8
    for rows in zm.CASE_TABLE:
        assert len(rows) == 12
        for code, (a, b) in enumerate(rows):
            assert 0 <= a <= 10 and 0 <= b <= 10
            if code != 11:
                assert a == b


def test_no_split_gives_the_whole_keybed_for_every_code() -> None:
    for code in range(11):
        lo, up = zm.zone_range(code, False, ALL, (1, 4, 7), (0, 0, 0))
        assert (lo[0], up[0]) == (21, 108)


def test_mid_only_split_halves() -> None:
    flags = (False, True, False)
    lo = zm.zone_range(0, True, flags, (1, 4, 7), (0, 0, 0))
    hi = zm.zone_range(6, True, flags, (1, 4, 7), (0, 0, 0))
    assert (lo[0][0], lo[1][0]) == (21, 59)
    assert (hi[0][0], hi[1][0]) == (60, 108)


def test_four_zone_codes_cover_the_documented_zones() -> None:
    def span(code: int) -> tuple[int, int]:
        lo, up = zm.zone_range(code, True, ALL, (1, 4, 7), (0, 0, 0))
        return lo[0], up[0]

    assert span(0) == (21, 40)  # zone 1
    assert span(1) == (41, 59)  # zone 2
    assert span(9) == (60, 76)  # zone 3
    assert span(10) == (77, 108)  # zone 4
    assert span(2) == (21, 59)  # zones 1-2
    assert span(3) == (41, 76)  # zones 2-3
    assert span(4) == (21, 76)  # zones 1-3
    assert span(6) == (41, 108)  # zones 2-4
    assert span(8) == (60, 108)  # zones 3-4
    assert span(5) == (21, 108)


def test_code_eleven_selector_one_is_empty() -> None:
    lo, up = zm.zone_range(11, True, ALL, (1, 4, 7), (0, 0, 0), sel=1)
    assert (lo[0], up[0]) == (0, 1)
    assert zm.note_gain(lo, up, 60) == 0


def test_split_index_ports_the_neighbour_limits() -> None:
    assert [zm.split_index(ALL, (0, 0, 0), k) for k in range(3)] == [0, 1, 2]
    assert zm.split_index(ALL, (9, 3, 1), 0) == 2
    assert zm.split_index((False, True, False), (9, 3, 1), 1) == 3
    assert zm.split_index((True, False, True), (4, 0, 4), 2) == 5


def test_crossfade_gains_follow_the_firmware_curves() -> None:
    flags = (False, True, False)
    lo = zm.zone_range(0, True, flags, (1, 4, 7), (0, 1, 0))
    hi = zm.zone_range(6, True, flags, (1, 4, 7), (0, 1, 0))
    assert zm.note_gain(*lo, 53) == zm.UNITY and zm.note_gain(*hi, 53) == 0
    assert zm.note_gain(*lo, 54) == zm.FADE_12[11]
    assert zm.note_gain(*hi, 54) == zm.FADE_12[0]
    assert zm.note_gain(*hi, 60) == 0x8000
    assert zm.note_gain(*hi, 66) == zm.UNITY and zm.note_gain(*lo, 66) == 0


def test_fade_curves_rise() -> None:
    assert list(zm.FADE_12) == sorted(zm.FADE_12)
    assert list(zm.FADE_24) == sorted(zm.FADE_24)
    assert zm.fade(0, 0) == zm.UNITY


def test_note_table_has_eleven_settings() -> None:
    assert len(zm.SPLIT_NOTES) == 11 and zm.SPLIT_NOTES[4] == 60
