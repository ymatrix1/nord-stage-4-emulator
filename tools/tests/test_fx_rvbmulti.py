"""Tests for tools/fx_rvbmulti.py: instance layout of the multi-tap reverb (types 2..11)."""

import csv
import struct

import fx_rvbmulti as rm


def write_image(path, words):
    raw = bytearray(4 * 0xC2000)
    for a, w in words.items():
        struct.pack_into("<I", raw, 4 * a, w)
    path.write_bytes(bytes(raw))


def test_rows_are_the_state_table_rows_of_the_module():
    # rows of `0xC0548A78`: core 1 at 0xC1AC8 + 108 k, core 2 at 0xC1A28 + 108 k
    assert rm.ROWS[1][:2] == [0xC1AC8, 0xC1B34]
    assert rm.ROWS[2][:2] == [0xC1A28, 0xC1A94]
    assert rm.CTX == rm.ROW + 0x20


def test_amount_words_are_the_captured_table_points():
    assert rm.amount_words(127) == (0x3F800000, 0)
    assert rm.amount_words(0) == (0, 0x3F800000)
    assert rm.amount_words(63) == (0x3EBB0410, 0x3F6E4FB0)


def test_type_words_move_row_addresses_and_keep_pm_and_fir_addresses():
    words = rm.type_words(3)
    # `c[-27]` of the reference row 0xC1AC8 + 0x20 - 27 lands on the row under test
    assert words[rm.CTX - 27] == 0x3E570A3D
    assert 0x94384 in words and 0xE0C46 in words  # PM list and FIR words are shared by the units
    assert all(not (rm.ROW_REF <= a < rm.ROW_REF + rm.ROW_WORDS) for a in words)


def test_layout_overlays_image_row_type_words_and_amount(tmp_path):
    img = tmp_path / "core2.bin"
    write_image(img, {rm.CTX - 32: 0x3F7DC91D, rm.CTX - 15: 0x8020000, rm.CTX - 14: 5000})
    mem = rm.layout(str(img), 3, 127)
    assert mem[rm.CTX - 32] == 0x3F7DC91D  # image word kept
    assert mem[rm.CTX - 15] == 0x8020000 and mem[rm.CTX - 14] == 5000
    assert mem[rm.CTX - 31] == 0x3F800000 and mem[rm.CTX - 30] == 0
    assert mem[rm.CTX - 29] == 0x3F800000  # smoothed state settled to the target
    assert mem[rm.CTX - 27] == 0x3E570A3D  # an ARM type word of type 3


def test_instance_rows_list_non_zero_words_per_row(tmp_path):
    img = tmp_path / "core1.bin"
    write_image(img, {rm.ROWS[1][1] + 0x20 + 5: 0x1234, rm.ROWS[1][0]: 0x77})
    rows = rm.instance_rows(str(img), 1)
    assert (1, 0, -32, 0x77) in rows and (1, 1, 5, 0x1234) in rows and len(rows) == 2
    raw_row = rm.row_report(str(img))
    assert raw_row == []  # core 2 row 5 is empty in this image


def test_committed_instance_csv_has_static_line_bases_for_all_ten_rows():
    with open(rm.DATA / "fx-rvbmulti-instance.csv", newline="") as f:
        rows = list(csv.DictReader(f))
    base = {(r["core"], r["row"]): int(r["word"], 16) for r in rows if r["ctx"] == "-15"}
    assert len(base) == 10
    assert base[("1", "0")] == 0x08004000 and base[("2", "0")] == 0x08020000
    assert base[("2", "4")] == 0x08020000 + 4 * 0x5000
    assert {int(r["word"], 16) for r in rows if r["ctx"] == "-14"} == {0x5000}


def test_committed_golden_csv_covers_nine_types_and_two_inputs():
    with open(rm.DATA / "fx-rvbmulti-golden.csv", newline="") as f:
        rows = list(csv.DictReader(f))
    assert len(rows) == 2 * len(rm.TYPES)
    assert {int(r["type"]) for r in rows} == set(rm.TYPES)
    imp = {int(r["type"]): int(r["first"]) for r in rows if r["kind"] == "impulse"}
    assert imp[2] == 848 and imp[10] == 3025 and all(v >= 0 for v in imp.values())
    assert all(r["hash"].startswith("0x") and len(r["hash"]) == 18 for r in rows)


def test_main_instance_prints_csv(tmp_path, capsys):
    img = tmp_path / "core2.bin"
    write_image(img, {rm.ROWS[2][0] + 0x20: 0xC1A94})
    assert rm.main(["instance", str(img), "--core", "2"]) == 0
    out = capsys.readouterr().out.splitlines()
    assert out[0] == "core,row,ctx,word" and out[1] == "2,0,0,0x000c1a94"
