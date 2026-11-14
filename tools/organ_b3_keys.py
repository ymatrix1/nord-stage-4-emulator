#!/usr/bin/env python3
"""Derive the B3 per-key footage weights from the key-down capture (derived values only).

Input: `docs/research/data/keydown-organ-keys.csv` (model 0 = B3, velocity 100, phase `on`; see
`docs/research/keydown-zones.md` section 6.2). The B3 key-down writes, per footage item, the
weight word `W` of the partial note at `DM 0x951F1 + (note - 36)`; a word written several times in
one burst steps up cumulatively (several footages folded onto one wheel), so the value of one
footage is the increment of its write. The four items of a key come in the order 5 1/3', 8', 4',
16' (the partial notes `key + 7, key, key + 12, key - 12`, verified against the word index of
every row).

    python tools/organ_b3_keys.py derive docs/research/data/keydown-organ-keys.csv \
        > docs/research/data/b3-key-weights.csv
    python tools/organ_b3_keys.py check docs/research/data/keydown-organ-keys.csv \
        docs/research/data/b3-key-weights.csv

Output columns: `note,w16,w5_1_3,w8,w4` (the weight `W` of the footage at the drawbar level of the
captured default program, as the float the ARM sends).
"""

from __future__ import annotations

import csv
import struct
import sys
from collections import defaultdict
from pathlib import Path
from typing import Iterable

W_BASE = 0x951F1  # DM word of the weight of ring note 36 (VERIFIED, keydown-zones.md s6.2)
W_RING_FIRST_NOTE = 36
W_WORDS = 80  # ring notes 36..115
# Item order of the key-down burst and the footage offset in semitones of each item.
ITEM_OFFSETS = (7, 0, 12, -12)  # 5 1/3', 8', 4', 16'
COLUMNS = ("note", "w16", "w5_1_3", "w8", "w4")
FOOTAGE_COLUMN = {-12: "w16", 7: "w5_1_3", 0: "w8", 12: "w4"}
HEADER = ",".join(COLUMNS)


def f32(bits: str) -> float:
    """Float of a hex word `0x3d...`."""
    return struct.unpack("<f", struct.pack("<I", int(bits, 16)))[0]


def fold_wheel(wheel: int) -> int:
    """Fold a wheel index into 12..88 by octaves (VERIFIED, keydown-zones.md s6.2)."""
    while wheel < 12:
        wheel += 12
    while wheel > 88:
        wheel -= 12
    return wheel


def read_bursts(rows: Iterable[dict[str, str]]) -> dict[int, list[tuple[int, float]]]:
    """`{key: [(word, value), ...]}` of the weight words (B3, velocity 100, key-down) in order."""
    out: dict[int, list[tuple[int, float]]] = defaultdict(list)
    for r in rows:
        if r["model"] != "0" or r["velocity"] != "100" or r["phase"] != "on":
            continue
        word = int(r["dm_word"], 16)
        if W_BASE <= word < W_BASE + W_WORDS:
            out[int(r["note"])].append((word, f32(r["value"])))
    return out


def derive_key(note: int, writes: list[tuple[int, float]]) -> dict[str, float]:
    """The four footage weights of one key from its ordered weight writes."""
    if len(writes) != len(ITEM_OFFSETS):
        raise ValueError(f"note {note}: {len(writes)} weight writes, expected 4")
    last: dict[int, float] = {}
    row: dict[str, float] = {}
    for (word, value), off in zip(writes, ITEM_OFFSETS):
        partial = fold_wheel(note + off - 24) + 24
        if word - W_BASE + W_RING_FIRST_NOTE != partial:
            raise ValueError(
                f"note {note} footage {off}: word index {word:#x} is not note {partial}"
            )
        row[FOOTAGE_COLUMN[off]] = round(value - last.get(word, 0.0), 6)
        last[word] = value
    return row


def derive(rows: Iterable[dict[str, str]]) -> list[dict[str, float]]:
    """One row per captured key, sorted by note."""
    out = []
    for note, writes in sorted(read_bursts(rows).items()):
        out.append({"note": note, **derive_key(note, writes)})
    return out


def format_rows(rows: list[dict[str, float]]) -> str:
    lines = [HEADER]
    for r in rows:
        lines.append(",".join([str(r["note"])] + [f"{r[c]:.6f}" for c in COLUMNS[1:]]))
    return "\n".join(lines) + "\n"


def check(derived: list[dict[str, float]], table: Iterable[dict[str, str]]) -> list[str]:
    """Differences between a derived table and a committed CSV (empty = identical)."""
    problems = []
    committed = {int(r["note"]): r for r in table}
    if set(committed) != {r["note"] for r in derived}:
        problems.append("note sets differ")
    for r in derived:
        c = committed.get(r["note"])
        if c is None:
            continue
        for col in COLUMNS[1:]:
            if abs(float(c[col]) - r[col]) > 1e-6:
                problems.append(f"note {r['note']} {col}: {c[col]} vs {r[col]:.6f}")
    return problems


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    args = sys.argv[1:] if argv is None else argv
    if not args or args[0] not in ("derive", "check"):
        print(__doc__)
        return 2
    with Path(args[1]).open(newline="", encoding="utf-8") as fh:
        derived = derive(csv.DictReader(fh))
    if args[0] == "derive":
        sys.stdout.write(format_rows(derived))
        return 0
    with Path(args[2]).open(newline="", encoding="utf-8") as fh:
        problems = check(derived, csv.DictReader(fh))
    print("\n".join(problems) if problems else f"ok: {len(derived)} keys match")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
