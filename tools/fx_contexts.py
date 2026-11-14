#!/usr/bin/env python3
"""Join captured ARM -> DSP words to the state block (routine context) they land in.

A DSP effect routine reads its parameters from a **context**: `I1` points at word `c[0]` and the
routine uses `c[-0x20 .. +n]`. The ARM allocates one context per unit instance in a *state table*
(`docs/research/fx-type-map.md` s2: 16-byte rows `{target, base, size, tag}` with `base` a system
address `0x28000000 + 4 w` (core 1) or `0x28800000 + 4 w` (core 2)). `blocks` reads every state
table of the 33 descriptor lists of the firmware; `locate` finds the row that owns a DM word and
returns the **context offset** `o = word - (row base + 0x20)`, i.e. the `c[o]` of the models in
`docs/research/fx-routine-models.md`.

`join_capture` reads a CSV of `tools/fx_clients.py` and writes one line per (id, value, word) with
the block and the context offset. Addresses and values only.
"""

from __future__ import annotations

import csv
import struct
import sys
from collections.abc import Callable, Iterable
from dataclasses import dataclass
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import fx_type_map as ftm

# VERIFIED (`docs/research/fx-contexts.md` s3): the context pointer `I1` of a routine is the row
# base plus 0x20 words (every routine uses `c[-0x20]` as its first word and the state tables are
# sized for `c[-0x20 .. c_max]`: LFO 54 words, delay 70 words).
CONTEXT_BIAS = 0x20
# VERIFIED (42 state tables of `fx-type-map.md` s2, run): row targets 0 and 2 address core 1
# (`0x28000000 + 4 w`; target 2 rows sit at the top of the core 1 DM, e.g. delay rows 10 and 11),
# target 1 rows address core 2 (`0x28800000 + 4 w`).
TARGET_CORE = {0: 1, 1: 2, 2: 1}
SYS_BASE = {1: 0x28000000, 2: 0x28800000}


@dataclass(frozen=True)
class Block:
    """One state row: the DM words `[word0, word0 + words)` of `core` (1 or 2)."""

    list_addr: int
    entry: int
    table: int
    row: int
    core: int
    word0: int
    words: int

    def offset(self, dm_word: int) -> int:
        """Context offset `o` (`c[o]`) of `dm_word`."""
        return dm_word - self.word0 - CONTEXT_BIAS


def words_of_size(size: int) -> int:
    """EMPIRICAL (delay 0x463, LFO 0x353, resonant 0x183): a row has `(size >> 4) + 1` words
    at most; the row stride in the table (`stride`) decides the exact count."""
    return (size >> 4) + 1


def blocks(read32: Callable[[int], int]) -> list[Block]:
    """Every state row of every descriptor list (rows of tables that hold code are skipped)."""
    out: list[Block] = []
    for dlist in ftm.find_descriptor_lists(read32):
        for entry, d in enumerate(dlist):
            if ftm.code_rows(read32, d.table, d.rows):
                continue
            recs = [[read32(d.table + 16 * k + 4 * j) for j in range(4)] for k in range(d.rows)]
            if any(r[0] not in TARGET_CORE or r[1] < 0x28000000 for r in recs):
                continue
            bases = sorted(r[1] for r in recs)
            gaps = [b - a for a, b in zip(bases, bases[1:], strict=False) if b > a]
            stride = min(gaps) // 4 if gaps else 0
            for k, (target, base, size, _tag) in enumerate(recs):
                words = min(words_of_size(size), stride) if stride else words_of_size(size)
                core = TARGET_CORE[target]
                word0 = (base - SYS_BASE[core]) // 4
                out.append(Block(dlist[0].addr, entry, d.table, k, core, word0, words))
    return out


def locate(table: list[Block], core: int, dm_word: int) -> Block | None:
    """The state row that owns `dm_word` of `core` (None when no row does)."""
    for b in table:
        if b.core == core and b.word0 <= dm_word < b.word0 + b.words:
            return b
    return None


def f32(word: int) -> float:
    return struct.unpack("<f", struct.pack("<I", word & 0xFFFFFFFF))[0]


def f32_word(value: float) -> int:
    """Bit pattern of `value` as an IEEE single."""
    return struct.unpack("<I", struct.pack("<f", value))[0]


def join_rows(table: list[Block], rows: Iterable[dict[str, str]]) -> list[dict[str, str]]:
    """Rows of a `fx_clients` CSV with the owning block and the context offset appended."""
    out = []
    for r in rows:
        core = int(r["core"])
        word = int(r["dm_word"], 0)
        b = locate(table, core, word)
        out.append(
            {
                **r,
                "list": f"{b.list_addr:#x}" if b else "",
                "entry": str(b.entry) if b else "",
                "row": str(b.row) if b else "",
                "ctx": str(b.offset(word)) if b else "",
            }
        )
    return out


