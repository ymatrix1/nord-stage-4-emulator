#!/usr/bin/env python3
"""List lookup tables (arrays of smooth 32-bit floats) inside an exported SHARC DSP image.

Input is one image written by `tools/ns4_dsp_images.py --out DIR` (or a slice of `bank0_CODE.bin`).
Heuristic (ASSUMED): a table is a run of at least `--min-len` finite little-endian floats whose
neighbouring differences stay small; runs are cut where the step jumps to more than six times the
median of the previous steps, or where zero padding starts or ends.

    python tools/ns4_dsp_tables.py dsp_image_0_FX.bin
    python tools/ns4_dsp_tables.py dsp_image_0_FX.bin --min-len 100
"""

from __future__ import annotations

import argparse
import statistics
import struct
import sys
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class Table:
    offset: int  # byte offset in the image
    count: int
    first: float
    last: float
    kind: str  # "linear", "exponential" or "curve"


def classify(seg: list[float]) -> str:
    """`linear` if the steps are nearly constant, `exponential` if the ratios are, else `curve`."""
    steps = [b - a for a, b in zip(seg, seg[1:])]
    mean = statistics.mean(steps)
    if abs(mean) > 1e-12 and statistics.pstdev(steps) / abs(mean) < 0.05:
        return "linear"
    if all(x > 0 for x in seg) or all(x < 0 for x in seg):
        ratios = [b / a for a, b in zip(seg, seg[1:])]
        if statistics.pstdev(ratios) / abs(statistics.mean(ratios)) < 0.02:
            return "exponential"
    return "curve"


def _finite(x: float) -> bool:
    return x == x and abs(x) < 1e7 and (x == 0 or abs(x) > 1e-12)


def find_tables(img: bytes, min_len: int = 48) -> list[Table]:
    """Smooth float tables of at least `min_len` entries, in image order."""
    n = len(img) // 4
    w = struct.unpack_from(f"<{n}f", img, 0)
    tables: list[Table] = []
    i = 0
    while i < n:
        if not _finite(w[i]) or w[i] == 0:
            i += 1
            continue
        s = i
        j = i + 1
        while j < n and _finite(w[j]) and w[j] != 0:
            window = [abs(w[k + 1] - w[k]) for k in range(max(s, j - 9), j - 1)]
            ref = statistics.median(window) if len(window) >= 4 else None
            if ref is not None and abs(w[j] - w[j - 1]) > 6 * ref + 1e-6:
                break
            j += 1
        if j - s >= min_len:
            seg = list(w[s:j])
            tables.append(Table(s * 4, j - s, seg[0], seg[-1], classify(seg)))
        i = j
    return tables


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("image", type=Path)
    p.add_argument("--min-len", type=int, default=48)
    args = p.parse_args(argv)
    tables = find_tables(args.image.read_bytes(), args.min_len)
    for t in tables:
        print(f"0x{t.offset:06x} n={t.count:5d} {t.first:12.5g} .. {t.last:12.5g}  {t.kind}")
    print(f"{len(tables)} tables")
    return 0 if tables else 1


if __name__ == "__main__":
    sys.exit(main())
