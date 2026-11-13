"""Tests for tools/fx_chain_install.py on synthetic memory, rows and images (no firmware bytes)."""

from __future__ import annotations

import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import fx_chain_install as fc

CSV_HEAD = "table,index,target,base,size,tag\n"


def record(target: int, base: int, size: int = 3, tag: int = 0) -> bytes:
    return struct.pack("<4I", target, base, size, tag)


def sys1(pc: int) -> int:
    return 0x28000000 + 4 * pc


def sys2(pc: int) -> int:
    return 0x28800000 + 4 * pc


def test_base_to_pc_cores_and_non_code() -> None:
    assert fc.base_to_pc(sys1(0x120630)) == (1, 0x120630)
    assert fc.base_to_pc(sys2(0x1243F4)) == (2, 0x1243F4)
    assert fc.base_to_pc(0x28300758) is None  # DM data, not code
    assert fc.base_to_pc(sys2(0xB0000)) is None
    assert fc.base_to_pc(0x20000000) is None


def test_scan_code_records_keeps_matching_target_and_core_only() -> None:
    mem = (
        bytes(16)
        + record(0, sys1(0x120630))  # FX c1: kept
        + record(1, sys2(0x120630))  # FX c2: kept
        + record(1, sys1(0x120630))  # target 1 with a core 1 base: wrong core, dropped
        + record(2, sys1(0x122620))  # PNO target: not an FX record
        + record(0, 0x283073A8, 0x463)  # state block, not code
    )
    got = fc.scan_code_records(mem, 0xC0544000)
    assert [(r.addr, r.target, r.core, r.pc) for r in got] == [
        (0xC0544010, 0, 1, 0x120630),
        (0xC0544020, 1, 2, 0x120630),
    ]
    assert got[0].size == 3 and got[0].tag == 0


def test_scan_code_records_reads_size_and_tag() -> None:
    mem = bytes(16) + record(0, sys1(0x1206B0), 1, 0x100)
    (r,) = fc.scan_code_records(mem, 0)
    assert (r.size, r.tag) == (1, 0x100)


def test_group_runs_splits_on_gaps() -> None:
    recs = [
        fc.Record(0x100, 0, 1, 0x120630, 3, 0),
        fc.Record(0x110, 1, 2, 0x120630, 3, 0),
        fc.Record(0x130, 0, 1, 0x1206B0, 1, 0),  # 0x20 apart: new run
    ]
    assert fc.group_runs(recs) == [0, 0, 1]
    assert fc.group_runs([]) == []


def test_csv_rows_without_listings_has_blank_census() -> None:
    recs = [fc.Record(0xC0544010, 0, 1, 0x120630, 3, 0)]
    (row,) = fc.csv_rows(recs, {})
    assert row["record_addr"] == "0xc0544010" and row["pc"] == "0x120630"
    assert row["instr"] == "" and row["run"] == 0
    assert set(row) == set(fc.CSV_FIELDS)


def test_csv_rows_with_listing_counts_instructions() -> None:
    text = "\n".join(
        f"{a:08x}  0001  {t}"
        for a, t in ((0, "f0=f1*f2"), (1, "nop"), (2, "rts (db)"), (3, "nop"), (4, "nop"))
    )
    ins = fc.pm.sr.parse_listing(text + "\n", fc.pm.LISTING_BASE)
    (row,) = fc.csv_rows([fc.Record(0, 0, 1, 0x120300, 3, 0)], {0: ins})
    assert int(row["instr"]) == 5  # type: ignore[arg-type]


def test_stage_rows_decode_dm_words_and_size() -> None:
    import csv
    import io

    text = (
        CSV_HEAD
        + f"0xc0547668,0,0,{sys1(0xB63EE):#x},0x83,0x0\n"
        + f"0xc0547668,1,0,{sys1(0xB63F6):#x},0x83,0x0\n"
        + f"0xc0547668,35,1,{sys2(0xB63EE):#x},0x83,0x0\n"
        + f"0xc0546268,0,0,{sys1(0xB0000):#x},0x353,0x0\n"  # other table
    )
    rows = fc.stage_rows(list(csv.DictReader(io.StringIO(text))))
    assert [(r["index"], r["core"], r["dm"], r["words"]) for r in rows] == [
        (0, 1, 0xB63EE, 9),
        (1, 1, 0xB63F6, 9),
        (35, 2, 0xB63EE, 9),
    ]


def test_sram_word_and_check_stages() -> None:
    image = bytearray(4 * 0xB6500)
    for dm in (0xB63EE, 0xB63F6):
        struct.pack_into("<I", image, 4 * dm, 0x1249D2)
    assert fc.sram_word(bytes(image), 0xB63EE) == 0x1249D2
    rows = [
        {"index": 0, "target": 1, "core": 2, "dm": 0xB63EE, "words": 9},
        {"index": 1, "target": 1, "core": 2, "dm": 0xB63F6, "words": 9},
        {"index": 2, "target": 1, "core": 2, "dm": 0xB63FE, "words": 9},  # not a stub
        {"index": 3, "target": 0, "core": 1, "dm": 0xB63EE, "words": 9},  # other core
    ]
    assert fc.check_stages(rows, bytes(image), 2) == (2, 3)
    assert fc.check_stages(rows, bytes(image), 1) == (0, 1)


def test_amp_cases_keeps_last_value_per_word_and_f32() -> None:
    assert fc.amp_cases([(0xB060B, 1), (0xB060C, 2), (0xB060B, 3)]) == {0xB060B: 3, 0xB060C: 2}
    assert fc.f32(0x3F800000) == 1.0
    assert fc.f32(0xBF800000) == -1.0


def test_cmd_stages_prints_rows_and_check(tmp_path: Path, capsys) -> None:  # noqa: ANN001
    csv_path = tmp_path / "rows.csv"
    csv_path.write_text(CSV_HEAD + f"0xc0547668,0,0,{sys1(0xB63EE):#x},0x83,0x0\n")
    image = bytearray(4 * 0xB6500)
    struct.pack_into("<I", image, 4 * 0xB63EE, 0x125346)
    sram = tmp_path / "core1_sram.bin"
    sram.write_bytes(bytes(image))
    assert fc.main(["stages", str(csv_path), str(sram)]) == 0
    out = capsys.readouterr().out
    assert "stage row  0 core 1 DM 0xb63ee words 9" in out
    assert "core 1: 1 of 1 rows hold the null stub 0x125346" in out
