from __future__ import annotations

import struct
import zipfile
from pathlib import Path

import ns4_inspect
import pytest


def make_cbin_bytes(tag: str = "ns4p", bank: int = 0, loc: int = 5, cat: int = 0x2D) -> bytes:
    total = ns4_inspect.EXPECTED_SIZES[tag]
    schema = ns4_inspect.EXPECTED_SCHEMAS[tag]
    header = (
        b"CBIN"
        + struct.pack("<I", 1)
        + tag.encode("ascii")
        + struct.pack("<HHIII", bank, loc, cat, schema, 0x12345678)
        + (b"\x00" * 18)
        + struct.pack(">H", schema)
    )
    return header + (b"\xaa" * (total - len(header)))


def test_parse_all_cbin_preset_tags():
    for tag, expected_len in ns4_inspect.EXPECTED_SIZES.items():
        raw = make_cbin_bytes(tag=tag, bank=1, loc=12, cat=0x12)
        summary = ns4_inspect.parse_cbin_bytes(raw)
        assert summary.tag == tag
        assert summary.bank_index == 1
        assert summary.location_index == 12
        assert summary.total_size == expected_len
        assert summary.schema_id == ns4_inspect.EXPECTED_SCHEMAS[tag]


def test_rejects_bad_magic_and_short_buffer():
    with pytest.raises(ValueError, match="too short"):
        ns4_inspect.parse_cbin_bytes(b"CBIN")

    bad = bytearray(make_cbin_bytes("ns4o"))
    bad[:4] = b"NOPE"
    with pytest.raises(ValueError, match="invalid CBIN magic"):
        ns4_inspect.parse_cbin_bytes(bytes(bad))


def test_inspect_synthetic_ns4b_zip(tmp_path: Path):
    ns4b_file = tmp_path / "sample.ns4b"
    xml = (
        '<?xml version="1.0" encoding="UTF-8"?>'
        '<backup file_version="1" backup_format_version="1" product_id="46" '
        'product_version="166" product_build="2934" product_content_version="0" '
        'manager_version="918" manager_build="1444"/>'
    )
    with zipfile.ZipFile(ns4b_file, "w") as zf:
        zf.writestr("meta.xml", xml)
        zf.writestr("Program/Bank A/Test.ns4p", make_cbin_bytes("ns4p"))

    summary = ns4_inspect.inspect_ns4b_archive(ns4b_file)
    assert summary.product_id == 46
    assert summary.product_version == 166
    assert summary.extension_counts["ns4p"] == 1
    assert summary.extension_counts["xml"] == 1
    assert ns4_inspect.main([str(ns4b_file)]) == 0


def test_decode_organ_bitstream(tmp_path: Path):
    organ_cbin = tmp_path / "test.ns4o"
    raw = make_cbin_bytes("ns4o")
    organ_cbin.write_bytes(raw)

    decoded = ns4_inspect.decode_cbin_payload("ns4o", raw[48:])
    assert "organ" in decoded
    assert "drawbars" in decoded["organ"]["layer_a"]
    assert len(decoded["organ"]["layer_a"]["drawbars"]) == 9
    assert ns4_inspect.main([str(organ_cbin), "--decode"]) == 0


def test_decode_program_bitstream(tmp_path: Path):
    prog_cbin = tmp_path / "test.ns4p"
    raw = make_cbin_bytes("ns4p")
    prog_cbin.write_bytes(raw)

    decoded = ns4_inspect.decode_cbin_payload("ns4p", raw[48:])
    assert "organ" in decoded
    assert decoded["piano_bytes"] == 147
    assert decoded["synth_bytes"] == 493
    assert ns4_inspect.main([str(prog_cbin), "--decode"]) == 0
