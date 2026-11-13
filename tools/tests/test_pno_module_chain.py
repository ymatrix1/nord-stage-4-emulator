"""Tests for tools/pno_module_chain.py on synthetic listings and rows (no firmware bytes)."""

from __future__ import annotations

import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import pno_module_chain as pm

CSV_HEAD = "table,index,target,base,size,tag\n"


def listing(lines: list[tuple[int, str]]) -> str:
    """Text in the format of tools/adi_dis.py (`ADDR  parcels  text`, address from 0x120300)."""
    return "\n".join(f"{a:08x}  0001  {t}" for a, t in lines) + "\n"


def parsed(lines: list[tuple[int, str]]) -> list[tuple[int, str]]:
    return pm.sr.parse_listing(listing(lines), pm.LISTING_BASE)


def test_row_pc_core1_core2_and_non_code() -> None:
    assert pm.row_pc(0x28000000 + 4 * 0x122620) == 0x122620
    assert pm.row_pc(0x28800000 + 4 * 0x1238FC) == 0x1238FC
    assert pm.row_pc(0x28300758) is None  # DM(0xC01D6): data, not a code address
    assert pm.row_pc(0x20000000) is None
    assert pm.row_pc(0x28800000 + 4 * 0xB0000) is None


def test_code_rows_keeps_only_code_addresses_of_known_targets(tmp_path: Path) -> None:
    text = (
        CSV_HEAD
        + f"0xc0545148,8,2,{0x28000000 + 4 * 0x122620:#x},0x1,0x0\n"
        + "0xc05475e8,0,2,0x28300758,0x33,0x19190000\n"
        + f"0xc0545148,15,0,{0x28000000 + 4 * 0x124270:#x},0x3,0x0\n"
        + f"0xc0545148,99,9,{0x28000000 + 4 * 0x124270:#x},0x3,0x0\n"
    )
    path = tmp_path / "rows.csv"
    path.write_text(text)
    rows = pm.code_rows(pm.read_rows(path))
    assert [(r["index"], r["target"], r["pc"]) for r in rows] == [
        (8, 2, 0x122620),
        (15, 0, 0x124270),
    ]
    assert rows[0]["table"] == 0xC0545148 and rows[0]["size"] == 1


def test_routine_at_stops_at_return_and_takes_delay_slots() -> None:
    ins = parsed(
        [
            (0x00, "nop"),
            (0x01, "i3=i1"),
            (0x02, "f0=f1*f2"),
            (0x03, "rts (db)"),
            (0x04, "nop"),
            (0x05, "r0=r0+r1"),
            (0x06, "r9=0"),
            (0x07, "rts"),
        ]
    )
    r = pm.routine_at(ins, 0x120300 + 1)
    assert [t for _, t in r] == ["i3=i1", "f0=f1*f2", "rts (db)", "nop", "r0=r0+r1"]
    assert pm.routine_at(ins, 0x120300 + 6)[-1][1] == "rts"
    assert pm.routine_at(ins, 0x123456) == []  # not an instruction start


def test_skeleton_masks_registers_and_addresses_so_images_compare_equal() -> None:
    a = parsed([(0, "r4=dm(0xc27c5)"), (1, "f1=f2*f3"), (2, "rts")])
    b = parsed([(0, "r7=dm(0xc2a11)"), (1, "f5=f6*f7"), (2, "rts")])
    c = parsed([(0, "r4=dm(0xc27c5)"), (1, "f1=f2+f3"), (2, "rts")])
    assert pm.skeleton(a) == pm.skeleton(b)
    assert pm.similarity(a, b) == 1.0
    assert pm.similarity(a, c) < 1.0
    assert pm.similarity(a, []) == 0.0


def test_float_constants_decodes_immediates_in_range() -> None:
    one = struct.unpack("<I", struct.pack("<f", 0.9908))[0]
    ins = parsed(
        [
            (0, f"r3={one:#010x}"),
            (1, "r2=0x3f800000"),
            (2, "r5=0xffffffe1"),  # integer-looking: outside the float window
            (3, "r2=0x3f800000"),  # duplicate
            (4, "rts"),
        ]
    )
    vals = pm.float_constants(ins)
    assert len(vals) == 2
    assert abs(vals[0] - 0.9908) < 1e-6 and vals[1] == 1.0


def test_best_match_prefers_the_most_similar_routine_of_similar_length() -> None:
    mine = parsed([(i, "f1=f2*f3") for i in range(10)] + [(10, "rts")])
    same = parsed([(i, "f4=f5*f6") for i in range(10)] + [(10, "rts")])
    other = parsed([(i, "r1=r2+r3") for i in range(10)] + [(10, "rts")])
    short = parsed([(0, "rts")])
    best = pm.best_match(mine, {0x100: other, 0x200: same, 0x300: short})
    assert best == (0x200, 1.0)
    assert pm.best_match(mine, {0x300: short}) is None


