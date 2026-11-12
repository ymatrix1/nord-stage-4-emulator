"""Tests for tools/arm_params.py on synthetic ARM code (no firmware bytes)."""

from __future__ import annotations

import csv
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import arm_params as ap
import arm_xref as ax

BASE = ap.LOAD_BASE
TABLE = 0xC0545448  # inside CHAN_REGION
PUSH = 0xE92D4070  # push {r4, r5, r6, lr}
NOP = 0xE1A00000


def add_imm(rd: int, rn: int, imm: int) -> int:
    return 0xE2800000 | (rn << 16) | (rd << 12) | imm


def str_(rt: int, rn: int, imm: int = 0) -> int:
    return 0xE5800000 | (rn << 16) | (rt << 12) | imm


def ldr_pc(rd: int, offset: int) -> int:
    return 0xE59F0000 | (rd << 12) | offset


def ldr_lsl4(rd: int, rn: int, rm: int) -> int:
    return 0xE7900200 | (rn << 16) | (rd << 12) | rm


def bl(at: int, target: int) -> int:
    return 0xEB000000 | (((target - (at + 8)) >> 2) & 0xFFFFFF)


def build(words: dict[int, int], size: int = 0x70000) -> bytes:
    """Zero image of `size` bytes with 32-bit words placed at absolute addresses."""
    buf = bytearray(size)
    for addr, value in words.items():
        struct.pack_into("<I", buf, addr - BASE, value)
    return bytes(buf)


def sender_image() -> bytes:
    """One two-path sender at 0xC0010000: path 1 adds 0x39, path 2 adds 4 * 0x39, table literal."""
    f = 0xC0010000
    w: dict[int, int] = {f: PUSH}
    w[f + 4] = ldr_pc(2, 0x40 - 12)  # ldr r2, =TABLE   (literal at f + 0x40)
    w[f + 8] = ldr_lsl4(1, 2, 0)  # ldr r1, [r2, r0, lsl #4]
    w[f + 12] = bl(f + 12, ap.FLOW_CONTROL)
    w[f + 16] = add_imm(8, 8, 0x39)
    w[f + 20] = str_(8, 3)
    w[f + 24] = NOP
    w[f + 28] = add_imm(2, 2, 0xE4)
    w[f + 32] = str_(2, 1)
    w[f + 0x40] = TABLE
    # second function: wrapper call with add r1,r1,#0x0e, inline add r1,r1,#0x38 (4 * 0x0e)
    g = 0xC0010100
    w[g] = PUSH
    w[g + 4] = add_imm(1, 1, 0x0E)
    w[g + 8] = bl(g + 8, ap.EMIFA_WRITERS[1])
    w[g + 12] = add_imm(1, 1, 0x38)
    w[g + 16] = str_(1, 3)
    return build(w)


def test_decoders() -> None:
    assert ap.decode_add_imm(add_imm(1, 2, 0x39)) == (1, 2, 0x39)
    assert ap.decode_add_imm(NOP) is None
    assert ap.decode_str_base(str_(8, 3, 4)) == (8, 3, 4)
    assert ap.decode_str_base(ldr_pc(1, 0)) is None
    assert ap.decode_shifted_lsl4(ldr_lsl4(1, 2, 0)) == (1, 2, 0)
    assert ap.decode_shifted_lsl4(NOP) is None
    assert ap.is_push_lr(PUSH)
    assert not ap.is_push_lr(0xE92D0070)


def test_function_start() -> None:
    img = sender_image()
    assert ap.function_start(img, 0xC001001C) == 0xC0010000


def test_table_literals_and_immediates() -> None:
    img = sender_image()
    assert ap.table_literals(img, 0xC0010000, 0xC0010060) == [TABLE]
    imms = ap.immediates_stored(img, 0xC0010000, 0xC0010060)
    assert [i for _, i in imms] == [0x39, 0xE4]
    assert ap.wrapper_arg_imm(img, 0xC0010108) == 0x0E


