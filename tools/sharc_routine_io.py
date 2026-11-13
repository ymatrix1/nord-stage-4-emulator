#!/usr/bin/env python3
"""Static data-flow census of PNO core 1 routines from an ADI `elfdump` listing.

For every routine of a flat listing (`tools/adi_dis.py` / `elfdump -ns`, listing address 0 =
pc `0x120200`) the tool collects the *absolute* data addresses the routine touches, which is
what links one slot routine to another through shared state:

* `reads` / `writes`: `dm(0xADDR)` and `pm(0xADDR)` accesses with a literal address,
* `pointers`: literals loaded into an index, base, modify or length register (`i12=0x955b6;`,
  `b6=0xb32da;`; a data register load such as `r0=0xb3300;` is *not* tracked),
* `consts`: float immediates (`rN=0x3f7d70a4`) are decoded and listed.

    python tools/sharc_routine_io.py LISTING.txt 0x121E2B 0x122296 --ends 0x121E2B=0x122293
    python tools/sharc_routine_io.py LISTING.txt 0x121BBE 0x121E2B --shared

Routine bounds: a routine runs from its entry to the instruction before the next entry of the
sorted set, or to the pc given by `--ends ENTRY=END`. `--shared` prints, for the first two
entries A and B, the absolute words A writes and B reads (and the reverse), which is how a
producer / consumer pair shows up.

Provenance: `docs/research/b3-tone-source.md`. Output is addresses and small numbers only.
"""

from __future__ import annotations

import argparse
import re
import struct
import sys
from dataclasses import dataclass, field
from pathlib import Path

LISTING_BASE = (
    0x120200  # VERIFIED (docs/research/organ-tonegen.md: listing address 0 = pc 0x120200)
)

LINE = re.compile(r"^([0-9a-f]{8})\s+((?:[0-9a-f]{4}\s?)+?)\s{2,}(.*)$")
# `dm(0xb33ba)=r2;` write, `r1=dm(0xb33ba);` read (absolute address, no index register)
ABS_ACCESS = re.compile(r"\b(dm|pm)\(0x([0-9a-f]+)\)(\s*=)?", re.I)
# `i12=0x955b6;` `b6=0xb32da;` `m3=0x...` (register immediates)
PTR_LOAD = re.compile(r"\b([ibml]\d{1,2})=0x([0-9a-f]+)\b", re.I)
# `r14=0x3f800000;` data register immediates (candidate float constants)
REG_IMM = re.compile(r"\b[rf]\d{1,2}=0x([0-9a-f]{8})\b", re.I)


@dataclass
class Routine:
    entry: int
    end: int
    reads: set[int] = field(default_factory=set)
    writes: set[int] = field(default_factory=set)
    pointers: set[int] = field(default_factory=set)
    consts: set[float] = field(default_factory=set)
    instructions: int = 0


def parse_listing(text: str) -> list[tuple[int, str]]:
    """`(pc, instruction text)` pairs of an elfdump listing (pc = listing address + base)."""
    out = []
    for line in text.splitlines():
        m = LINE.match(line)
        if m:
            out.append((int(m.group(1), 16) + LISTING_BASE, m.group(3).strip()))
    return out


def as_float(word: int) -> float:
    return struct.unpack("<f", struct.pack("<I", word))[0]


def plausible_float(word: int) -> bool:
    """A float immediate: exponent in a sane range and a short mantissa."""
    exp = (word >> 23) & 0xFF
    return 0x70 <= exp <= 0x88 and (word & 0xFFF) == 0


def scan(lines: list[tuple[int, str]], entry: int, end: int) -> Routine:
    """Collect the absolute accesses of the instructions with `entry <= pc <= end`."""
    r = Routine(entry, end)
    for pc, text in lines:
        if pc < entry or pc > end:
            continue
        r.instructions += 1
        for m in ABS_ACCESS.finditer(text):
            addr = int(m.group(2), 16)
            # a write has the access on the left of `=`: `dm(0x...)=r0`
            (r.writes if m.group(3) else r.reads).add(addr)
        for m in PTR_LOAD.finditer(text):
            r.pointers.add(int(m.group(2), 16))
        for m in REG_IMM.finditer(text):
            w = int(m.group(1), 16)
            if plausible_float(w):
                r.consts.add(round(as_float(w), 6))
    return r


def routines(
    lines: list[tuple[int, str]], entries: list[int], ends: dict[int, int] | None = None
) -> dict[int, Routine]:
    """Scan each entry up to the given end pc (default: the instruction before the next entry)."""
    ends = ends or {}
    ordered = sorted(entries)
    out = {}
    for i, e in enumerate(ordered):
        nxt = ordered[i + 1] - 1 if i + 1 < len(ordered) else max(pc for pc, _ in lines)
        out[e] = scan(lines, e, ends.get(e, nxt))
    return out


def shared(a: Routine, b: Routine) -> dict[str, list[int]]:
    """Words `a` writes that `b` reads (`a_to_b`) and the reverse (`b_to_a`)."""
    return {
        "a_to_b": sorted(a.writes & b.reads),
        "b_to_a": sorted(b.writes & a.reads),
        "both_write": sorted(a.writes & b.writes),
    }


def ranges(words: list[int]) -> str:
    """`0x1-0x3 0x9` run-length text of sorted addresses."""
    out: list[list[int]] = []
    for w in sorted(set(words)):
        if out and w == out[-1][1] + 1:
            out[-1][1] = w
        else:
            out.append([w, w])
    return " ".join(f"{a:#x}" if a == b else f"{a:#x}-{b:#x}" for a, b in out) or "-"


def report(r: Routine) -> str:
    return (
        f"{r.entry:#x}..{r.end:#x}  {r.instructions} instr\n"
        f"  reads    {ranges(sorted(r.reads))}\n"
        f"  writes   {ranges(sorted(r.writes))}\n"
        f"  pointers {ranges(sorted(r.pointers))}\n"
        f"  consts   {sorted(r.consts)}"
    )


def parse_int(s: str) -> int:
    return int(s, 0)


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("listing", type=Path)
    ap.add_argument("entries", nargs="+", type=parse_int)
    ap.add_argument("--ends", help="ENTRY=END,... explicit last pc of a routine")
    ap.add_argument("--shared", action="store_true", help="also print words of the first two")
    ns = ap.parse_args(argv)
    ends = {}
    for item in (ns.ends or "").split(","):
        if item:
            k, v = item.split("=")
            ends[parse_int(k)] = parse_int(v)
    lines = parse_listing(ns.listing.read_text())
    found = routines(lines, ns.entries, ends)
    for r in found.values():
        print(report(r))
    if ns.shared and len(ns.entries) >= 2:
        a, b = (found[e] for e in ns.entries[:2])
        for key, words in shared(a, b).items():
            print(f"{key}: {ranges(words)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
