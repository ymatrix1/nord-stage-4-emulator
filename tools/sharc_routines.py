#!/usr/bin/env python3
"""Routine census of an ADI listing of SHARC VISA code.

Used by docs/research/pno-dsp-map.md and sy-dsp-map.md.

Input is the output of `tools/adi_dis.py` (`ADDR  parcels  text`, one instruction per line, address
relative to the start of the disassembled range). The code is cut into routines after every
`rts`/`rti` (a delayed return takes its two delay-slot instructions along) and each routine gets a
signature: instruction count and the counts of

| key      | what is counted                                                              |
|----------|------------------------------------------------------------------------------|
| mul      | multiplies (`fx=fy*fz`, `rx=ry*rz`)                                          |
| biquad   | two consecutive multiplies by one register, consecutive left operands        |
|          | (`f12=f2*f7`, `f13=f3*f7`: `a1*y`, `a2*y` of a transposed direct form II)    |
| fir      | `pm(...)` coefficient fetches fused with a multiply (`fx=fy*fz, ... pm(`)    |
| pm, dm   | program- and data-memory accesses                                            |
| recips   | `RECIPS` / `RSQRTS` seeds (division, square root)                            |
| loop     | `DO ... UNTIL` loops                                                          |
| call     | direct and indirect calls                                                    |
| circ     | writes to `L`/`B` registers (circular buffers)                               |
| clip     | `CLIP`, `MIN`, `MAX`                                                         |
| cvt      | `FIX`, `FLOAT`, `TRUNC`, `FUNPACK`, `FPACK`                                  |
| bitf     | `FEXT`, `FDEP`, `BITEXT`, `BITDEP`                                            |

The counts are textual (EMPIRICAL): they classify code by shape, they do not prove behaviour.

    python tools/adi_dis.py SRAM.bin 0x240600 0x24c700 > listing.txt
    python tools/sharc_routines.py listing.txt 0x120300 --min 30
"""

from __future__ import annotations

import argparse
import re
import sys
from collections import Counter
from pathlib import Path

LINE = re.compile(r"^([0-9a-f]{8})\s{2}((?:[0-9a-f]{4} ?)+)\s{2,}(.*?)\s*$")
RET = re.compile(r"^(rts|rti)\b")
MUL = re.compile(r"\b[fr]\d+=[fr]\d+\*[fr]\d+")
PAIR = re.compile(r"\bf(\d+)=f(\d+)\*f(\d+)\b")


def parse_listing(text: str, base: int = 0) -> list[tuple[int, str]]:
    """`(short-word address, instruction text)` for every instruction line of an ADI listing."""
    out = []
    for line in text.splitlines():
        m = LINE.match(line)
        if m:
            out.append((int(m.group(1), 16) + base, m.group(3)))
    return out


def split_routines(ins: list[tuple[int, str]]) -> list[list[tuple[int, str]]]:
    """Cut after each return; `rts (db)` keeps its two delay slots."""
    routines: list[list[tuple[int, str]]] = []
    cur: list[tuple[int, str]] = []
    delay = -1
    for item in ins:
        cur.append(item)
        if delay >= 0:
            delay -= 1
            if delay < 0:
                routines.append(cur)
                cur = []
            continue
        if RET.match(item[1]):
            if "(db" in item[1]:
                delay = 1
            else:
                routines.append(cur)
                cur = []
    if cur:
        routines.append(cur)
    return routines


def biquad_pairs(texts: list[str]) -> int:
    """Consecutive multiplies `fA=fB*fY`, `fA+1=fB+1*fY` (the feedback pair of a biquad section)."""
    n = 0
    prev = None
    for t in texts:
        # a multifunction line may carry several operations; the multiply is the first one
        m = PAIR.search(t)
        if (
            m
            and prev
            and int(m.group(1)) == prev[0] + 1
            and int(m.group(2)) == prev[1] + 1
            and m.group(3) == prev[2]
        ):
            n += 1
            prev = None
            continue
        prev = (int(m.group(1)), int(m.group(2)), m.group(3)) if m else None
    return n


def signature(routine: list[tuple[int, str]]) -> Counter:
    """Feature counts of one routine (keys in the module docstring)."""
    texts = [t for _, t in routine]
    f: Counter = Counter()
    f["n"] = len(texts)
    f["biquad"] = biquad_pairs(texts)
    for t in texts:
        f["mul"] += bool(MUL.search(t))
        f["fir"] += bool(MUL.search(t)) and "pm(" in t
        f["pm"] += "pm(" in t
        f["dm"] += "dm(" in t
        f["recips"] += bool(re.search(r"recips|rsqrts", t))
        f["loop"] += "do (" in t
        f["call"] += "call" in t
        f["circ"] += bool(re.match(r"^[lb]\d+=", t))
        f["clip"] += bool(re.search(r"clip|min\(|max\(", t))
        f["cvt"] += bool(re.search(r"=(fix|float|trunc|funpack|fpack)\b", t))
        f["bitf"] += bool(re.search(r"fext|fdep|bitext|bitdep", t))
    return f


def census(text: str, base: int, min_len: int = 1) -> list[tuple[int, int, Counter]]:
    """`(first, last address, signature)` of every routine with at least `min_len` instructions."""
    rows = []
    for r in split_routines(parse_listing(text, base)):
        if len(r) >= min_len:
            rows.append((r[0][0], r[-1][0], signature(r)))
    return rows


KEYS = ("mul", "biquad", "fir", "pm", "dm", "recips", "loop", "call", "circ", "clip", "cvt", "bitf")


def format_rows(rows: list[tuple[int, int, Counter]]) -> str:
    """Fixed-width table, one routine per line."""
    head = "first    last       n " + " ".join(f"{k:>6}" for k in KEYS)
    lines = [head]
    for a, b, f in rows:
        lines.append(f"{a:06x}-{b:06x} {f['n']:5d} " + " ".join(f"{f[k]:6d}" for k in KEYS))
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("listing", type=Path, help="output of tools/adi_dis.py")
    p.add_argument(
        "base", help="short-word address of the first listed instruction (for example 0x120300)"
    )
    p.add_argument("--min", type=int, default=15, help="smallest routine to print (instructions)")
    args = p.parse_args(argv)
    rows = census(args.listing.read_text(), int(args.base, 0), args.min)
    print(format_rows(rows))
    return 0


if __name__ == "__main__":
    sys.exit(main())
