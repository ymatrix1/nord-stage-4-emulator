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
LAYER_BASES = (248, 656, 1064)
LAYER_BITS = 408
BLOCKS = {
    "sound": (SOUND_BASES, SOUND_BITS),  # 0x2F
    "layer": (LAYER_BASES, LAYER_BITS),  # 0x32
}


def to_bits(payload: bytes) -> str:
    """MSB-first bit string of a payload."""
    return "".join(f"{b:08b}" for b in payload)


def synth_blocks(payload: bytes, kind: str = "sound") -> list[str]:
    """Three per-layer blocks (A, B, C): `sound` = 384-bit 0x2F, `layer` = 408-bit 0x32."""
    if len(payload) < SYNTH_PAYLOAD_LEN:
        raise ValueError(f"synth payload too short: {len(payload)} < {SYNTH_PAYLOAD_LEN}")
    bases, width = BLOCKS[kind]
    bits = to_bits(payload[:SYNTH_PAYLOAD_LEN])
    return [bits[b : b + width] for b in bases]


def sound_blocks(payload: bytes) -> list[str]:
    """Returns the three 384-bit `0x2F` Sound blocks (layers A, B, C) of a synth payload."""
    return synth_blocks(payload, "sound")


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


def collect_blocks(presets: dict[str, bytes], kind: str = "sound") -> list[str]:
    blocks: list[str] = []
    for payload in presets.values():
        blocks.extend(synth_blocks(payload, kind))
    return blocks


def associate(
    rows: list[tuple[str, str]], pattern: str, start: int, end: int, widths=(1, 2, 3), top: int = 6
) -> list[tuple[float, int, int, int, int]]:
    """Bit windows whose value over-represents in rows whose name matches `pattern`.

    `rows` = (preset_name, block_bits). Returns (diff, start, width, value, count) best first,
    where diff = P(value | name matches) - P(value | name does not match).
    """
    import re

    pos = [b for n, b in rows if re.search(pattern, n, re.I)]
    neg = [b for n, b in rows if not re.search(pattern, n, re.I)]
    if not pos or not neg:
        return []
    out: list[tuple[float, int, int, int, int]] = []
    for a in range(start, end):
        for w in widths:
            if a + w > end:
                continue
            pc = collections.Counter(int(b[a : a + w], 2) for b in pos)
            nc = collections.Counter(int(b[a : a + w], 2) for b in neg)
            for v, n in pc.items():
                if n >= 3:
                    out.append((n / len(pos) - nc[v] / len(neg), a, w, v, n))
    out.sort(reverse=True)
    return out[:top]


def named_rows(presets: dict[str, bytes], kind: str = "sound") -> list[tuple[str, str]]:
    """(preset stem, block bits) per layer block."""
    rows: list[tuple[str, str]] = []
    for name, payload in presets.items():
        stem = name.rsplit("/", 1)[-1].rsplit(".", 1)[0]
        rows.extend((stem, b) for b in synth_blocks(payload, kind))
    return rows


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
    p.add_argument("--block", choices=sorted(BLOCKS), default="sound", help="0x2F or 0x32 block")
    p.add_argument(
        "--assoc",
        action="append",
        default=[],
        metavar="REGEX:START:END",
        help="bit windows correlated with preset names matching REGEX",
    )
    p.add_argument("--map", action="store_true", help="print 64-bit-per-row variation map")
    p.add_argument(
        "--hist",
        action="append",
        default=[],
        metavar="START:WIDTH[:COUNT]",
        help="value histogram of consecutive fields (repeatable)",
    )
    args = p.parse_args(argv)

    presets = load_presets(args.path)
    blocks = collect_blocks(presets, args.block)
    print(f"{len(blocks)} Sound blocks")
    if args.map:
        m = bit_map(blocks)
        for i in range(0, len(m), 64):
            print(f"{i:3d} {m[i : i + 64]}")
    if args.assoc:
        rows = named_rows(presets, args.block)
        for spec in args.assoc:
            rx, a, e = spec.rsplit(":", 2)
            for diff, st, w, v, n in associate(rows, rx, int(a), int(e)):
                print(f"{rx}: @{st} w{w} val={v} n={n} diff={diff:.2f}")
    for spec in args.hist:
        parts = [int(x) for x in spec.split(":")]
        start, width = parts[0], parts[1]
        count = parts[2] if len(parts) > 2 else 1
        for i, h in enumerate(group_histogram(blocks, start, width, count)):
            print(f"+{start + i * width} w{width}: {h}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
