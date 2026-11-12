#!/usr/bin/env python3
"""Find and export the five SHARC DSP images inside an unpacked Nord Stage 4 `bank0_CODE.bin`.

The OS hands the images to the DSP driver as (pointer, length) pairs held in a literal pool
(`bank0_CODE@0x241DAC..0x241DD4` in v1.64; see docs/research/dsp-image-hunt.md 5.12). The pairs
chain: `pointer + length` of one image equals the pointer of the next. This tool does not rely on
fixed offsets: it looks for the longest such chain of pointer/length word pairs that sit near each
other in the file, so it keeps working when a release moves the pool.

    python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin
    python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out some/dir

Images are listed in address order, which is the download index order 0..4 (FX, PNO, ORG, SY1SY2,
SMP by the name array of the bank 1 test; names are ASSUMED). Do not commit exported images.
"""

from __future__ import annotations

import argparse
import struct
import sys
from collections import defaultdict
from pathlib import Path

LOAD_BASE = 0xC0000000
NAMES = ("FX", "PNO", "ORG", "SY1SY2", "SMP")  # ASSUMED order, see docs 5.12
MIN_LEN, MAX_LEN = 0x400, 0x100000


def find_chain(data: bytes, base: int = LOAD_BASE, window: int = 2) -> list[tuple[int, int]]:
    """Longest chain of `(offset, length)` pairs with `offset + length == next offset`.

    A pair is a pointer word (`base + offset`) and a length word within `window` words of it.
    """
    n = len(data) // 4
    words = struct.unpack_from(f"<{n}I", data, 0)
    ptr_at: dict[int, list[int]] = defaultdict(list)
    for i, w in enumerate(words):
        if base <= w < base + len(data):
            ptr_at[w - base].append(i)
    pairs: dict[int, dict[int, int]] = defaultdict(dict)  # offset -> {length: word distance}
    for off, locs in ptr_at.items():
        for i in locs:
            for j in range(max(0, i - window), min(n, i + window + 1)):
                length = words[j]
                if j != i and MIN_LEN <= length <= MAX_LEN and off + length <= len(data):
                    pairs[off][length] = min(abs(i - j), pairs[off].get(length, window + 1))
    best: list[tuple[int, int]] = []
    best_key = (0, 0)

    def walk(off: int, path: list[tuple[int, int]], dist: int) -> None:
        nonlocal best, best_key
        for length, d in sorted(pairs.get(off, {}).items()):
            nxt = path + [(off, length)]
            key = (len(nxt), -(dist + d))  # longest chain, then the closest pointer/length words
            if key > best_key:
                best, best_key = nxt, key
            walk(off + length, nxt, dist + d)

    for start in sorted(pairs):
        walk(start, [], 0)
    while best and _looks_like_text(data[best[-1][0] : best[-1][0] + 32]):
        best = best[:-1]  # the chain ran on into the UI string table
    return best


def _looks_like_text(head: bytes) -> bool:
    """True when at least 75% of `head` is printable ASCII (a string table, not an image)."""
    return bool(head) and sum(32 <= b < 127 for b in head) * 4 >= len(head) * 3


def export(data: bytes, chain: list[tuple[int, int]], out: Path) -> list[Path]:
    """Write each image of `chain` to `out/dsp_image_<i>_<NAME>.bin` and return the paths."""
    out.mkdir(parents=True, exist_ok=True)
    paths = []
    for i, (off, length) in enumerate(chain):
        name = NAMES[i] if i < len(NAMES) else f"X{i}"
        path = out / f"dsp_image_{i}_{name}.bin"
        path.write_bytes(data[off : off + length])
        paths.append(path)
    return paths


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("image", type=Path, help="unpacked bank0_CODE.bin")
    p.add_argument("--out", type=Path, help="directory to write the images to")
    args = p.parse_args(argv)
    data = args.image.read_bytes()
    chain = find_chain(data)
    if len(chain) < 2:
        print("no pointer/length chain found")
        return 1
    for i, (off, length) in enumerate(chain):
        name = NAMES[i] if i < len(NAMES) else f"X{i}"
        print(f"idx{i} {name:7s} 0x{off:06x}..0x{off + length:06x} ({length} bytes)")
    total = chain[-1][0] + chain[-1][1] - chain[0][0]
    print(f"{len(chain)} images, contiguous, {total} bytes in total")
    if args.out:
        for path in export(data, chain, args.out):
            print("wrote", path)
    return 0


if __name__ == "__main__":
    sys.exit(main())
