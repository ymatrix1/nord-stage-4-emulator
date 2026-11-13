"""Unit tests for tools/fx_contexts.py with a synthetic descriptor list (no firmware)."""

from __future__ import annotations

import fx_contexts as fc
import pytest

LIST = 0xC0279000
STATE_TABLE = 0xC0546000
CODE_TABLE = 0xC0546400
SYS1 = 0x28000000


def memory(rows: list[tuple[int, int, int]], code_base: int | None = None) -> dict[int, int]:
    """A descriptor list `{table, 0, 1, rows}` over one state table (and one code table)."""
    mem: dict[int, int] = {}
    entries = [(STATE_TABLE, len(rows))]
    for k, (target, base, size) in enumerate(rows):
        for j, w in enumerate((target, base, size, 0)):
            mem[STATE_TABLE + 16 * k + 4 * j] = w
    if code_base is not None:
        entries.append((CODE_TABLE, 1))
        for j, w in enumerate((0, code_base, 3, 0)):
            mem[CODE_TABLE + 4 * j] = w
    for n, (table, count) in enumerate(entries):
        for j, w in enumerate((table, 0, 1, count)):
            mem[LIST + 16 * n + 4 * j] = w
    return mem


def reader(mem: dict[int, int]):
    return lambda addr: mem.get(addr, 0)


def test_blocks_reads_state_rows_with_stride_and_core() -> None:
    # three rows of a 54-word context (size 0x353), stride 54 words, one on core 2
    rows = [
        (0, SYS1 + 4 * 0xE0000, 0x353),
        (0, SYS1 + 4 * (0xE0000 + 54), 0x353),
        (1, 0x28800000 + 4 * 0x500, 0x353),
    ]
    found = fc.blocks(reader(memory(rows)))
    assert [b.core for b in found] == [1, 1, 2]
    assert [b.word0 for b in found] == [0xE0000, 0xE0000 + 54, 0x500]
    assert found[0].row == 0 and found[2].row == 2
    assert all(b.words == 54 for b in found[:2])


def test_blocks_skips_tables_that_hold_code() -> None:
    code_base = SYS1 + 4 * 0x1206B0  # the delay routine
    mem = memory([(0, SYS1 + 4 * 0xE0000, 0x353)], code_base=code_base)
    found = fc.blocks(reader(mem))
    assert len(found) == 1 and found[0].table == STATE_TABLE


def test_locate_gives_context_offset() -> None:
    rows = [(0, SYS1 + 4 * 0xE0000, 0x353), (0, SYS1 + 4 * (0xE0000 + 54), 0x353)]
    table = fc.blocks(reader(memory(rows)))
    b = fc.locate(table, 1, 0xE0000 + 0x20 + 5)
    assert b is not None and b.row == 0 and b.offset(0xE0000 + 0x25) == 5
    second = fc.locate(table, 1, 0xE0000 + 54 + 0x20 - 2)
    assert second is not None and second.row == 1
    assert second.offset(0xE0000 + 54 + 0x20 - 2) == -2
    assert fc.locate(table, 2, 0xE0000 + 5) is None
    assert fc.locate(table, 1, 0xF0000) is None


def test_words_of_size_matches_known_tables() -> None:
    assert fc.words_of_size(0x353) == 54  # table LFO
    assert fc.words_of_size(0x183) == 25
    assert fc.words_of_size(0x463) == 71  # delay: one word more than the 70-word stride


def test_f32_decodes_ieee_words() -> None:
    assert fc.f32(0x3F800000) == 1.0
    assert fc.f32(0xBF800000) == -1.0


def test_join_rows_appends_block_and_offset() -> None:
    rows = [(0, SYS1 + 4 * 0xE0000, 0x353), (0, SYS1 + 4 * (0xE0000 + 54), 0x353)]
    table = fc.blocks(reader(memory(rows)))
    capture = [
        {"core": "1", "dm_word": hex(0xE0000 + 0x20 + 3)},
        {"core": "1", "dm_word": hex(0xE0000 + 4)},
        {"core": "1", "dm_word": "0x10"},
    ]
    joined = fc.join_rows(table, capture)
    assert joined[0]["ctx"] == "0x3" and joined[0]["row"] == "0"
    assert joined[1]["ctx"] == "-28" and joined[1]["row"] == "0"
    assert joined[2]["list"] == "" and joined[2]["ctx"] == ""


def test_read_capture_roundtrip(tmp_path) -> None:
    path = tmp_path / "c.csv"
    path.write_text("id,core,dm_word\n0x1,1,0x10\n", encoding="utf-8")
    assert fc.read_capture(path) == [{"id": "0x1", "core": "1", "dm_word": "0x10"}]


@pytest.mark.parametrize("size,expected", [(0x83, 9), (0x20, 3)])
def test_words_of_size_small(size: int, expected: int) -> None:
    assert fc.words_of_size(size) == expected
