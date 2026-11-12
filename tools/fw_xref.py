#!/usr/bin/env python3
"""String and pointer cross-reference helper for the unpacked OS v1.66 `CODE` images.

``bank0_CODE.bin`` loads at ``0xC0000000`` (ARM, little endian). Typical use: find where a UI /
field-name string lives, which 32-bit words point at it, and dump the neighbouring words with the
strings they reference.

    python tools/fw_xref.py target/fw_unpacked/bank0_CODE.bin --string Edt_Syn_kGlideRate
    python tools/fw_xref.py target/fw_unpacked/bank0_CODE.bin --dump 0x1aa590 0x1aa6c0
"""

from __future__ import annotations

import argparse
import struct
import sys
from pathlib import Path

LOAD_BASE = 0xC0000000


def find_string(data: bytes, text: str) -> int | None:
    """File offset of the NUL-delimited string `text`, or None."""
    at = data.find(b"\0" + text.encode("ascii") + b"\0")
    return None if at < 0 else at + 1


def xrefs(data: bytes, file_offset: int, base: int = LOAD_BASE) -> list[int]:
    """File offsets of aligned little-endian words equal to the address of `file_offset`."""
    needle = struct.pack("<I", base + file_offset)
    hits: list[int] = []
    at = data.find(needle)
    while at != -1:
        if at % 4 == 0:
            hits.append(at)
        at = data.find(needle, at + 1)
    return hits


def c_string_at(data: bytes, address: int, base: int = LOAD_BASE, max_len: int = 48) -> str | None:
    """Printable ASCII C string at the absolute `address`, or None."""
    off = address - base
    if not 0 <= off < len(data):
        return None
    end = data.find(b"\0", off)
    raw = data[off:end] if end >= 0 else b""
    if 0 < len(raw) <= max_len and all(32 <= c < 127 for c in raw):
        return raw.decode("ascii")
    return None


def dump_words(
    data: bytes, start: int, end: int, base: int = LOAD_BASE
) -> list[tuple[int, int, str]]:
    """`(file offset, word, referenced string or "")` for each aligned word in `[start, end)`."""
    rows: list[tuple[int, int, str]] = []
    for off in range(start - start % 4, min(end, len(data) - 3), 4):
        word = struct.unpack_from("<I", data, off)[0]
        text = c_string_at(data, word, base) if word >= base else None
        rows.append((off, word, text or ""))
    return rows


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("image", type=Path, help="bank0_CODE.bin")
    p.add_argument("--string", help="find a string and the words that point at it")
    p.add_argument("--dump", nargs=2, metavar=("START", "END"), help="dump words (hex offsets)")
    args = p.parse_args(argv)
    data = args.image.read_bytes()
    if not args.string and not args.dump:
        p.error("give --string and/or --dump")
    if args.string:
        at = find_string(data, args.string)
        if at is None:
            print(f"{args.string!r} not found")
            return 1
        print(f"{args.string} @0x{at:x}  pointed to by {[hex(x) for x in xrefs(data, at)]}")
    if args.dump:
        for off, word, text in dump_words(data, int(args.dump[0], 16), int(args.dump[1], 16)):
            print(f"0x{off:x} {word:08x} {text}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
