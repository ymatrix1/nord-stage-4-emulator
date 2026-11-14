#!/usr/bin/env python3
"""Instruction form census of the Leslie processor `0x1256AD..0x126366` (PNO core 1).

Sizes the work of an instruction-level model: it counts the distinct instruction forms (operands
reduced to `R` for a register and `N` for a number) of an ADI listing made with `tools/adi_dis.py`
(`python tools/adi_dis.py core1_sram.bin 0x240000 0x254000 > pno1.txt`, address 0 = pc `0x120000`).

    python tools/fx_leslie_census.py pno1.txt
"""

from __future__ import annotations

import collections
import re
import sys
from pathlib import Path

LO, HI = 0x56AD, 0x6367  # listing offsets of pc 0x1256AD .. 0x126366


def form_key(text: str) -> str:
    """Operands of one instruction reduced to their kind; the parts of a multifunction line join."""
    out = []
    for part in re.split(r",(?![^()]*\))", text):
        t = re.sub(r"0x[0-9a-f]+", "N", part.strip())
        t = re.sub(r"\b[rfmib]\d+\b", "R", t)
        t = re.sub(r"\bs\d+\b", "S", t)
        out.append(t)
    return " , ".join(out)


def census(listing: Path) -> collections.Counter:
    """Form counts of the routine; a listing line is `address parcels... text`."""
    forms: collections.Counter = collections.Counter()
    for line in listing.read_text(encoding="utf-8").splitlines():
        p = line.split(None, 1)
        if len(p) < 2:
            continue
        try:
            addr = int(p[0], 16)
        except ValueError:
            continue
        if LO <= addr < HI:
            text = re.sub(r"^(?:[0-9a-f]{4}\s+)+", "", p[1].strip())
            forms[form_key(text)] += 1
    return forms


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    forms = census(Path((argv or sys.argv[1:])[0]))
    print(sum(forms.values()), "instructions,", len(forms), "forms")
    for k, v in forms.most_common():
        print(v, k)
    return 0


if __name__ == "__main__":
    sys.exit(main())
