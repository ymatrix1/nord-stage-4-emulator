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
    parser = argparse.ArgumentParser(description="Inspect Nord Stage 4 CBIN or .ns4b files.")
    parser.add_argument("path", type=Path, help="Path to .ns4p/.ns4y/.ns4o/.ns4n/.ns4b file")
    parser.add_argument("--decode", action="store_true", help="Decode internal bitstream fields")
    args = parser.parse_args(argv)

    if args.path.suffix.lower() == ".ns4b":
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
