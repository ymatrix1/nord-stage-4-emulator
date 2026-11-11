#!/usr/bin/env python3
"""Nord Stage 4 CBIN preset, .ns4b backup archive, and OS v1.66 firmware inspector.

Spec: docs/specs/cbin-and-presets.md
"""

from __future__ import annotations

import argparse
import json
import struct
import sys
import xml.etree.ElementTree as ET
import zipfile
from dataclasses import asdict, dataclass
from pathlib import Path

CBIN_MAGIC = b"CBIN"
CBIN_HEADER_SIZE = 48

EXPECTED_SIZES: dict[str, int] = {
    "ns4p": 868,
    "ns4l": 868,
    "ns4y": 541,
    "ns4n": 195,
    "ns4o": 183,
    "ns4t": 124,
}

EXPECTED_SCHEMAS: dict[str, int] = {
    "ns4p": 0x0139,
    "ns4l": 0x0139,
    "ns4y": 0x00D0,
    "ns4n": 0x00CB,
    "ns4o": 0x00CD,
    "ns4t": 0x006C,
}


@dataclass(frozen=True)
class CbinSummary:
    tag: str
    format_version: int
    bank_index: int
    location_index: int
    category_id: int
    schema_id: int
    checksum: int
    total_size: int
    payload_size: int


@dataclass(frozen=True)
class BackupSummary:
    product_id: int
    product_version: int
    product_build: int
    manager_version: int
    extension_counts: dict[str, int]


class BitReader:
    """Reads sequential bits from big-endian packed byte stream."""

    def __init__(self, data: bytes):
        self.data = data
        self.bit_pos = 0

    def read_bits(self, count: int) -> int:
        val = 0
        for _ in range(count):
            byte_idx = self.bit_pos // 8
            bit_idx = 7 - (self.bit_pos % 8)
            bit = (self.data[byte_idx] >> bit_idx) & 1
            val = (val << 1) | bit
            self.bit_pos += 1
        return val

    def read_bool(self) -> bool:
        return self.read_bits(1) != 0


def decode_organ_payload(payload: bytes) -> dict:
    if len(payload) < 135:
        raise ValueError(f"organ payload too short: {len(payload)} < 135")
    r = BitReader(payload)
    r.bit_pos = 72
    models = ["B3", "Vox", "Farfisa", "Pipe1", "Pipe2"]
    model_idx = r.read_bits(3)
    model = models[model_idx] if model_idx < len(models) else "Unknown"
    perc_on = r.read_bool()
    perc_soft = r.read_bool()
    perc_fast = r.read_bool()
    perc_third = r.read_bool()
    vib_on = r.read_bool()
    vib_type = r.read_bits(3)
    rot_speeds = ["Stop", "Slow", "Fast"]
    rot_spd_idx = r.read_bits(2)
    rot_speed = rot_speeds[rot_spd_idx] if rot_spd_idx < len(rot_speeds) else "Unknown"
    rot_drive = round((r.read_bits(7) / 127.0) * 10.0, 2)

    la_enabled = r.read_bool()
    la_level = round(r.read_bits(7) / 127.0, 2)
    la_octave = r.read_bits(3) - 2
    drawbars_a = [min(r.read_bits(4), 8) for _ in range(9)]

    lb_enabled = r.read_bool()
    lb_level = round(r.read_bits(7) / 127.0, 2)
    lb_octave = r.read_bits(3) - 2
    drawbars_b = [min(r.read_bits(4), 8) for _ in range(9)]

    return {
        "model": model,
        "percussion": {
            "enabled": perc_on,
            "soft": perc_soft,
            "fast": perc_fast,
            "third": perc_third,
        },
        "vibrato": {"enabled": vib_on, "type": vib_type},
        "rotary": {"speed": rot_speed, "drive": rot_drive},
        "layer_a": {
            "enabled": la_enabled,
            "level": la_level,
            "octave": la_octave,
            "drawbars": drawbars_a,
        },
        "layer_b": {
            "enabled": lb_enabled,
            "level": lb_level,
            "octave": lb_octave,
            "drawbars": drawbars_b,
        },
    }


