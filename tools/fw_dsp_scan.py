#!/usr/bin/env python3
"""Scan firmware blobs for SHARC (ADSP-21489) code or ADI loader streams.

Heuristics only (ASSUMED): a hit is a lead, not proof.

    python tools/fw_dsp_scan.py target/fw_unpacked/bank0_CODE.bin --window 0x10000
    python tools/fw_dsp_scan.py target/fw_unpacked/bank0_CODE.bin --ldr

* ``entropy``: Shannon entropy in bits per byte (random or compressed data is above 7.9).
* ``period_contrast``: byte-position entropy at period 6 (SHARC 48-bit words) vs periods 5 and 7.
  Positive = fixed-width structure. ARM code scores about 0.5 too, so it is not a SHARC test alone;
  compare ``period_score`` at 4, 6 and 8 instead.
* ``ldr_candidates``: aligned word triples ``(tag, count, address)`` shaped like an ADI block
  header: address in ADSP-21489 internal RAM (``0x00080000..0x000FFFFF``), small non-zero count.
  ASSUMED shape: the exact 21489 loader block layout is not in the hardware reference.
"""

from __future__ import annotations

import argparse
import math
import struct
import sys
from collections import Counter
from pathlib import Path

L1_LO, L1_HI = 0x00080000, 0x00100000


def entropy(data: bytes) -> float:
    """Shannon entropy in bits per byte (0.0 for empty input)."""
    if not data:
        return 0.0
    n = len(data)
    return -sum(c / n * math.log2(c / n) for c in Counter(data).values())


def period_score(data: bytes, period: int) -> float:
    """Sum of per-position byte entropy for positions modulo ``period`` (lower = more structure)."""
    return sum(entropy(data[i::period]) for i in range(period))


def period_contrast(data: bytes, period: int = 6) -> float:
    """Mean per-byte score at ``period - 1`` and ``period + 1`` minus the score at ``period``.

    Positive means the stream has structure at ``period`` that its non-multiple neighbours lack
    (fixed-width words). Near zero or negative means no such structure.
    """
    mid = period_score(data, period) / period
    near = (
        period_score(data, period - 1) / (period - 1)
        + period_score(data, period + 1) / (period + 1)
    ) / 2
    return near - mid


def ldr_candidates(data: bytes, max_count: int = 0x10000, byteorder: str = "<") -> list[int]:
    """Offsets of aligned ``(tag, count, address)`` triples with address in 21489 internal RAM."""
    fmt = byteorder + "III"
    hits = []
    for off in range(0, len(data) - 11, 4):
        tag, count, addr = struct.unpack_from(fmt, data, off)
        if L1_LO <= addr < L1_HI and 0 < count <= max_count and tag not in (0, 0xFFFFFFFF):
            hits.append(off)
    return hits


def windows(data: bytes, size: int) -> list[tuple[int, float, float]]:
    """``(offset, entropy, 6-byte contrast)`` per window of ``size`` bytes."""
    return [
        (o, entropy(data[o : o + size]), period_contrast(data[o : o + size]))
        for o in range(0, len(data), size)
    ]


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("image", type=Path)
    p.add_argument("--window", type=lambda s: int(s, 0), default=0x10000)
    p.add_argument("--ldr", action="store_true", help="list ADI loader header candidates")
    args = p.parse_args(argv)
    data = args.image.read_bytes()
    if args.ldr:
        for bo in ("<", ">"):
            hits = ldr_candidates(data, byteorder=bo)
            print(f"byteorder {bo}: {len(hits)} candidates", [hex(h) for h in hits[:10]])
        return 0
    for off, h, per in windows(data, args.window):
        print(f"0x{off:08x} entropy {h:5.2f} contrast6 {per:6.3f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