def test_read_id_table_and_shape() -> None:
    code = bytes(8) + struct.pack("<3I", 0x120CD0, 0x121183, 0x125275) + bytes(4)
    assert pm.read_id_table(code, 8, 3) == [0x120CD0, 0x121183, 0x125275]
    ins = parsed([(0, "f1=f2*f3"), (1, "f4=f5*f6"), (2, "do (pc,3) until lce"), (3, "rts")])
    text = pm.shape(ins)
    assert text.startswith("4 instr, 2 mul") and "1 loop" in text


def test_directory_triples_and_alignment() -> None:
    rows = [
        {"table": 7, "index": 3, "target": 1, "pc": 0x120310, "size": 3},
        {"table": 7, "index": 1, "target": 0, "pc": 0x120300, "size": 3},
        {"table": 7, "index": 2, "target": 2, "pc": 0x120320, "size": 3},
        {"table": 7, "index": 4, "target": 0, "pc": 0x120330, "size": 3},
        {"table": 9, "index": 0, "target": 0, "pc": 0x120300, "size": 1},
    ]
    d = pm.directory(rows, 7)
    assert [r["index"] for r in d] == [1, 2, 3, 4]
    groups = pm.triples(d)
    assert [[r["target"] for r in g] for g in groups] == [[0, 2], [1], [0]]
    listings = {0: parsed([(0x00, "nop"), (0x30, "nop")]), 2: parsed([(0x20, "nop")])}
    c = pm.alignment(rows, listings)
    assert c[(0, True)] == 3 and c[(2, True)] == 1 and (1, True) not in c


def test_cli_directory_check_and_ids(tmp_path: Path, capsys) -> None:
    csv_path = tmp_path / "rows.csv"
    csv_path.write_text(
        CSV_HEAD
        + f"0xc0545148,6,2,{0x28000000 + 4 * 0x120301:#x},0x1,0x0\n"
        + f"0xc0545148,7,2,{0x28000000 + 4 * 0x120305:#x},0x1,0x0\n"
    )
    lst = tmp_path / "pno.txt"
    lst.write_text(listing([(0, "nop"), (1, "i3=i1"), (2, "f0=f1*f2"), (3, "rts")]))
    assert pm.main(["directory", str(csv_path), "--listing", f"2={lst}", "--check"]) == 0
    out = capsys.readouterr().out
    assert "target 2 PNO c1: aligned 1" in out and "MISALIGNED 1" in out
    assert pm.main(["directory", str(csv_path), "--listing", f"2={lst}"]) == 0
    assert "row  6 PNO c1 0x120301" in capsys.readouterr().out
    img = tmp_path / "arm.bin"
    img.write_bytes(struct.pack("<2I", 0x120301, 0x123456))
    assert pm.main(["ids", str(img), "0", "2", "--listing", f"2={lst}"]) == 0
    lines = capsys.readouterr().out.splitlines()
    assert lines[0].startswith("  0 0x120301 3 instr, 1 mul") and lines[1].endswith(
        "NOT-A-ROUTINE-START"
    )


def test_find_identical_and_shared_command(tmp_path: Path, capsys) -> None:
    body = [(i, "f1=f2*f3") for i in range(6)] + [(6, "rts")]
    mine = parsed(body)
    other = {7: parsed([(i, "f4=f5*f6") for i in range(6)] + [(6, "rts")])}
    assert pm.find_identical(mine, {0: {0x120400: other[7]}}) == (0, 0x120400)
    assert pm.find_identical(mine, {0: {0x120400: parsed([(0, "rts")])}}) is None
    csv_path = tmp_path / "rows.csv"
    csv_path.write_text(
        CSV_HEAD
        + f"0xc0545148,15,0,{0x28000000 + 4 * 0x120300:#x},0x3,0x0\n"
        + f"0xc0545148,17,2,{0x28000000 + 4 * 0x120300:#x},0x3,0x0\n"
        + f"0xc0545148,18,2,{0x28000000 + 4 * 0x120310:#x},0x3,0x0\n"
    )
    fx = tmp_path / "fx.txt"
    fx.write_text(listing(body))
    pno = tmp_path / "pno.txt"
    pno.write_text(listing(body + [(0x10 + i, "r1=r2+r3") for i in range(4)] + [(0x14, "rts")]))
    argv = ["shared", str(csv_path), "--listing", f"0={fx}", "--listing", f"2={pno}"]
    assert pm.main(argv) == 0
    out = capsys.readouterr().out
    assert "0xc0545148:17 0x120300 = FX c1 0x120300" in out
    assert "0xc0545148:18 0x120310 only here" in out
    assert "1 of 2 rows" in out
