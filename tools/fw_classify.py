#!/usr/bin/env python3
"""Classify an unpacked firmware `CODE` image block by block.

Heuristics label every 4 KiB block as one of ``zero``, ``text``, ``arm`` (ARM-mode code),
``ptr`` (pointer table into the image), ``float`` (f32 tables), ``pcm16`` (audio-like int16)
or ``other``. Runs of ``other`` are the candidates for data we do not understand yet, e.g.
SHARC DSP code (see ``docs/research/synth-0x2f-ns4decode-crosscheck.md``).

    python tools/fw_classify.py target/fw_unpacked/bank0_CODE.bin
"""

from __future__ import annotations

import argparse
import collections
import math
import struct
import sys
from pathlib import Path

BLOCK = 4096
LOAD_BASE = 0xC0000000


def entropy(data: bytes) -> float:
    counts = collections.Counter(data)
    n = len(data)
    return -sum(c / n * math.log2(c / n) for c in counts.values())


def _words(block: bytes) -> tuple[int, ...]:
    return struct.unpack_from(f"<{len(block) // 4}I", block)


def _is_plausible_float(word: int) -> bool:
    if word == 0:
        return True
    value = struct.unpack("<f", struct.pack("<I", word))[0]
    return math.isfinite(value) and 1e-6 < abs(value) < 1e6


def _pcm_correlation(block: bytes) -> float:
    s = struct.unpack_from(f"<{len(block) // 2}h", block)
    a, b = s[:-1], s[1:]
    ma, mb = sum(a) / len(a), sum(b) / len(b)
    cov = sum((x - ma) * (y - mb) for x, y in zip(a, b, strict=True))
    va, vb = sum((x - ma) ** 2 for x in a), sum((y - mb) ** 2 for y in b)
    return cov / math.sqrt(va * vb) if va > 0 and vb > 0 else 0.0


def classify_block(block: bytes, image_size: int) -> str:
    """Label one block (length must be a multiple of 4)."""
    if block.count(0) > 0.9 * len(block):
        return "zero"
    if sum(1 for c in block if 32 <= c < 127 or c in (0, 9, 10, 13)) > 0.9 * len(block):
        return "text"
    words = _words(block)
    n = len(words)
    if sum(1 for w in words if (w >> 28) == 0xE) > 0.45 * n:
        return "arm"
    if sum(1 for w in words if LOAD_BASE <= w < LOAD_BASE + image_size) > 0.3 * n:
        return "ptr"
    nonzero = [w for w in words if w]
    plausible = sum(1 for w in words if _is_plausible_float(w))
    if len(nonzero) > 0.3 * n and plausible > 0.65 * n:
        return "float"
    if _pcm_correlation(block) > 0.6 and entropy(block) > 4.5:
        return "pcm16"
    return "other"


def classify_image(image: bytes) -> list[tuple[int, str]]:
    """`(offset, class)` for every block of the image."""
    usable = len(image) - len(image) % 4
    out = []
    for off in range(0, usable, BLOCK):
        block = image[off : min(off + BLOCK, usable)]
        out.append((off, classify_block(block, len(image))))
    return out


def merge_runs(labels: list[tuple[int, str]]) -> list[tuple[int, int, str]]:
    """Merge consecutive equal labels into `(start, end, class)`."""
    runs: list[list] = []
    for off, cls in labels:
        if runs and runs[-1][2] == cls:
            runs[-1][1] = off + BLOCK
        else:
            runs.append([off, off + BLOCK, cls])
    return [(a, b, c) for a, b, c in runs]


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("image", type=Path)
    p.add_argument("--min-kib", type=int, default=16, help="hide runs smaller than this")
    args = p.parse_args(argv)
    image = args.image.read_bytes()
    runs = merge_runs(classify_image(image))
    totals: collections.Counter[str] = collections.Counter()
    for a, b, c in runs:
        totals[c] += b - a
        if (b - a) >= args.min_kib * 1024:
            print(f"0x{a:07x}..0x{b:07x} {(b - a) // 1024:5d} KiB  {c}")
    print("totals:", {k: f"{v / 1024:.0f} KiB" for k, v in totals.most_common()})
    return 0


if __name__ == "__main__":
    sys.exit(main())
