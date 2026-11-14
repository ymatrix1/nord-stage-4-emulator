#!/usr/bin/env python3
"""Generator weights of the B3 Bass (model 5): the state dependent `DM 0x95122 + (note - 36)` words.

`docs/research/organ-farfisa-bass.md` s12. The ARM key-down writes one weight word per footage item
per wheel; the value is a pure function of the wheel and of the number `n` of earlier writes to that
wheel that are still held (bit exact over 297 replayed writes of 9 key sequences):

    weight(note, n) = T[note][n]            (T[note][0] is the first-write amplitude `A`)

A key-up of a key removes its writes one by one and leaves `T[note][max(held - 1, 0)]`.

    uv run --with unicorn --with capstone --with numpy python tools/organ_bass_gen.py capture \
        target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin \
        docs/research/data/bass-gen-weights.csv --cache target/settled.snap

The CSV has two row kinds: `w,note,n,bits` (the table `T`) and `k,key,note,count` (writes per wheel
of one key-down, drawbars `888888888`, all items). `verify` replays sequences against `T`.
"""

from __future__ import annotations

import argparse
import struct
import sys
from collections import Counter, defaultdict
from pathlib import Path

GEN_BASE, GEN_END = 0x95122, 0x95171
FIRST_NOTE = 36


def f32(bits: int) -> float:
    return struct.unpack("<f", struct.pack("<I", bits))[0]


def note_of(addr: int) -> int | None:
    """Wheel note of a generator weight word, `None` for any other address."""
    return addr - GEN_BASE + FIRST_NOTE if GEN_BASE <= addr < GEN_END else None


def split_writes(words: list[tuple[int, int]]) -> list[tuple[int, int]]:
    """`(note, bits)` of the generator weight writes of one key-down, in order."""
    return [(n, b) for a, b in words if (n := note_of(a)) is not None]


def build_tables(
    per_key: dict[int, list[tuple[int, int]]],
) -> tuple[dict[int, list[int]], dict[int, Counter]]:
    """`(T, counts)`: `T[note][n]` bits from an ascending run, `counts[key][note]` writes per key."""
    table: dict[int, list[int]] = defaultdict(list)
    counts: dict[int, Counter] = {}
    for key in sorted(per_key):
        counts[key] = Counter(n for n, _ in per_key[key])
        for note, bits in per_key[key]:
            table[note].append(bits)
    return dict(table), counts


def replay(
    keys: list[int], table: dict[int, list[int]], counts: dict[int, Counter]
) -> dict[int, int]:
    """Final weight bits per wheel after the key-downs of `keys` (any order)."""
    held: Counter = Counter()
    for k in keys:
        held.update(counts[k])
    return {n: table[n][c - 1] for n, c in held.items() if c > 0}


def csv_lines(table: dict[int, list[int]], counts: dict[int, Counter]) -> list[str]:
    out = ["kind,a,b,value"]
    for note in sorted(table):
        out += [f"w,{note},{n},{bits:#010x}" for n, bits in enumerate(table[note])]
    for key in sorted(counts):
        out += [f"k,{key},{note},{c}" for note, c in sorted(counts[key].items())]
    return out


def parse_csv(text: str) -> tuple[dict[int, list[int]], dict[int, Counter]]:
    table: dict[int, list[int]] = defaultdict(list)
    counts: dict[int, Counter] = defaultdict(Counter)
    for line in text.splitlines()[1:]:
        kind, a, b, v = line.split(",")
        if kind == "w":
            table[int(a)].append(int(v, 16))
        else:
            counts[int(a)][int(b)] = int(v)
    return dict(table), dict(counts)


def cmd_capture(a: argparse.Namespace) -> int:  # pragma: no cover - needs the ARM harness
    import arm_edit_capture as aec
    import arm_model_paths as mp
    import arm_steady as ast
    import organ_b3_pass as ob

    emu, _ = aec.settled_emulator(a.code, a.sram, a.cache)
    mp.ensure_scratch(emu)
    store = next(s for s in aec.first_store_per_layer(aec.find_stores(emu)) if s.kind == "organ")
    mp.install_model(emu, store, 5)
    rec = emu.read32(ob.RECORD_TABLE)
    for k, v in enumerate(int(c) for c in a.drawbars):
        emu.write32(rec + ob.DRAWBAR_WORDS + 4 * k, v)
    emu.uc.mem_write(rec + ob.DIRTY, bytes([1]))
    emu.uc.mem_write(rec + ob.BUSY, bytes([1]))
    ast.call_preserving(emu, ob.RECORD_UPDATE, rec, max_insns=20_000_000)
    per_key: dict[int, list[tuple[int, int]]] = {}
    for key in range(a.first, a.last + 1):
        words: list[tuple[int, int]] = []
        for off in (ob.SUB_DRAWBARS, ob.SUB_PERCUSSION):
            emu.clear_messages()
            ast.call_preserving(
                emu, ob.ITEM_KEYDOWN, emu.read32(rec + off), key, 100, 0, max_insns=20_000_000
            )
            words += [
                (int(m.to_dict()["dm"], 16), int(m.to_dict()["w2"], 16))
                for m in emu.messages
                if m.to_dict()["chip"] == "PNO"
            ]
        per_key[key] = split_writes(words)
    table, counts = build_tables(per_key)
    a.csv.write_text("\n".join(csv_lines(table, counts)) + "\n", encoding="utf-8")
    print(f"{sum(map(len, table.values()))} table words, {len(counts)} keys")
    return 0


def cmd_verify(a: argparse.Namespace) -> int:
    table, counts = parse_csv(a.csv.read_text())
    for seq in ([60], [60, 64, 67], [67, 64, 60], list(range(24, 109))):
        w = replay(seq, table, counts)
        print(seq[:3], f"wheel 79: {f32(w[79]):.5f}" if 79 in w else "", f"{len(w)} wheels")
    return 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("capture")
    c.add_argument("code", type=Path)
    c.add_argument("sram", type=Path)
    c.add_argument("csv", type=Path)
    c.add_argument("--drawbars", default="888888888")
    c.add_argument("--first", type=int, default=24)
    c.add_argument("--last", type=int, default=108)
    c.add_argument("--cache", type=Path)
    v = sub.add_parser("verify")
    v.add_argument("csv", type=Path)
    ns = ap.parse_args(argv)
    return cmd_capture(ns) if ns.cmd == "capture" else cmd_verify(ns)


if __name__ == "__main__":
    sys.exit(main())
