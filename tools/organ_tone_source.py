#!/usr/bin/env python3
"""Evidence helpers for docs/research/organ-tone-source.md (which DSP code makes the organ tone).

Three read-only checks on an unpacked `bank0_CODE.bin` (ARM926, loads at 0xC0000000). Nothing
firmware-derived is stored; the tool prints addresses and small statistics only.

    python tools/organ_tone_source.py tables  target/fw_unpacked/bank0_CODE.bin
    python tools/organ_tone_source.py callers target/fw_unpacked/bank0_CODE.bin 0xC022D9F0
    python tools/organ_tone_source.py window  target/fw_unpacked/bank0_CODE.bin LO HI

`tables`   statistics of the two 184-word increment tables the bank-programming routine
           `0xC022D9F0` can write (`0xC0443EA8` and `0xC0444188`).
`callers`  every ARM-mode `bl`/`b` (condition AL) whose target is the given address.
`window`   every 32-bit literal in [lo, hi) that is a DSP system address (`0x28xxxxxx`, core 1 or
           core 2 window) with the DM word it names (`(addr & 0x7FFFFF) / 4`).
"""

from __future__ import annotations

import argparse
import math
import struct
import sys
from pathlib import Path

LOAD_BASE = 0xC0000000
ELEMENTS = 184  # 92 semitones x 2 copies
# Both tables sit in the ARM rodata; `0xC022D9F0` (bank-programming routine) indexes them.
TABLE_A_ADDR = 0xC0443EA8  # written when the level argument is 1
TABLE_B_ADDR = 0xC0444188  # written otherwise


def read_table(code: bytes, addr: int, count: int = ELEMENTS) -> list[int]:
    """Read `count` little-endian u32 words at ARM address `addr`."""
    off = addr - LOAD_BASE
    if off < 0 or off + 4 * count > len(code):
        raise ValueError(f"table at {addr:#x} outside the image")
    return list(struct.unpack_from(f"<{count}I", code, off))


def table_stats(table: list[int]) -> dict[str, float]:
    """Pair structure of an interleaved (copy0, copy1) increment table.

    `spread_rms` is the rms of `1200 log2(copy1 / copy0)` (cents); `step_mean` / `step_sd` describe
    the semitone ladder of the geometric mean of each pair (100 cents per step for 12-TET).
    """
    if len(table) < 4 or len(table) % 2:
        raise ValueError("need an even number of entries (>= 4)")
    pairs = list(zip(table[0::2], table[1::2], strict=True))
    spread = [1200.0 * math.log2(b / a) for a, b in pairs]
    mean_ln = [0.5 * (math.log2(a) + math.log2(b)) for a, b in pairs]
    steps = [1200.0 * (mean_ln[k + 1] - mean_ln[k]) for k in range(len(pairs) - 1)]
    step_mean = sum(steps) / len(steps)
    step_sd = math.sqrt(sum((s - step_mean) ** 2 for s in steps) / len(steps))
    return {
        "pairs": float(len(pairs)),
        "spread_rms": math.sqrt(sum(s * s for s in spread) / len(spread)),
        "step_mean": step_mean,
        "step_sd": step_sd,
    }


def cents_between(a: list[int], b: list[int]) -> tuple[float, float]:
    """Mean and population sd of `1200 log2(b[i] / a[i])` over two equal-length tables."""
    c = [1200.0 * math.log2(y / x) for x, y in zip(a, b, strict=True)]
    mean = sum(c) / len(c)
    return mean, math.sqrt(sum((v - mean) ** 2 for v in c) / len(c))


def find_branches(
    code: bytes, target: int, lo: int = LOAD_BASE, hi: int | None = None
) -> list[int]:
    """ARM addresses of `b`/`bl` (condition AL, opcodes 0xEA / 0xEB) that jump to `target`."""
    end = len(code) if hi is None else min(hi - LOAD_BASE, len(code))
    out: list[int] = []
    for off in range(max(lo - LOAD_BASE, 0) & ~3, end - 3, 4):
        (w,) = struct.unpack_from("<I", code, off)
        if (w >> 24) not in (0xEA, 0xEB):
            continue
        imm = w & 0x00FFFFFF
        if imm & 0x800000:
            imm -= 0x1000000
        if LOAD_BASE + off + 8 + imm * 4 == target:
            out.append(LOAD_BASE + off)
    return out


def window_literals(code: bytes, lo: int, hi: int) -> list[tuple[int, int, int, int]]:
    """(pool address, literal, core 1 or 2, DM word) for DSP system addresses in [lo, hi).

    Core 2 is selected by bit 23 (`0x28800000`), see tools/ns4_dsp_ldr.py.
    """
    out: list[tuple[int, int, int, int]] = []
    for addr in range(lo & ~3, hi, 4):
        off = addr - LOAD_BASE
        if off < 0 or off + 4 > len(code):
            continue
        (w,) = struct.unpack_from("<I", code, off)
        if (w >> 24) == 0x28:
            out.append((addr, w, 2 if w & 0x800000 else 1, (w & 0x7FFFFF) >> 2))
    return out


def _addr(text: str) -> int:
    return int(text, 0)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    for name in ("tables", "callers", "window"):
        p = sub.add_parser(name)
        p.add_argument("code", type=Path)
        if name == "callers":
            p.add_argument("target", type=_addr)
        if name == "window":
            p.add_argument("lo", type=_addr)
            p.add_argument("hi", type=_addr)
    args = ap.parse_args(argv)
    code = args.code.read_bytes()
    if args.cmd == "tables":
        a = read_table(code, TABLE_A_ADDR)
        b = read_table(code, TABLE_B_ADDR)
        for name, t in (("A", a), ("B", b)):
            s = table_stats(t)
            print(
                f"table {name}: copy spread rms {s['spread_rms']:.2f} cent, semitone step "
                f"{s['step_mean']:.2f} +- {s['step_sd']:.2f} cent"
            )
        mean, sd = cents_between(a, b)
        print(f"B against A: mean {mean:.2f} cent, sd {sd:.2f} cent")
    elif args.cmd == "callers":
        for site in find_branches(code, args.target):
            print(f"{site:#010x}")
    else:
        for addr, w, core, dm in window_literals(code, args.lo, args.hi):
            print(f"pool {addr:#010x}  {w:#010x}  core {core}  DM({dm:#x})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
