"""Tests for tools/fx_type_map.py on synthetic memory (no firmware)."""

import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import fx_type_map as tm  # noqa: E402


class Mem:
    """Sparse little-endian word memory with a `read32` like the emulator's."""

    def __init__(self) -> None:
        self.words: dict[int, int] = {}

    def put(self, addr: int, *words: int) -> None:
        for i, w in enumerate(words):
            self.words[addr + 4 * i] = w

    def read32(self, addr: int) -> int:
        return self.words.get(addr, 0)


def core1_base(pc: int) -> int:
    return 0x28000000 + 4 * pc


def core2_base(pc: int) -> int:
    return 0x28800000 + 4 * pc


def synthetic() -> Mem:
    m = Mem()
    # code table at 0xC0545000: core 1 row, core 2 row, and a row with a target/core mismatch
    m.put(0xC0545000, 0, core1_base(0x123243), 3, 0)
    m.put(0xC0545010, 1, core2_base(0x1228CF), 3, 0)
    m.put(0xC0545020, 0, core2_base(0x1228CF), 3, 0)  # target 0 must name a core 1 address
    # state table at 0xC0548000: bases are not FX code addresses
    m.put(0xC0548000, 0, 0x282C2120, 0x202, 0x1000301)
    # descriptor list at 0xC02BB000: state then code, then a lone descriptor after a gap
    m.put(0xC02BB000, 0xC0548000, 0, 1, 12)
    m.put(0xC02BB010, 0xC0545000, 0, 1, 3)
    m.put(0xC02BB030, 0xC0545000, 0, 1, 2)
    return m


def test_find_descriptor_lists_groups_consecutive_entries():
    m = synthetic()
    lists = tm.find_descriptor_lists(m.read32, 0xC02BB000, 0xC02BB100)
    assert [[d.table for d in lst] for lst in lists] == [[0xC0548000, 0xC0545000], [0xC0545000]]
    assert lists[0][0].rows == 12 and lists[0][1].addr == 0xC02BB010


def test_find_descriptor_lists_rejects_bad_fields():
    m = Mem()
    m.put(0xC02BB000, 0xC0545000, 1, 1, 3)  # word 1 must be 0
    m.put(0xC02BB010, 0x00545000, 0, 1, 3)  # table outside the RAM range
    m.put(0xC02BB020, 0xC0545000, 0, 1, 99)  # too many rows
    m.put(0xC02BB030, 0xC0545000, 0, 1, 0)  # no rows
    assert tm.find_descriptor_lists(m.read32, 0xC02BB000, 0xC02BB100) == []


def test_code_rows_keep_only_fx_code_of_the_matching_core():
    m = synthetic()
    rows = tm.code_rows(m.read32, 0xC0545000, 3)
    assert [(r.target, r.core, r.pc) for r in rows] == [(0, 1, 0x123243), (1, 2, 0x1228CF)]
    assert tm.code_rows(m.read32, 0xC0548000, 1) == []


def test_slice_at_stops_at_the_first_non_descriptor():
    m = synthetic()
    sl = tm.slice_at(m.read32, 0xC02BB000)
    assert [d.table for d in sl] == [0xC0548000, 0xC0545000]
    assert tm.slice_at(m.read32, 0xC02BB010)[0].rows == 3
    assert tm.slice_at(m.read32, 0xC02BB020) == []


def test_inventory_rows_mark_state_and_code():
    m = synthetic()
    rows = tm.inventory_rows(m.read32, 0xC02BB000, 0xC02BB100)
    kinds = [(r["list_addr"], r["entry"], r["kind"]) for r in rows]
    assert kinds[0] == ("0xc02bb000", 0, "state")
    assert [k for k in kinds if k[2] == "code"] == [
        ("0xc02bb000", 1, "code"),
        ("0xc02bb000", 1, "code"),
        ("0xc02bb030", 0, "code"),
        ("0xc02bb030", 0, "code"),
    ]
    code = [r for r in rows if r["kind"] == "code"][0]
    assert code["pc"] == "0x123243" and code["core"] == 1 and code["rows"] == 3


def test_class_pcs_collects_sorted_unique_pcs_per_core():
    m = synthetic()
    rows = tm.inventory_rows(m.read32, 0xC02BB000, 0xC02BB100)
    pcs = tm.class_pcs(rows)
    assert pcs["0xc02bb000"] == {1: [0x123243], 2: [0x1228CF]}
    assert "0xc02bb100" not in pcs


def test_summarize_messages_collapses_zero_runs():
    msgs = [
        (0x282C2000, 0, 1),
        (0x282C2004, 0, 1),
        (0x282C2008, 0x3F800000, 0xC0204868),
        (0x282C200C, 0, 2),
    ]
    out = tm.summarize_messages(msgs)
    assert out == [
        "zeros x2 0x282c2000..0x282c2004",
        "0x282c2008 = 0x3f800000 (1) from 0xc0204868",
        "zeros x1 0x282c200c..0x282c200c",
    ]


def test_f32_decodes_words():
    assert tm.f32(0x3F800000) == 1.0
    assert tm.f32(struct.unpack("<I", struct.pack("<f", -2.5))[0]) == -2.5
