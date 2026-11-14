"""Unit tests for tools/sharc_routine_io.py (synthetic listing, no firmware)."""

from __future__ import annotations

import sharc_routine_io as sio

# Listing addresses are pc - 0x120200. Routine A: 0x120200..0x120205, routine B from 0x120206.
LISTING = """
_fir:
00000000   1120 000b 33ba   dm(0xb33ba)=m0;
00000003   1001 000b 33ba   r1=dm(0xb33ba);
00000006   0f1c 0009 55b6   i12=0x955b6;
00000009   0f0e 3f80 0000   r14=0x3f800000;
0000000c   0afe   rts (db);
0000000d   1001 000b 33bc   r1=dm(0xb33bc);
00000010   1100 000b 33ba   dm(0xb33ba)=r0;
00000013   0f0e 0001 2345   r14=0x00012345;
00000016   0a3e 0000 0000   rts;
"""


def test_parse_listing_adds_the_listing_base() -> None:
    lines = sio.parse_listing(LISTING)
    assert lines[0] == (0x120200, "dm(0xb33ba)=m0;")
    assert lines[-1][0] == 0x120216


def test_scan_separates_reads_from_writes_and_ignores_other_routines() -> None:
    lines = sio.parse_listing(LISTING)
    a = sio.scan(lines, 0x120200, 0x12020C)
    assert a.writes == {0xB33BA}
    assert a.reads == {0xB33BA}
    assert a.pointers == {0x955B6}
    assert a.consts == {1.0}
    assert a.instructions == 5


def test_float_filter_drops_integers_that_are_not_float_like() -> None:
    assert sio.plausible_float(0x3F800000)
    assert not sio.plausible_float(0x00012345)
    assert sio.as_float(0x3F800000) == 1.0


def test_routines_split_at_the_next_entry_and_use_explicit_ends() -> None:
    lines = sio.parse_listing(LISTING)
    out = sio.routines(lines, [0x120200, 0x12020D], {0x120200: 0x12020C})
    assert out[0x120200].end == 0x12020C
    assert out[0x12020D].reads == {0xB33BC}
    assert out[0x12020D].writes == {0xB33BA}
    assert out[0x12020D].consts == set()


def test_shared_words_show_the_producer_consumer_direction() -> None:
    lines = sio.parse_listing(LISTING)
    out = sio.routines(lines, [0x120200, 0x12020D], {0x120200: 0x12020C})
    s = sio.shared(out[0x120200], out[0x12020D])
    assert s["b_to_a"] == [0xB33BA]
    assert s["a_to_b"] == []
    assert s["both_write"] == [0xB33BA]


def test_ranges_collapse_consecutive_words() -> None:
    assert sio.ranges([5, 1, 2, 3, 9]) == "0x1-0x3 0x5 0x9"
    assert sio.ranges([]) == "-"


def test_report_lists_the_four_sections() -> None:
    lines = sio.parse_listing(LISTING)
    text = sio.report(sio.scan(lines, 0x120200, 0x12020C))
    assert "reads" in text and "writes" in text and "pointers" in text and "consts" in text
