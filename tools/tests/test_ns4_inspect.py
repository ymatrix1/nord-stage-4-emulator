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


def test_calculate_checksum():
    data = bytes([0x10, 0x20, 0x30, 0x40])
    expected = 0xFFFFFFFF - (0x10 + 0x20 + 0x30 + 0x40)
    assert ns4_inspect.calculate_checksum(data) == expected


def make_synthetic_firmware_container() -> bytes:
    # 32-byte top header:
    # header_size=0x20, image0_size=320 (0x140), image1_offset=0x160 (352), image1_size=160
    hdr = struct.pack(">IIII", 0x20, 320, 352, 160) + (b"\x00" * 16)

    # Image 0 at 0x20: subheader 20 bytes (seg_count=3 at offset 0x10)
    img0_sub = (b"\x00" * 16) + struct.pack(">I", 3)
    # 3 segment descriptors of 44 bytes: SRAM, CODE, ARM0
    sram_data = b"SRAM_PAYLOAD_TEST_BYTES"
    sram_csum = ns4_inspect.calculate_checksum(sram_data)
    seg_sram = (
        b"SRAM"
        + struct.pack(">IIII", 0x2D4, len(sram_data), 0x80000000, sram_csum)
        + (b"\x00" * 24)
    )
    seg_code = b"CODE" + struct.pack(">IIII", 0xFF54, 5000, 0xC0000000, 0x12345678) + (b"\x00" * 24)
    seg_arm0 = b"ARM0" + struct.pack(">IIII", 0x300, 100, 0xFFFF0020, 0x87654321) + (b"\x00" * 24)

    # Pad until Image 1 offset 352
    base = hdr + img0_sub + seg_sram + seg_code + seg_arm0
    pad = b"\x00" * (352 - len(base))

    # Image 1 at 352: subheader 20 bytes (seg_count=1)
    img1_sub = (b"\x00" * 16) + struct.pack(">I", 1)
    seg_img1_code = (
        b"CODE" + struct.pack(">IIII", 0xC42C, 1000, 0xC0000000, 0x99999999) + (b"\x00" * 24)
    )

    # Add DSP partition tags
    tags = b"__DSPOrg__DSPFxCore1__DSPSyn__Fx to Org__PNO__"

    total = base + pad + img1_sub + seg_img1_code + tags
    return total


def test_parse_firmware_container_synthetic(tmp_path: Path):
    raw = make_synthetic_firmware_container()
    info = ns4_inspect.parse_firmware_container(raw)

    assert info["header_size"] == 0x20
    assert info["image0_size"] == 320
    assert info["image1_offset"] == 352
    assert len(info["image0_segments"]) == 3
    assert info["image0_segments"][0]["name"] == "SRAM"
    assert info["image0_segments"][0]["target_address"] == "0x80000000"
    assert len(info["image1_segments"]) == 1
    assert info["image1_segments"][0]["name"] == "CODE"

    dsp_tags = [p["tag"] for p in info["dsp_partitions"]]
    assert "DSPOrg" in dsp_tags
    assert "DSPFxCore1" in dsp_tags
    assert "DSPSyn" in dsp_tags
    assert "Fx to Org" in dsp_tags
    assert "PNO" in dsp_tags

    fw_file = tmp_path / "firmware.bin"
    fw_file.write_bytes(raw)
    assert ns4_inspect.main([str(fw_file)]) == 0
    assert ns4_inspect.main([str(fw_file), "--firmware"]) == 0


def test_extract_firmware_from_pe_synthetic(tmp_path: Path):
    fw_raw = make_synthetic_firmware_container()
    # Create fake PE header
    dos_hdr = b"MZ" + (b"\x00" * 0x3A) + struct.pack("<I", 0x80)
    dos_pad = b"\x00" * (0x80 - len(dos_hdr))
    pe_hdr = b"PE\x00\x00" + (b"\x00" * 20)
    pe_pad = b"\x00" * (0x1000 - len(dos_hdr + dos_pad + pe_hdr))

    pe_file = tmp_path / "updater.exe"
    pe_file.write_bytes(dos_hdr + dos_pad + pe_hdr + pe_pad + fw_raw)

    extracted = ns4_inspect.extract_firmware_payload(pe_file.read_bytes())
    assert extracted[:4] == b"\x00\x00\x00\x20"
    assert ns4_inspect.main([str(pe_file)]) == 0