def decode_cbin_payload(tag: str, payload: bytes) -> dict:
    if tag == "ns4o":
        return {"organ": decode_organ_payload(payload)}
    if tag in ("ns4p", "ns4l") and len(payload) >= 820:
        return {
            "organ": decode_organ_payload(payload[45:180]),
            "piano_bytes": len(payload[180:327]),
            "synth_bytes": len(payload[327:820]),
        }
    return {"raw_payload_len": len(payload)}


def parse_cbin_bytes(data: bytes) -> CbinSummary:
    """Parses and validates a 48-byte CBIN v1 preset header."""
    if len(data) < CBIN_HEADER_SIZE:
        raise ValueError(f"CBIN buffer too short: {len(data)} < {CBIN_HEADER_SIZE}")
    magic = data[:4]
    if magic != CBIN_MAGIC:
        raise ValueError(f"invalid CBIN magic: {magic!r}")
    (fmt_ver,) = struct.unpack("<I", data[4:8])
    if fmt_ver != 1:
        raise ValueError(f"unsupported CBIN version: {fmt_ver}")
    tag = data[8:12].decode("ascii", errors="replace")
    bank, loc, cat, schema_le, crc = struct.unpack("<HHIII", data[12:28])
    (schema_be,) = struct.unpack(">H", data[46:48])

    if tag in EXPECTED_SIZES and len(data) != EXPECTED_SIZES[tag]:
        raise ValueError(f"{tag} size mismatch: expected {EXPECTED_SIZES[tag]}, got {len(data)}")
    if tag in EXPECTED_SCHEMAS and schema_le != schema_be:
        raise ValueError(f"schema LE/BE mismatch: 0x{schema_le:04X} != 0x{schema_be:04X}")

    return CbinSummary(
        tag=tag,
        format_version=fmt_ver,
        bank_index=bank,
        location_index=loc,
        category_id=cat,
        schema_id=schema_be,
        checksum=crc,
        total_size=len(data),
        payload_size=len(data) - CBIN_HEADER_SIZE,
    )


def parse_meta_xml(xml_text: str) -> dict[str, int]:
    """Parses the root <backup .../> element from a .ns4b archive's meta.xml."""
    root = ET.fromstring(xml_text)
    if root.tag != "backup":
        raise ValueError(f"unexpected root tag: {root.tag}")
    return {k: int(v) for k, v in root.attrib.items()}


def calculate_checksum(data: bytes) -> int:
    """Calculates Clavia's 32-bit one's-complement byte checksum: 0xFFFFFFFF - sum(bytes)."""
    return (0xFFFFFFFF - sum(data)) & 0xFFFFFFFF


KNOWN_DSP_TAGS: list[tuple[bytes, str]] = [
    (b"DSPFxCore1", "SHARC Effects Pipeline & String Resonance DSP (DSPFxCore1)"),
    (b"DSPOrg", "SHARC Organ Physical Modeling & Rotary 122 DSP (DSPOrg)"),
    (b"DSPSyn", "SHARC 3-Layer Nord Wave 2 Synth DSP (DSPSyn)"),
    (b"Fx to Org", "SPORT Bus Routing: Effects to Organ"),
    (b"to Fx", "SPORT Bus Routing: Inter-core to Effects"),
    (b"Host", "AM1705 Host Processor Routing"),
    (b"ORG", "B3 Organ Physical Model Domain Tag"),
    (b"PNO", "Grand / Upright Piano Acoustic Model Domain Tag"),
    (b"SMP", "Sample Library Synth Domain Tag"),
    (b"SY1", "Wave 2 Synth Layer 1 Domain Tag"),
    (b"SYL3", "Wave 2 Synth Layer 3 Domain Tag"),
]


def extract_firmware_payload(raw: bytes) -> bytes:
    """Extracts Resource 101 payload from Windows PE executable, or returns raw bytes."""
    if raw.startswith(b"MZ") and len(raw) > 0x40:
        (e_lfanew,) = struct.unpack_from("<I", raw, 0x3C)
        if e_lfanew + 24 <= len(raw) and raw[e_lfanew : e_lfanew + 4] == b"PE\x00\x00":
            # Search for the 32-byte firmware container header signature:
            # 00 00 00 20 followed by image0_size and image1_offset
            for offset in range(0x1000, len(raw) - 0x20, 4):
                if raw[offset : offset + 4] == b"\x00\x00\x00\x20":
                    img0_size, img1_off = struct.unpack_from(">II", raw, offset + 4)
                    if 0x20 + img0_size == img1_off and img1_off < len(raw):
                        return raw[offset:]
    return raw


