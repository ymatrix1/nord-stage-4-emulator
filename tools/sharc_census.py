#!/usr/bin/env python3
"""Census of the instruction forms and operations used by the SHARC code in the DSP images.

Scopes the work for a SHARC core model: it counts instruction types and normalised operations
(`R1 = R2 * R3` becomes `X = X * X`) over the code spans of every core section of the FX, PNO and
ORG images. Code spans come from `sharc_visa.sweep` on the flat SRAM image of each core.

    python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DIR
    python tools/sharc_census.py DIR            # directory with dsp_image_<i>_<NAME>.bin
    python tools/sharc_census.py DIR --top 60
"""

from __future__ import annotations

import argparse
import re
import sys
from collections import Counter
from pathlib import Path

import ns4_dsp_ldr as ldr
import sharc_visa as v

SRAM_CODE_START, SRAM_CODE_END = 0x240000, 0x3A0000  # byte offsets of short-word 0x120000..0x1D0000
MIN_HITS = 3


def normalise(op: str) -> str:
    """`IF EQ R1 = R2 * R3 (SSF)` -> `X = X * X (SSF)`; hex numbers become `#`, registers `X`."""
    op = re.sub(r"^IF \w+ ", "", op)
    op = re.sub(r"0x[0-9a-f]+", "#", op)
    return re.sub(r"\b[RFIMLBS]\d+\b", "X", op)


def operations(text: str) -> list[str]:
    """Normalised operations of one decoded instruction (parallel parts split at `, `)."""
    parts: list[str] = []
    depth, start = 0, 0
    for i, ch in enumerate(text):
        depth += ch == "("
        depth -= ch == ")"
        if ch == "," and depth == 0:
            parts.append(text[start:i].strip())
            start = i + 1
    parts.append(text[start:].strip())
    return [normalise(part) for part in parts]


def census_core(mem: bytes) -> tuple[Counter, Counter, int, int]:
    """Type and operation counts plus `(instructions, partly raw)` for one SRAM image."""
    types: Counter = Counter()
    ops: Counter = Counter()
    total = raw = 0
    for lo, hi, hits in v.sweep(mem, SRAM_CODE_START, SRAM_CODE_END):
        if hits < MIN_HITS:
            continue
        insns, _ = v.tile(v.parcels_le(mem[lo:hi]))
        for name, parcels in insns:
            text = v.decode(parcels)
            total += 1
            types[name] += 1
            if "? raw" in text or "(operands raw)" in text:
                raw += 1
                continue
            ops.update(operations(text))
    return types, ops, total, raw


def census_dir(directory: Path) -> tuple[Counter, Counter, int, int, int]:
    """Census over every core section of every `dsp_image_*` file with a loader stream."""
    types: Counter = Counter()
    ops: Counter = Counter()
    total = raw = cores = 0
    for path in sorted(directory.glob("dsp_image_*.bin")):
        img = path.read_bytes()
        blocks, end = ldr.parse_blocks(img)
        if not blocks or end != len(img):
            continue
        for core in ldr.split_cores(blocks):
            t, o, n, r = census_core(bytes(ldr.sram_image(img, core)))
            types.update(t)
            ops.update(o)
            total += n
            raw += r
            cores += 1
    return types, ops, total, raw, cores


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("directory", type=Path)
    p.add_argument("--top", type=int, default=40)
    args = p.parse_args(argv)
    types, ops, total, raw, cores = census_dir(args.directory)
    if not total:
        print("no code found")
        return 1
    print(f"{total} instructions in {cores} core sections, {raw} with raw operands")
    print("types:", ", ".join(f"{k} {n}" for k, n in types.most_common(24)))
    print(f"{len(ops)} distinct normalised operations; top {args.top}:")
    for op, n in ops.most_common(args.top):
        print(f"  {n:7d}  {op}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
