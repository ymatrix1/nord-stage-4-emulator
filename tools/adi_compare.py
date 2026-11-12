#!/usr/bin/env python3
"""Compare the SHARC decoder of this project with ADI's `elfdump` disassembly.

    python tools/adi_compare.py IMAGE BYTE_START BYTE_END [--cces DIR] [--examples N]

Both listings are reduced to a canonical form (lower case, no spaces, numbers as signed
integers, memory operands in `dm(modify,index)` order, comma separated parts sorted) and compared
per instruction, grouped by the instruction type of `tools/sharc_visa.py`. Differences point to
wrong field positions or missing forms (`docs/research/dsp-image-hunt.md` 5.34).
"""

import argparse
import collections
import re
import sys
from pathlib import Path

import adi_dis
import sharc_visa as v

# numbers that are not part of a register name (r10, i7, ...)
NUMBER = re.compile(r"(?<![a-z0-9])-?0x[0-9a-f]+|(?<![a-z0-9])-?\d+")


def _number(m: re.Match) -> str:
    text = m.group(0)
    n = int(text, 16) if "0x" in text else int(text, 10)
    if n >= 0x80000000 and "0x" in text:
        n -= 1 << 32  # 32-bit two's complement
    return str(n)


def split_parts(text: str) -> list[str]:
    """Split on commas that are not inside parentheses."""
    parts, depth, cur = [], 0, ""
    for ch in text:
        depth += ch == "("
        depth -= ch == ")"
        if ch == "," and depth == 0:
            parts.append(cur)
            cur = ""
        else:
            cur += ch
    parts.append(cur)
    return parts


def canonical(text: str) -> str:
    """Canonical form of an instruction text (ADI or this project's)."""
    t = text.lower().strip().rstrip(";")
    t = NUMBER.sub(_number, t)  # before the spaces go: `by 0x18` must not look like a name
    t = re.sub(r"\s+", "", t)
    # a pre-modify access is written `dm(i6,m6)[pre-mod]` here and `dm(m6,i6)` by ADI
    t = re.sub(r"(dm|pm)\((i\d+),(m\d+)\)\[pre-mod\]", r"\1(\3,\2)", t)
    t = t.replace("not_", "not").replace("(lw)", "")
    return ",".join(sorted(p for p in split_parts(t) if p))


def our_listing(parcels: list[int]) -> dict[int, tuple[str, list[int], str]]:
    """This project's decode of a parcel stream: address -> (type, parcels, text)."""
    insns, _exact = v.tile(parcels)
    result, addr = {}, 0
    for name, par in insns:
        result[addr] = (name, list(par), v.decode(tuple(par)))
        addr += len(par)
    return result


def compare(adi: dict, ours: dict) -> tuple[dict, dict]:
    """Per instruction type: `[same, different, tiling mismatch]` counts and examples."""
    stats = collections.defaultdict(lambda: [0, 0, 0])
    examples = collections.defaultdict(list)
    for addr, (par, text) in sorted(adi.items()):
        if addr not in ours or ours[addr][1] != par:
            stats["(tiling)"][2] += 1
            continue
        name, _, our_text = ours[addr]
        if canonical(text) == canonical(our_text):
            stats[name][0] += 1
        else:
            stats[name][1] += 1
            examples[name].append((addr, par, text, our_text))
    return stats, examples


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("image")
    ap.add_argument("start", type=lambda s: int(s, 0))
    ap.add_argument("end", type=lambda s: int(s, 0))
    ap.add_argument("--cces", default=None)
    ap.add_argument("--examples", type=int, default=3)
    args = ap.parse_args()
    parcels = adi_dis.image_parcels(Path(args.image).read_bytes(), args.start, args.end)
    stats, examples = compare(adi_dis.disassemble(parcels, args.cces), our_listing(parcels))
    for name, (same, diff, tile) in sorted(stats.items(), key=lambda kv: -kv[1][1]):
        print(f"{name:8} same {same:6} different {diff:6} tiling {tile:6}")
        for addr, par, adi_text, our_text in examples[name][: args.examples]:
            hexw = " ".join(f"{p:04x}" for p in par)
            print(f"    {addr:06x} {hexw}\n        adi:  {adi_text}\n        ours: {our_text}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
