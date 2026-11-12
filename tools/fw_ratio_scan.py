#!/usr/bin/env python3
"""Find tables of pitch-ratio constants (`2^(cents/1200)` as f32) in the firmware image.

A cluster of floats that are all whole or half cents from unity (`0.9971`, `1.0029`, ...) is a
detune table. Found so far in ``bank0_CODE.bin @0x1ca2a0``: ``+/-5, 15, 25 ... 75`` cents (see
``docs/research/synth-0x2f-ns4decode-crosscheck.md``).

    python tools/fw_ratio_scan.py target/fw_unpacked/bank0_CODE.bin
"""

from __future__ import annotations

import argparse
import math
import struct
import sys
from pathlib import Path


def ratio_to_cents(ratio: float) -> float:
    return 1200.0 * math.log2(ratio)


def cents_to_ratio(cents: float) -> float:
    return 2.0 ** (cents / 1200.0)


def find_cent_constants(
    image: bytes, step: float = 0.5, tol: float = 0.002, max_cents: float = 100.0
) -> list[tuple[int, float]]:
    """`(offset, cents)` of aligned f32 words that are within `tol` of a multiple of `step` cents.

    Unity (0 cents) and values outside `+/- max_cents` are ignored.
    """
    out = []
    usable = len(image) - len(image) % 4
    for off in range(0, usable, 4):
        (v,) = struct.unpack_from("<f", image, off)
        if not (math.isfinite(v) and 0.9 < v < 1.1) or v == 1.0:
            continue
        cents = ratio_to_cents(v)
        if abs(cents) > max_cents:
            continue
        nearest = round(cents / step) * step
        if nearest != 0.0 and abs(cents - nearest) < tol:
            out.append((off, nearest))
    return out


def clusters(
    hits: list[tuple[int, float]], gap: int = 64, min_size: int = 4
) -> list[list[tuple[int, float]]]:
    """Group hits whose offsets are at most `gap` bytes apart; keep groups of `min_size`+."""
    groups: list[list[tuple[int, float]]] = []
    for hit in hits:
        if groups and hit[0] - groups[-1][-1][0] <= gap:
            groups[-1].append(hit)
        else:
            groups.append([hit])
    return [g for g in groups if len(g) >= min_size]


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("image", type=Path)
    p.add_argument("--min-size", type=int, default=4)
    args = p.parse_args(argv)
    for group in clusters(find_cent_constants(args.image.read_bytes()), min_size=args.min_size):
        values = ", ".join(f"{c:+g}" for _, c in group)
        print(f"0x{group[0][0]:x}..0x{group[-1][0]:x} ({len(group)}): {values} cents")
    return 0


if __name__ == "__main__":
    sys.exit(main())
