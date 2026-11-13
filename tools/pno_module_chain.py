#!/usr/bin/env python3
"""Module directory of the Nord Stage 4 DSP chain: ARM channel rows -> DSP routine -> shape.

The ARM keeps the entry points of the DSP routines it installs at run time as *system addresses*
in its channel-record tables (`docs/research/arm-chan-tables.csv`, `tools/arm_params.py`): a record
`{target, base, size, tag}` with `base = 0x28000000 + 4 * pc` (core 1) or `0x28800000 + 4 * pc`
(core 2) names the routine at program counter `pc` of the target's image. That is why a search for
`0x1206BE`-style immediates in the ARM finds nothing. This tool turns those rows into a module
directory, checks that every entry is an instruction start of the right core, classifies each
routine with the census of `tools/sharc_routines.py` and compares the routine bodies across cores
(FX, PNO, ORG) to transfer roles. Results: `docs/research/pno-module-chain.md`.

    python tools/pno_module_chain.py directory docs/research/arm-chan-tables.csv \\
        --listing 0=fx_c1.txt --listing 1=fx_c2.txt --listing 2=pno_c1.txt [--table 0xc0545148]
    python tools/pno_module_chain.py shared docs/research/arm-chan-tables.csv --target 2 \
        --listing 0=fx_c1.txt --listing 1=fx_c2.txt --listing 2=pno_c1.txt
    python tools/pno_module_chain.py ids bank0_CODE.bin 0x2FA050 74 --listing 5=org_c2.txt
    uv run --with unicorn --with capstone python tools/pno_module_chain.py slots bank0_CODE.bin \\
        bank0_SRAM.bin

`--listing T=FILE` is the output of `tools/adi_dis.py` over `0x240600..` for the core of channel
target T (0 FX core 1, 1 FX core 2, 2 PNO core 1, 3 PNO core 2, 4 ORG core 1, 5 ORG core 2); the
first listed instruction is pc `0x120300`.
"""

from __future__ import annotations

import argparse
import csv
import difflib
import re
import struct
import sys
from collections import Counter
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import sharc_routines as sr

LISTING_BASE = 0x120300
CORE1_BASE = 0x28000000
CORE2_BASE = 0x28800000
# VERIFIED: docs/research/pno-module-chain.md s2 (`directory --check`): the code rows of each
# target are instruction starts of exactly this image and core (116 of 116).
TARGET_NAMES = {0: "FX c1", 1: "FX c2", 2: "PNO c1", 3: "PNO c2", 4: "ORG c1", 5: "ORG c2"}
CODE_LO = 0x120300
CODE_HI = 0x126400
SKELETON_HEX = re.compile(r"0x[0-9a-f]+")
SKELETON_REG = re.compile(r"\b([rfsimlb])(\d+)\b")
FLOAT_IMM = re.compile(r"=(0x[0-9a-f]{8})$")


def row_pc(base: int) -> int | None:
    """Program counter named by a channel-record base address, or None when it is not a code row."""
    if CORE2_BASE <= base < 0x29000000:
        pc = (base - CORE2_BASE) >> 2
    elif CORE1_BASE <= base < CORE2_BASE:
        pc = (base - CORE1_BASE) >> 2
    else:
        return None
    return pc if CODE_LO <= pc < CODE_HI else None


def read_rows(path: Path) -> list[dict[str, str]]:
    with path.open(newline="") as fh:
        return list(csv.DictReader(fh))


def code_rows(rows: list[dict[str, str]]) -> list[dict[str, object]]:
    """Rows whose base is a code address: `{table, index, target, pc}` (table as int)."""
    out = []
    for r in rows:
        pc = row_pc(int(r["base"], 16))
        target = int(r["target"])
        if pc is not None and target in TARGET_NAMES:
            out.append(
                {
                    "table": int(r["table"], 16),
                    "index": int(r["index"]),
                    "target": target,
                    "pc": pc,
                    "size": int(r["size"], 16),
                }
            )
    return out


def load_listing(path: Path) -> list[tuple[int, str]]:
    return sr.parse_listing(path.read_text(), LISTING_BASE)


def routine_at(ins: list[tuple[int, str]], pc: int) -> list[tuple[int, str]]:
    """Instructions from `pc` up to and including its return (a delayed one with its slots)."""
    starts = {a: i for i, (a, _) in enumerate(ins)}
    i = starts.get(pc)
    if i is None:
        return []
    out: list[tuple[int, str]] = []
    delay = -1
    for a, t in ins[i:]:
        out.append((a, t))
        if delay >= 0:
            delay -= 1
            if delay < 0:
                break
            continue
        if sr.RET.match(t):
            if "(db" in t:
                delay = 1
            else:
                break
    return out


