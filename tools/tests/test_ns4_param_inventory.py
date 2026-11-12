"""Tests for tools/ns4_param_inventory.py (synthetic presets only, no factory data)."""

from __future__ import annotations

import zipfile

import ns4_param_inventory as inv


def put_bits(buf: bytearray, bit: int, width: int, value: int) -> None:
    for k in range(width):
        pos = bit + k
        mask = 0x80 >> (pos & 7)
        if (value >> (width - 1 - k)) & 1:
            buf[pos >> 3] |= mask
        else:
            buf[pos >> 3] &= ~mask & 0xFF


def organ_block(drawbar: int = 6, eq_mid: int = -20, drive: int = 100) -> bytes:
    blk = bytearray(135)
    by_name = {p.name: p for p in inv.organ_params()}
    put_bits(blk, by_name["drawbar_3_a"].bit, 4, drawbar)
    put_bits(blk, by_name["rotary_drive"].bit, 7, drive)
    mid = next(p for p in inv.fx_params() if p.name == "eq_mid")
    put_bits(blk, 80 * 8 + mid.bit, 7, eq_mid & 0x7F)
    return bytes(blk)


def test_read_bits_is_msb_first_and_crosses_bytes() -> None:
    assert inv.read_bits(bytes([0b10100000, 0b11000000]), 0, 3) == 0b101
    assert inv.read_bits(bytes([0x0F, 0xF0]), 4, 8) == 0xFF


def test_read_param_sign_extends() -> None:
    p = inv.Param("organ", "x", 0, 4, signed=True)
    assert inv.read_param(bytes([0b11100000]), p) == -2
    assert inv.read_param(bytes([0b01100000]), p) == 6


def test_param_tables_have_unique_names_and_fit_their_blocks() -> None:
    for fn, size in ((inv.organ_params, 135), (inv.piano_params, 147), (inv.synth_params, 493)):
        params = fn()
        assert len({p.name for p in params}) == len(params)
        assert all(p.bit + p.width <= size * 8 for p in params)
    assert all(p.bit + p.width <= 55 * 8 for p in inv.fx_params())


def test_drawbar_offsets_follow_the_19_bit_morph_stride() -> None:
    db = {p.name: p for p in inv.organ_params() if p.name.startswith("drawbar")}
    assert db["drawbar_1_a"].bit == 184 and db["drawbar_2_a"].bit == 203
    assert db["drawbar_1_b"].bit == 432 and all(p.width == 4 and p.morph for p in db.values())


def test_split_blocks_for_program_and_standalone() -> None:
    blocks = inv.split_blocks("ns4p", bytes(820))
    assert {k: len(v) for k, v in blocks.items()} == {"organ": 135, "piano": 147, "synth": 493}
    assert list(inv.split_blocks("ns4o", bytes(135))) == ["organ"]
    assert inv.split_blocks("ns4t", bytes(76)) == {}


def test_observe_collects_min_max_distinct_for_organ_fields() -> None:
    pres = [("ns4o", organ_block(2, -20, 10)), ("ns4o", organ_block(8, 30, 99))]
    stats = inv.observe(iter(pres), "organ")
    by = {p.name: st for p, st in stats.items()}
    assert (by["drawbar_3_a"].lo, by["drawbar_3_a"].hi, len(by["drawbar_3_a"].seen)) == (2, 8, 2)
    assert (by["rotary_drive"].lo, by["rotary_drive"].hi) == (10, 99)
    assert (by["fx80_eq_mid"].lo, by["fx80_eq_mid"].hi) == (-20, 30)


def test_observe_skips_other_sections() -> None:
    assert inv.observe(iter([("ns4o", organ_block())]), "synth") == {}


def test_iter_presets_reads_directory_and_zip(tmp_path) -> None:
    raw = bytes(48) + organ_block()
    (tmp_path / "a.ns4o").write_bytes(raw)
    (tmp_path / "skip.txt").write_bytes(b"x")
    assert [e for e, _ in inv.iter_presets(tmp_path)] == ["ns4o"]
    zpath = tmp_path / "b.ns4b"
    with zipfile.ZipFile(zpath, "w") as zf:
        zf.writestr("Organ Preset/Bank 1/x.ns4o", raw)
        zf.writestr("meta.xml", "<backup/>")
    got = list(inv.iter_presets(zpath))
    assert len(got) == 1 and got[0][1] == organ_block()


def test_format_row_and_cli(tmp_path, capsys) -> None:
    p = inv.Param("organ", "drawbar_1_a", 184, 4, morph=True, note="0..8")
    assert "u4m" in inv.format_row(p, None) and "bit   184" in inv.format_row(p, None)
    (tmp_path / "a.ns4o").write_bytes(bytes(48) + organ_block())
    assert inv.main([str(tmp_path), "--section", "organ"]) == 0
    assert "fields" in capsys.readouterr().out
    assert inv.main([str(tmp_path), "--list", "--section", "piano"]) == 0
    assert "model_id_a" in capsys.readouterr().out
    empty = tmp_path / "e"
    empty.mkdir()
    assert inv.main([str(empty)]) == 1
