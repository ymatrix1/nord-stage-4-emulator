#!/usr/bin/env python3
"""List monotone float runs in the data memory of a DSP flat SRAM image (lookup tables).

Used by docs/research/fx-dsp-map.md section 7. A flat SRAM image (`tools/ns4_dsp_ldr.py --sram`)
stores normal-word address `A` at byte `4 * A`. Interpolation and parameter-map tables show up as
long runs of strictly increasing or decreasing finite floats (`-cos` quarter waves, 128 + 1 entry
control maps). Prints `address length direction first last` for every run of at least `--min`
values.

    python tools/fx_dm_tables.py SRAM/core2_sram.bin --min 100
"""

from __future__ import annotations

import argparse
import math
import struct
import sys
from pathlib import Path

#: Normal-word range backed by SRAM (blocks 1 to 3 of the ADSP-21489).
SRAM_WORDS = range(0x90000, 0x100000)


def floats(data: bytes, lo: int = SRAM_WORDS.start, hi: int = SRAM_WORDS.stop) -> list[float]:
    """The words `lo..hi` of a flat image as float32 values (NaN where the image is shorter)."""
    out = []
    for a in range(lo, hi):
        o = 4 * a
        out.append(struct.unpack_from("<f", data, o)[0] if o + 4 <= len(data) else math.nan)
    return out


def usable(x: float) -> bool:
    return x == x and 1e-30 < abs(x) < 1e20


def runs(values: list[float], base: int, min_len: int) -> list[tuple[int, int, int, float, float]]:
    """(address, length, +1 or -1, first, last) of strictly monotone runs of usable floats."""
    found = []
    i = 0
    n = len(values)
    while i < n:
        if not usable(values[i]):
            i += 1
            continue
        j = i + 1
        sign = 0
        while j < n and usable(values[j]) and values[j] != values[j - 1]:
            s = 1 if values[j] > values[j - 1] else -1
            if sign and s != sign:
                break
            sign = s
            j += 1
        if j - i >= min_len:
            found.append((base + i, j - i, sign, values[i], values[j - 1]))
        i = j if j > i + 1 else i + 1
    return found


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("image", type=Path)
    p.add_argument("--min", type=int, default=100, help="shortest run to print (default 100)")
    args = p.parse_args(argv)
    values = floats(args.image.read_bytes())
    for addr, n, sign, first, last in runs(values, SRAM_WORDS.start, args.min):
        print(f"{addr:#x} {n} {'inc' if sign > 0 else 'dec'} {first:.9g} {last:.9g}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
