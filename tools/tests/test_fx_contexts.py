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
    assert joined[0]["ctx"] == "3" and joined[0]["row"] == "0"
    assert joined[1]["ctx"] == "-28" and joined[1]["row"] == "0"
    assert joined[2]["list"] == "" and joined[2]["ctx"] == ""


def test_read_capture_roundtrip(tmp_path) -> None:
    path = tmp_path / "c.csv"
    path.write_text("id,core,dm_word\n0x1,1,0x10\n", encoding="utf-8")
    assert fc.read_capture(path) == [{"id": "0x1", "core": "1", "dm_word": "0x10"}]


@pytest.mark.parametrize("size,expected", [(0x83, 9), (0x20, 3)])
def test_words_of_size_small(size: int, expected: int) -> None:
    assert fc.words_of_size(size) == expected


def joined_row(**kw: str) -> dict[str, str]:
    base = {
        "kind": "organ",
        "slot": "Fx2",
        "type": "1",
        "id": "Amt",
        "stage": "param",
        "value": "0",
        "word": "0x3f800000",
        "list": "0xc0",
        "entry": "1",
        "ctx": "-5",
    }
    return {**base, **kw}


def test_type_modules_keeps_type_rows_and_last_word() -> None:
    rows = [
        joined_row(stage="type", word="0x1"),
        joined_row(stage="type", word="0x2"),
        joined_row(stage="type", ctx="3", entry="2"),
        joined_row(stage="param"),
    ]
    got = fc.type_modules(rows)
    assert got == {
        ("organ", "Fx2", "1"): {("0xc0", "1"): {"-5": "0x2"}, ("0xc0", "2"): {"3": "0x3f800000"}}
    }


def test_param_series_groups_values_and_skips_unjoined() -> None:
    rows = [
        joined_row(value="0", word="0x0"),
        joined_row(value="64", word="0x3f800000"),
        joined_row(value="64", word="0x40000000"),
        joined_row(list=""),
        joined_row(stage="type"),
    ]
    got = fc.param_series(rows)
    assert got == {("organ", "Fx2", "1", "Amt", "0xc0", "1", "-5"): {0: 0, 64: 0x40000000}}


def test_type_table_lists_pcs_and_word_counts(tmp_path) -> None:
    tm = {
        ("organ", "Fx2", "10"): {("0xc0", "1"): {"-5": "0x1"}},
        ("organ", "Fx2", "2"): {("0xc0", "1"): {"-5": "0x1", "3": "0x2"}, ("", ""): {"0": "0x0"}},
    }
    pcs = {"0xc0": {1: [0x123456], 2: [0x1206B0, 0x1208AF]}}
    rows = fc.type_table(tm, pcs)
    assert [r["type"] for r in rows] == ["2", "2", "10"]
    assert rows[1]["list"] == "0xc0" and rows[1]["words"] == "2"
    assert rows[1]["core1_pcs"] == "0x123456" and rows[1]["core2_pcs"] == "0x1206b0 0x1208af"
    assert rows[0]["list"] == "" and rows[0]["core2_pcs"] == ""
    out = tmp_path / "t.csv"
    fc.write_rows(out, rows)
    assert out.read_text(encoding="utf-8").splitlines()[0].startswith("kind,slot,type,list")


def test_fit_rows_describes_a_linear_word() -> None:
    values = [0, 16, 32, 64, 127]
    series = {
        ("organ", "Fx2", "1", "Amt", "0xc0", "1", "4"): {v: fc.f32_word(v / 127) for v in values}
    }
    (row,) = fc.fit_rows(series)
    assert row["form"] == "linear" and row["float"] == "1" and row["n"] == "5"
    assert float(row["b"]) == pytest.approx(1 / 127, rel=1e-4)