def test_pair_offsets() -> None:
    paired, rest = ap.pair_offsets([0x39, 0xE4, 1, 1, 0x20, 0x80, 0x1F, 3])
    assert paired == [0x20, 0x39]
    assert rest == [0x1F]
    assert ap.pair_offsets([]) == ([], [])


def test_find_senders_and_rows(tmp_path: Path) -> None:
    img = sender_image()
    senders = ap.find_senders(img)
    f = senders[0xC0010000]
    assert f.sites == [0xC001000C]
    assert f.tables == [TABLE]
    assert f.offsets == [0x39]
    g = senders[0xC0010100]
    assert g.wrapper_calls == [0xC0010108]
    assert g.offsets == [0x0E]  # wrapper add 0x0e pairs with the inline 4 * 0x0e = 0x38
    summary = ap.summarize(senders)
    assert summary["flow_control_sites"] == 1
    assert summary["wrapper_calls"] == 1
    assert summary["functions"] == 2
    rows = ap.sender_rows(senders)
    assert rows[0]["function"] == "0xc0010000"
    assert rows[0]["word_offsets"] == "0x39"
    assert rows[0]["byte_offsets_path2"] == "0xe4"
    out = tmp_path / "s.csv"
    ap.write_csv(rows, out)
    assert list(csv.DictReader(out.open()))[0]["table"] == "0xc0545448"


def mem_with_table() -> bytes:
    buf = bytearray(0x600000)
    records = [(0, 0x282C9068, 0x1393, 0x1000000), (1, 0x28AC9068, 0x1393, 0xD000000)]
    for k, rec in enumerate(records):
        struct.pack_into("<4I", buf, TABLE - BASE + 16 * k, *rec)
    return bytes(buf)


def test_read_table_and_records() -> None:
    mem = mem_with_table()
    assert ap.read_table(mem, TABLE, 2)[1] == (1, 0x28AC9068, 0x1393, 0xD000000)
    assert ap.table_extents([TABLE, TABLE + 0x20]) == {TABLE: 2, TABLE + 0x20: 48}
    rows = ap.record_rows(mem, [TABLE])
    assert rows[0]["base"] == "0x282c9068" and rows[1]["target"] == "1"
    img = sender_image()
    sender_rows = ap.sender_rows(ap.find_senders(img), mem)
    assert sender_rows[0]["first_record"] == "target=0 base=0x282c9068"


def test_read_flags() -> None:
    buf = bytearray(0x300000)
    for t in range(ap.NUM_TARGETS):
        struct.pack_into("<I", buf, ap.TARGET_FLAG_BASE - BASE + 4 * t, 1 if t < 6 else 0)
        struct.pack_into("<I", buf, ap.TARGET_COMM_BASE - BASE + 4 * t, [0, 0, 1, 1, 2, 2, 3, 4][t])
    flags = ap.read_flags(bytes(buf))
    assert flags[5] == {"target": 5, "path2": 1, "comm": 2}
    assert flags[6] == {"target": 6, "path2": 0, "comm": 3}


def test_main_commands(tmp_path: Path, capsys) -> None:
    img = tmp_path / "img.bin"
    img.write_bytes(sender_image())
    csv_out = tmp_path / "out.csv"
    assert ap.main(["senders", str(img), "--csv", str(csv_out)]) == 0
    assert csv_out.exists()
    mem = tmp_path / "mem.bin"
    mem.write_bytes(mem_with_table())
    assert ap.main(["tables", str(mem), "--table", f"{TABLE:#x}:2"]) == 0
    assert "target=1" in capsys.readouterr().out
    rec_out = tmp_path / "rec.csv"
    assert ap.main(["records", str(img), "--mem", str(mem), "--csv", str(rec_out)]) == 0
    assert "0x282c9068" in rec_out.read_text()
    assert ax.bl_callers(img.read_bytes(), ap.FLOW_CONTROL) == [0xC001000C]
