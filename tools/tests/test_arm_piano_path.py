"""Unit tests for tools/arm_piano_path.py (hand-assembled ARM words, no firmware)."""

from __future__ import annotations

import struct
from dataclasses import dataclass

import arm_piano_path as pp
import pytest

PUSH_LR = 0xE92D4010  # push {r4, lr}
BL_NEXT = 0xEB000000  # bl +8
BX_LR = 0xE12FFF1E


def image(ws: list[int], size: int = 0x400) -> bytes:
    buf = bytearray(size)
    for i, w in enumerate(ws):
        struct.pack_into("<I", buf, 4 * i, w)
    return bytes(buf)


def test_field_of_maps_record_offsets() -> None:
    assert pp.field_of(0x92848) == 0
    assert pp.field_of(0x92828) == -32  # step_int
    assert pp.field_of(0x92829) == -31  # step_frac
    assert pp.field_of(0x9285D) == 21
    assert pp.field_of(0x9285E) is None
    assert pp.field_of(0x92827) is None


def test_field_table_attributes_literal_to_enclosing_function() -> None:
    # fn A at +0 (push), literal 0x92829 at word 3; fn B at +16, literal 0x92848 at word 6
    ws = [PUSH_LR, BX_LR, 0, 0x92829, PUSH_LR, BX_LR, 0x92848, 0x12345]
    rows = pp.field_table(image(ws))
    assert rows == [
        (-31, pp.LOAD_BASE + 12, pp.LOAD_BASE),
        (0, pp.LOAD_BASE + 24, pp.LOAD_BASE + 16),
    ]


def test_scan_literals_ignores_words_outside_the_code_range() -> None:
    ws = [0x92834] * 3
    code = image(ws)
    assert len(pp.scan_literals(pp.words(code), 0x92830, 0x92840)[0x92834]) == 3
    assert pp.scan_literals(pp.words(code), 0x1, 0x2) == {}


def test_function_starts_and_enclosing() -> None:
    ws = [0, PUSH_LR, 0, PUSH_LR, 0]
    starts = pp.function_starts(ws)
    assert starts == [pp.LOAD_BASE + 4, pp.LOAD_BASE + 12]
    assert pp.enclosing(starts, pp.LOAD_BASE + 8) == pp.LOAD_BASE + 4
    assert pp.enclosing(starts, pp.LOAD_BASE) == 0


def test_synthetic_pitch_table_matches_formula_within_half_unit() -> None:
    t = pp.synthetic_pitch_table()
    assert len(t) == 1202
    assert t[0] == 0x1000000
    assert t[1200] == 0x800000
    assert t[1201] == 0
    assert pp.pitch_table_error(t) <= 0.5


def test_pitch_table_reads_the_words_at_the_table_address() -> None:
    t = pp.synthetic_pitch_table()
    off = pp.PITCH_TABLE - pp.LOAD_BASE
    buf = bytearray(off + 4 * len(t))
    struct.pack_into(f"<{len(t)}I", buf, off, *t)
    assert pp.pitch_table(bytes(buf)) == t


def test_step_frac_unity_octaves_and_cap() -> None:
    t = pp.synthetic_pitch_table()
    # 0 cents: 0x1000000 is capped to 0xFFFFFF, then << 8
    assert pp.step_frac(0, t) == 0xFFFFFF00
    # one octave down is table[1200] = 0.5 (no loop: n = 1200 is not > 1200)
    assert pp.step_frac(-1200, t) == 0x80000000
    # two octaves: n = 2400 -> 1200, one octave counted: 0x800000 >> 1
    assert pp.step_frac(-2400, t) == 0x40000000
    # 1201 cents: n = 1, one octave: table[1] >> 1
    assert pp.step_frac(-1201, t) == (t[1] >> 1) << 8
    # half an octave: ratio 2^-0.5
    ratio = pp.step_frac(-600, t) / 2**32
    assert ratio == pytest.approx(2**-0.5, abs=1e-6)


def test_step_frac_rejects_positive_cents() -> None:
    with pytest.raises(ValueError):
        pp.step_frac(1, pp.synthetic_pitch_table())


def test_fixed_key_note_range() -> None:
    assert pp.fixed_key_note(1) == 0x15  # A0
    assert pp.fixed_key_note(88) == 0x6C  # C8
    assert pp.fixed_key_note(89) == 0x6C  # clamped
    with pytest.raises(ValueError):
        pp.fixed_key_note(0)


@dataclass
class FakeStore:
    kind: str
    layer: int


def test_piano_stores_filters_by_kind() -> None:
    stores = [FakeStore("organ", 0), FakeStore("piano", 2), FakeStore("piano", 3)]
    assert [s.layer for s in pp.piano_stores(stores)] == [2, 3]


def test_return_address_detection_and_stack_scan() -> None:
    # word 0: bl, word 1: blx r3; return addresses are +4 and +8
    code = image([BL_NEXT, 0xE12FFF33, 0, 0])
    assert pp.is_return_address(code, pp.LOAD_BASE + 4)
    assert pp.is_return_address(code, pp.LOAD_BASE + 8)
    assert not pp.is_return_address(code, pp.LOAD_BASE + 12)
    assert not pp.is_return_address(code, pp.LOAD_BASE + 5)
    assert not pp.is_return_address(code, 0x1234)
    stack = {0x100: pp.LOAD_BASE + 8, 0x104: 7, 0x108: pp.LOAD_BASE + 4}
    got = pp.stack_return_candidates(code, lambda a: stack.get(a, 0), 0x100, 0x10)
    assert got == [(0, pp.LOAD_BASE + 4), (8, pp.LOAD_BASE)]
