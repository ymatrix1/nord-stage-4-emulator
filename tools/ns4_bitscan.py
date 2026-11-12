#!/usr/bin/env python3
"""Corpus bit-scanner for Synth `0x2F` Sound blocks in `.ns4y` / `.ns4p`-family presets.

Reverse-engineering helper: loads every synth preset from a `.ns4b` backup (or a
directory of `.ns4y` files), slices the three 384-bit `0x2F` blocks and reports
per-bit set counts and value histograms for candidate field ranges.

    python tools/ns4_bitscan.py ".reference/Nord Stage 4 Factory Restore RevK v1.66.ns4b" --map
    python tools/ns4_bitscan.py presets_dir/ --hist 318:7:3     # start:width[:count]
"""

from __future__ import annotations

import argparse
import collections
import sys
import zipfile
from pathlib import Path

CBIN_HEADER_SIZE = 48
SYNTH_PAYLOAD_LEN = 493
SOUND_BASES = (1472, 1856, 2240)
SOUND_BITS = 384


def to_bits(payload: bytes) -> str:
    """MSB-first bit string of a payload."""
    return "".join(f"{b:08b}" for b in payload)


def sound_blocks(payload: bytes) -> list[str]:
    """Returns the three 384-bit `0x2F` Sound blocks (layers A, B, C) of a synth payload."""
    if len(payload) < SYNTH_PAYLOAD_LEN:
        raise ValueError(f"synth payload too short: {len(payload)} < {SYNTH_PAYLOAD_LEN}")
    bits = to_bits(payload[:SYNTH_PAYLOAD_LEN])
    return [bits[b : b + SOUND_BITS] for b in SOUND_BASES]


def load_ns4y_dir(path: Path) -> dict[str, bytes]:
    """Loads `*.ns4y` files from a directory, returning name -> 493-byte payload."""
    out: dict[str, bytes] = {}
    for f in sorted(path.glob("*.ns4y")):
        out[f.stem] = f.read_bytes()[CBIN_HEADER_SIZE:]
    return out


def load_ns4b(path: Path) -> dict[str, bytes]:
    """Loads every `.ns4y` synth preset from a `.ns4b` ZIP without extracting to disk."""
    out: dict[str, bytes] = {}
    with zipfile.ZipFile(path, "r") as zf:
        for info in zf.infolist():
            if info.filename.endswith(".ns4y"):
                out[info.filename] = zf.read(info)[CBIN_HEADER_SIZE:]
    return out


def load_presets(path: Path) -> dict[str, bytes]:
    return load_ns4b(path) if path.suffix.lower() == ".ns4b" else load_ns4y_dir(path)


def collect_blocks(presets: dict[str, bytes]) -> list[str]:
    blocks: list[str] = []
    for payload in presets.values():
        blocks.extend(sound_blocks(payload))
    return blocks


def bit_counts(blocks: list[str]) -> list[int]:
    """Number of blocks with each bit set."""
    n = len(blocks[0]) if blocks else 0
    return [sum(1 for b in blocks if b[i] == "1") for i in range(n)]


def bit_map(blocks: list[str]) -> str:
    """One char per bit: `.` never set, `#` always set, `+` mixed (>5%), `o` rare."""
    total = len(blocks)
    thresh = max(1, total // 20)
    chars = []
    for c in bit_counts(blocks):
        if c == 0:
            chars.append(".")
        elif c == total:
            chars.append("#")
        elif thresh < c < total - thresh:
            chars.append("+")
        else:
            chars.append("o")
    return "".join(chars)


def field_histogram(
    blocks: list[str], start: int, width: int, top: int = 8
) -> list[tuple[int, int]]:
    """Most common integer values of the `width`-bit field at `start`."""
    c = collections.Counter(int(b[start : start + width], 2) for b in blocks)
    return c.most_common(top)


def group_histogram(
    blocks: list[str], start: int, width: int, count: int
) -> list[list[tuple[int, int]]]:
    """Histograms of `count` consecutive `width`-bit fields beginning at `start`."""
    return [field_histogram(blocks, start + i * width, width) for i in range(count)]


def fraction_equal(blocks: list[str], start: int, width: int, value: int) -> float:
    """Fraction of blocks whose `width`-bit field at `start` equals `value`."""
    if not blocks:
        return 0.0
    hits = sum(1 for b in blocks if int(b[start : start + width], 2) == value)
    return hits / len(blocks)


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description="Scan Synth 0x2F bit fields across a preset corpus.")
    p.add_argument("path", type=Path, help=".ns4b archive or directory of .ns4y files")
    p.add_argument("--map", action="store_true", help="print 64-bit-per-row variation map")
    p.add_argument(
        "--hist",
        action="append",
        default=[],
        metavar="START:WIDTH[:COUNT]",
        help="value histogram of consecutive fields (repeatable)",
    )
    args = p.parse_args(argv)

    blocks = collect_blocks(load_presets(args.path))
    print(f"{len(blocks)} Sound blocks")
    if args.map:
        m = bit_map(blocks)
        for i in range(0, len(m), 64):
            print(f"{i:3d} {m[i : i + 64]}")
    for spec in args.hist:
        parts = [int(x) for x in spec.split(":")]
        start, width = parts[0], parts[1]
        count = parts[2] if len(parts) > 2 else 1
        for i, h in enumerate(group_histogram(blocks, start, width, count)):
            print(f"+{start + i * width} w{width}: {h}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