def parse_firmware_container(raw: bytes) -> dict:
    """Parses dual-bank Clavia OS firmware container (Resource 101)."""
    data = extract_firmware_payload(raw)
    if len(data) < 32:
        raise ValueError(f"firmware buffer too short: {len(data)} < 32")

    header_size, img0_size, img1_off, img1_size = struct.unpack_from(">IIII", data, 0)
    if header_size != 0x20:
        raise ValueError(f"invalid firmware header size: 0x{header_size:X}")
    if 0x20 + img0_size != img1_off:
        raise ValueError(f"image1 offset mismatch: 0x{img1_off:X} != 0x20 + 0x{img0_size:X}")

    def parse_bank_segments(base_off: int) -> list[dict]:
        if len(data) < base_off + 20:
            return []
        (seg_count,) = struct.unpack_from(">I", data, base_off + 0x10)
        cursor = base_off + 0x14
        segs = []
        for _ in range(min(seg_count, 8)):
            if cursor + 44 > len(data):
                break
            entry = data[cursor : cursor + 44]
            name = entry[:4].decode("latin1", errors="replace").rstrip("\x00")
            off, size, target, csum = struct.unpack_from(">IIII", entry, 4)
            seg_info: dict = {
                "name": name,
                "data_offset": off,
                "data_size": size,
                "target_address": f"0x{target:08X}",
                "checksum": f"0x{csum:08X}",
            }
            if name != "CODE" and base_off + off + size <= len(data):
                seg_bytes = data[base_off + off : base_off + off + size]
                calc_csum = calculate_checksum(seg_bytes)
                seg_info["checksum_valid"] = calc_csum == csum
            segs.append(seg_info)
            cursor += 44
        return segs

    partitions = []
    for tag, desc in KNOWN_DSP_TAGS:
        pos = data.find(tag)
        if pos != -1:
            partitions.append({"tag": tag.decode("latin1"), "offset": pos, "description": desc})

    return {
        "header_size": header_size,
        "image0_size": img0_size,
        "image1_offset": img1_off,
        "image1_size": img1_size,
        "image0_segments": parse_bank_segments(0x20),
        "image1_segments": parse_bank_segments(img1_off),
        "dsp_partitions": partitions,
    }


def inspect_ns4b_archive(path: Path) -> BackupSummary:
    """Inspects a .ns4b ZIP archive without extracting large sample banks to disk."""
    with zipfile.ZipFile(path, "r") as zf:
        meta_bytes = zf.read("meta.xml")
        attrs = parse_meta_xml(meta_bytes.decode("utf-8"))
        counts: dict[str, int] = {}
        for name in zf.namelist():
            ext = name.rsplit(".", 1)[-1] if "." in name else "NONE"
            counts[ext] = counts.get(ext, 0) + 1
    return BackupSummary(
        product_id=attrs["product_id"],
        product_version=attrs["product_version"],
        product_build=attrs["product_build"],
        manager_version=attrs["manager_version"],
        extension_counts=counts,
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Inspect Nord Stage 4 CBIN presets, backups, or OS firmware."
    )
    parser.add_argument("path", type=Path, help="Path to preset, archive, or firmware file")
    parser.add_argument("--decode", action="store_true", help="Decode internal bitstream fields")
    parser.add_argument(
        "--firmware", action="store_true", help="Inspect as Clavia OS firmware container"
    )
    args = parser.parse_args(argv)

    if args.firmware or args.path.suffix.lower() in (".exe", ".bin"):
        raw = args.path.read_bytes()
        summary = parse_firmware_container(raw)
    elif args.path.suffix.lower() == ".ns4b":
        summary = asdict(inspect_ns4b_archive(args.path))
    else:
        raw = args.path.read_bytes()
        cbin = parse_cbin_bytes(raw)
        summary = asdict(cbin)
        if args.decode:
            summary["decoded"] = decode_cbin_payload(cbin.tag, raw[CBIN_HEADER_SIZE:])
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