def type_modules(joined: Iterable[dict[str, str]]) -> dict[tuple, dict[tuple, dict[str, str]]]:
    """Modules touched by the type switch: `{(kind, slot, type): {(list, entry): {ctx: word}}}`.

    Only rows of stage `type` count; the last word written to a context offset wins."""
    out: dict[tuple, dict[tuple, dict[str, str]]] = {}
    for r in joined:
        if r["stage"] != "type":
            continue
        mod = out.setdefault((r["kind"], r["slot"], r["type"]), {})
        mod.setdefault((r["list"], r["entry"]), {})[r["ctx"]] = r["word"]
    return out


def param_series(joined: Iterable[dict[str, str]]) -> dict[tuple, dict[int, int]]:
    """Samples of every context word under a tuning id:
    `{(kind, slot, type, id, list, entry, ctx): {value: word}}` (last message per value wins)."""
    out: dict[tuple, dict[int, int]] = {}
    for r in joined:
        if r["stage"] != "param" or not r["list"]:
            continue
        key = (r["kind"], r["slot"], r["type"], r["id"], r["list"], r["entry"], r["ctx"])
        out.setdefault(key, {})[int(r["value"])] = int(r["word"], 0)
    return out


def fit_rows(series: dict[tuple, dict[int, int]]) -> list[dict[str, str]]:
    """One CSV row per series with the closed form of `fx_fit.fit_series`."""
    import fx_fit as ff

    out = []
    for key, samples in sorted(series.items(), key=lambda kv: tuple(map(str, kv[0]))):
        values = sorted(samples)
        fit = ff.fit_series(values, [samples[v] for v in values])
        out.append(
            {
                "kind": key[0],
                "slot": key[1],
                "type": key[2],
                "id": key[3],
                "list": key[4],
                "entry": key[5],
                "ctx": key[6],
                "n": str(len(values)),
                "form": fit.kind,
                "float": "1" if fit.is_float else "0",
                "a": f"{fit.a:.9g}",
                "b": f"{fit.b:.9g}",
                "c": f"{fit.c:.9g}",
                "err": f"{fit.err:.2g}",
                "samples": ";".join(f"{v}:{samples[v]:#010x}" for v in values),
            }
        )
    return out


def read_capture(path: Path) -> list[dict[str, str]]:
    with path.open(encoding="utf-8", newline="") as fh:
        return list(csv.DictReader(fh))


def type_table(
    tm: dict[tuple, dict[tuple, dict[str, str]]], pcs: dict[str, dict[int, list[int]]]
) -> list[dict[str, str]]:
    """Flat rows of `type_modules`: the module (descriptor list) each type switch writes, its
    routines (`fx_type_map.class_pcs`) and how many context words the switch sends to it."""
    out = []
    for (kind, slot, typ), mods in sorted(
        tm.items(), key=lambda kv: (kv[0][0], kv[0][1], kv[0][2].zfill(3))
    ):
        for (lst, entry), words in sorted(mods.items()):
            per_core = pcs.get(lst, {})
            out.append(
                {
                    "kind": kind,
                    "slot": slot,
                    "type": typ,
                    "list": lst,
                    "entry": entry,
                    "words": str(len(words)),
                    "core1_pcs": " ".join(f"{p:#x}" for p in per_core.get(1, [])),
                    "core2_pcs": " ".join(f"{p:#x}" for p in per_core.get(2, [])),
                }
            )
    return out


def write_rows(path: Path, rows: list[dict[str, str]]) -> None:
    with path.open("w", encoding="utf-8", newline="") as fh:
        w = csv.DictWriter(fh, fieldnames=list(rows[0]))
        w.writeheader()
        w.writerows(rows)


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - needs unicorn + firmware
    import argparse

    import arm_boot as ab

    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("code", type=Path)
    ap.add_argument("sram", type=Path)
    ap.add_argument("capture", type=Path, nargs="+")
    ap.add_argument("--out", type=Path, help="every message with its block and context offset")
    ap.add_argument("--types", type=Path, help="type -> module table")
    ap.add_argument("--fits", type=Path, help="closed form of every context word under an id")
    ns = ap.parse_args(argv)
    emu = ab._fresh(ns.code, ns.sram)
    rows = [r for path in ns.capture for r in read_capture(path)]
    joined = join_rows(blocks(emu.read32), rows)
    if ns.out:
        write_rows(ns.out, joined)
    if ns.types:
        pcs = ftm.class_pcs(ftm.inventory_rows(emu.read32))
        write_rows(ns.types, type_table(type_modules(joined), pcs))
    if ns.fits:
        write_rows(ns.fits, fit_rows(param_series(joined)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
