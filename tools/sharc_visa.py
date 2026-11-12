#!/usr/bin/env python3
"""Length/type classifier and tiler for ADSP-214xx VISA instructions (best effort, ASSUMED).

Source: SHARC Processor Programming Reference rev 2.4, chapter 10 (`.reference/` only):
VISA instructions are MSB aligned and decoded from bit 47; a trailing `a`/`b`/`c` in the type name
is a 48/32/16-bit instruction. 32-bit forms carry `0111111` in bits 22..16, i.e. the low 7 bits of
their second 16-bit parcel are `0x3F`.

This module only recovers instruction boundaries and type names, not operands, so it can tell
whether a byte stream *tiles* into plausible instructions. Parcels are read little-endian
(firmware layout of `bank0_CODE` 0x440000..0x4FFFFF, see docs/research/dsp-image-hunt.md 5.5).

    python tools/sharc_visa.py target/fw_unpacked/bank0_CODE.bin 0x45d5ec 0x45d630
"""

from __future__ import annotations

import argparse
import struct
import sys
from pathlib import Path


def classify(p0: int, p1: int | None) -> tuple[str, int]:
    """Return ``(type name, length in 16-bit parcels)`` for the instruction starting at ``p0``."""
    marker = p1 is not None and (p1 & 0x7F) == 0x3F
    top3, top4, sub = p0 >> 13, p0 >> 12, (p0 >> 2) & 7
    if top4 == 0b1100:
        return "2c", 1
    if top4 == 0b1001:
        if sub & 4:
            return "3c", 1
        return {2: ("15b", 2), 1: ("16b", 2)}.get(sub, ("16a", 3))
    if top3 == 0b001:
        return ("1b", 2) if marker else ("1a", 3)
    if top3 == 0b010:
        return ("3b", 2) if marker else ("3a", 3)
    if top3 == 0b011:
        kind = "5" if (p0 >> 12) & 1 else "4"
        return (kind + "b", 2) if marker else (kind + "a", 3)
    if top3 == 0b100:
        return "6a", 3
    if top3 == 0b101:
        return "15a", 3
    if top3 >= 0b110:
        return "10a", 3
    if p0 >> 8 in (0x18, 0x19):
        return "25", 3
    code = (p0 >> 8) & 0x1F
    short = (p0 >> 7) & 1
    table = {
        0x01: ("2b", 2) if short else ("2a", 3),
        0x04: ("7b", 2) if marker else ("7a", 3),
        0x06: ("8a", 3),
        0x07: ("8a", 3),
        0x08: ("9b", 2) if marker else ("9a", 3),
        0x09: ("9b", 2) if marker else ("9a", 3),
        0x0A: ("11c", 1) if short else ("11a", 3),
        0x0B: ("11c", 1) if short else ("11a", 3),
        0x0C: ("12a", 3),
        0x0D: ("12a", 3),
        0x0E: ("13a", 3),
        0x0F: ("17b", 2) if short else ("17a", 3),
        0x14: ("18a", 3),
        0x16: ("19a", 3),
        0x17: ("20a", 3),
    }
    if code in table:
        return table[code]
    if 0x10 <= code <= 0x13:
        return "14a", 3
    return ("21/22", 3) if code == 0 else ("??", 3)


def tile(parcels: tuple[int, ...] | list[int]) -> tuple[list[tuple[str, tuple[int, ...]]], bool]:
    """Split parcels into instructions; the flag says the last one ends exactly at the end."""
    out: list[tuple[str, tuple[int, ...]]] = []
    i = 0
    while i < len(parcels):
        nxt = parcels[i + 1] if i + 1 < len(parcels) else None
        name, length = classify(parcels[i], nxt)
        out.append((name, tuple(parcels[i : i + length])))
        i += length
    return out, i == len(parcels)


def parcels_le(data: bytes) -> tuple[int, ...]:
    """Little-endian 16-bit parcels of ``data`` (odd trailing byte dropped)."""
    return struct.unpack(f"<{len(data) // 2}H", data[: len(data) & ~1])


def ureg_load(parcels: tuple[int, ...]) -> tuple[int, int] | None:
    """For a Type 17a/17b instruction return ``(ureg code, immediate)``, else None."""
    name, _ = classify(parcels[0], parcels[1] if len(parcels) > 1 else None)
    if name == "17b" and len(parcels) >= 2:
        return parcels[0] & 0x7F, parcels[1]
    if name == "17a" and len(parcels) >= 3:
        return parcels[0] & 0x7F, (parcels[1] << 16) | parcels[2]
    return None


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("image", type=Path)
    p.add_argument("start", type=lambda s: int(s, 0))
    p.add_argument("end", type=lambda s: int(s, 0))
    args = p.parse_args(argv)
    data = args.image.read_bytes()[args.start : args.end]
    insns, exact = tile(parcels_le(data))
    off = args.start
    for name, ps in insns:
        print(f"0x{off:06x} {name:5s} " + " ".join(f"{x:04x}" for x in ps))
        off += 2 * len(ps)
    print("tiles exactly" if exact else "does NOT tile exactly")
    return 0


if __name__ == "__main__":
    sys.exit(main())