def skeleton(routine: list[tuple[int, str]]) -> list[str]:
    """Instruction texts with addresses and registers masked: comparable across images."""
    out = []
    for _, t in routine:
        t = SKELETON_HEX.sub("N", t)
        t = SKELETON_REG.sub(lambda m: m.group(1) + "#", t)
        out.append(t)
    return out


def similarity(a: list[tuple[int, str]], b: list[tuple[int, str]]) -> float:
    """Ratio of the masked instruction streams (1.0 = identical shape)."""
    if not a or not b:
        return 0.0
    return difflib.SequenceMatcher(None, skeleton(a), skeleton(b), autojunk=False).ratio()


def float_constants(routine: list[tuple[int, str]]) -> list[float]:
    """Float immediates loaded by `rN=0x........` in a routine (distinct, order of first use)."""
    seen: list[float] = []
    for _, t in routine:
        m = FLOAT_IMM.search(t)
        if m and re.match(r"^[rf]\d+=0x", t):
            w = int(m.group(1), 16)
            if 0x30000000 <= (w & 0x7FFFFFFF) <= 0x45000000:
                v = struct.unpack("<f", struct.pack("<I", w))[0]
                if v not in seen:
                    seen.append(v)
    return seen


def best_match(
    routine: list[tuple[int, str]], others: dict[int, list[tuple[int, str]]], min_len: int = 8
) -> tuple[int, float] | None:
    """Routine of `others` (entry pc -> routine) with the highest similarity."""
    best: tuple[int, float] | None = None
    for pc, r in others.items():
        if len(r) < min_len or abs(len(r) - len(routine)) > max(4, len(routine) // 2):
            continue
        s = similarity(routine, r)
        if best is None or s > best[1]:
            best = (pc, s)
    return best


def all_routines(ins: list[tuple[int, str]]) -> dict[int, list[tuple[int, str]]]:
    return {r[0][0]: r for r in sr.split_routines(ins) if r}


def find_identical(
    routine: list[tuple[int, str]],
    pools: dict[int, dict[int, list[tuple[int, str]]]],
    threshold: float = 0.999,
) -> tuple[int, int] | None:
    """`(pool key, entry pc)` of the first routine of equal length with a masked stream match."""
    for key, pool in pools.items():
        for pc, other in pool.items():
            if len(other) == len(routine) and similarity(routine, other) >= threshold:
                return key, pc
    return None


def read_id_table(code: bytes, offset: int, count: int) -> list[int]:
    """`count` little-endian u32 module ids at a file offset of `bank0_CODE.bin`."""
    return list(struct.unpack_from(f"<{count}I", code, offset))


def shape(routine: list[tuple[int, str]]) -> str:
    """Short description from the census counts of one routine."""
    f = sr.signature(routine)
    parts = [f"{f['n']} instr", f"{f['mul']} mul"]
    for key in ("biquad", "fir", "recips", "loop", "circ", "clip", "cvt", "bitf", "call"):
        if f[key]:
            parts.append(f"{f[key]} {key}")
    return ", ".join(parts)


def directory(rows: list[dict[str, object]], table: int) -> list[dict[str, object]]:
    """Rows of one channel table sorted by index (module id = row index)."""
    return sorted((r for r in rows if r["table"] == table), key=lambda r: int(r["index"]))  # type: ignore[arg-type]


def triples(rows: list[dict[str, object]]) -> list[list[dict[str, object]]]:
    """Group consecutive rows of the module directory into per-module triples (targets 0, 1, 2)."""
    groups: list[list[dict[str, object]]] = []
    cur: list[dict[str, object]] = []
    last = -1
    for r in rows:
        t = int(r["target"])  # type: ignore[arg-type]
        if cur and t <= last:
            groups.append(cur)
            cur = []
        cur.append(r)
        last = t
    if cur:
        groups.append(cur)
    return groups


def alignment(rows: list[dict[str, object]], listings: dict[int, list[tuple[int, str]]]) -> Counter:
    """Count code rows that are instruction starts of the listing of their own target."""
    starts = {t: {a for a, _ in ins} for t, ins in listings.items()}
    c: Counter = Counter()
    for r in rows:
        t = int(r["target"])  # type: ignore[arg-type]
        if t in starts:
            c[(t, int(r["pc"]) in starts[t])] += 1  # type: ignore[arg-type]
    return c


def cmd_directory(args: argparse.Namespace) -> int:
    rows = code_rows(read_rows(args.csv))
    listings = {}
    for spec in args.listing:
        t, _, path = spec.partition("=")
        listings[int(t)] = load_listing(Path(path))
    if args.check:
        for (t, ok), n in sorted(alignment(rows, listings).items()):
            print(f"target {t} {TARGET_NAMES[t]}: {'aligned' if ok else 'MISALIGNED'} {n}")
        return 0
    table = int(args.table, 16)
    for r in directory(rows, table):
        t = int(r["target"])  # type: ignore[arg-type]
        pc = int(r["pc"])  # type: ignore[arg-type]
        ins = listings.get(t)
        desc = shape(routine_at(ins, pc)) if ins else ""
        print(f"row {r['index']:>2} {TARGET_NAMES[t]:6} {pc:#08x}  {desc}")
    return 0


def cmd_shared(args: argparse.Namespace) -> int:
    """Which ARM-named routines of `--target` also exist, unchanged, in the other listings."""
    rows = [r for r in code_rows(read_rows(args.csv)) if r["target"] == args.target]
    listings = {}
    for spec in args.listing:
        t, _, path = spec.partition("=")
        listings[int(t)] = load_listing(Path(path))
    own = listings[args.target]
    pools = {t: all_routines(ins) for t, ins in listings.items() if t != args.target}
    shared = 0
    for r in rows:
        pc = int(r["pc"])  # type: ignore[arg-type]
        hit = find_identical(routine_at(own, pc), pools)
        shared += hit is not None
        where = f"= {TARGET_NAMES[hit[0]]} {hit[1]:#08x}" if hit else "only here"
        print(f"{int(r['table']):#x}:{r['index']:<2} {pc:#08x} {where}")  # type: ignore[call-overload]
    print(f"{shared} of {len(rows)} rows have an identical routine elsewhere")
    return 0


def cmd_ids(args: argparse.Namespace) -> int:
    code = args.image.read_bytes()
    ids = read_id_table(code, int(args.offset, 0), args.count)
    ins = None
    if args.listing:
        t, _, path = args.listing[0].partition("=")
        ins = load_listing(Path(path))
    for i, pc in enumerate(ids):
        desc = ""
        if ins:
            rt = routine_at(ins, pc)
            desc = shape(rt) if rt else "NOT-A-ROUTINE-START"
        print(f"{i:3d} {pc:#08x} {desc}")
    return 0


def cmd_slots(args: argparse.Namespace) -> int:  # pragma: no cover - needs unicorn + firmware
    """Run the PNO slot-installer state machine on synthetic objects (see the research note)."""
    import arm_capture as ac

    emu = ac.new_session(args.image, args.sram)
    obj = 0x80010000
    print("state kind event slot -> DM(0xC01D6+4 slot) value [+ other messages]")
    for state in (0, 1, 2):
        for kind in range(6):
            for ev in range(6):
                emu.write_bytes(obj, bytes(0x400))
                for off, v in ((0xB0, state), (0x8C, kind), (0x90, ev), (0x3C, args.slot)):
                    emu.write32(obj + off, v)
                emu.clear_messages()
                try:
                    emu.call(SLOT_FSM, obj, max_insns=400_000)
                except Exception:  # unicorn error on an unmodelled path
                    continue
                msgs = emu.messages
                slot_w = [m for m in msgs if m.word1 == SLOT_TABLE_BASE + 16 * args.slot]
                other = len(msgs) - len(slot_w)
                val = f"{slot_w[0].word2:#x}" if slot_w else "-"
                print(f"{state} {kind} {ev} {args.slot} -> {val} +{other}")
    return 0


# VERIFIED: docs/research/pno-module-chain.md s3: state machine step that installs the routine of
# slot `[this+0x3c]` (state `[this+0xB0]`, kind `[this+0x8C]`, event `[this+0x90]`).
SLOT_FSM = 0xC022F060
# VERIFIED: channel table 0xC05475E8 rows 0..7: slot k word 0 at DM(0xC01D6 + 4 k).
SLOT_TABLE_BASE = 0x28300758


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    d = sub.add_parser("directory", help="module directory of one channel table")
    d.add_argument("csv", type=Path)
    d.add_argument("--listing", action="append", default=[], metavar="T=FILE")
    d.add_argument("--table", default="0xc0545148")
    d.add_argument("--check", action="store_true", help="count rows that are instruction starts")
    d.set_defaults(fn=cmd_directory)
    sh = sub.add_parser("shared", help="routines of one target that exist unchanged elsewhere")
    sh.add_argument("csv", type=Path)
    sh.add_argument("--target", type=int, default=2)
    sh.add_argument("--listing", action="append", default=[], metavar="T=FILE")
    sh.set_defaults(fn=cmd_shared)
    i = sub.add_parser("ids", help="shape of the routines named by an id table in the ARM image")
    i.add_argument("image", type=Path)
    i.add_argument("offset")
    i.add_argument("count", type=int)
    i.add_argument("--listing", action="append", default=[], metavar="T=FILE")
    i.set_defaults(fn=cmd_ids)
    s = sub.add_parser("slots", help="emulate the slot installer (needs unicorn)")
    s.add_argument("image", type=Path)
    s.add_argument("sram", type=Path)
    s.add_argument("--slot", type=int, default=0)
    s.set_defaults(fn=cmd_slots)
    args = p.parse_args(argv)
    return args.fn(args)


if __name__ == "__main__":
    sys.exit(main())
