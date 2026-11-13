#!/usr/bin/env python3
"""FX module objects of the ARM firmware: the routines each owns, and what a type switch sends.

Every effect module of the ARM is a *property object* (an enum value with a change handler). Its
word at `+0x34` points into a list of 16-byte descriptors `{table, 0, 1, rows}` (`slice_at`).
`table` points at a run of channel records `{target, base, size, tag}` of
`docs/research/fx-chain-install.md`: either FX code records (`base` = system address of a routine
of the FX image) or DSP state-block records. So the descriptor lists are the **static module to
routine map** of the effect library (`inventory`); `modules` prints it for the six module objects of
the Amp layer.

`probe` runs the change handler of one module object of the Amp layer (`0xC0220E50`) for the type
values `0..N-1` in the unicorn harness and groups the DSP messages it sends (`docs/research/
fx-type-map.md` sections 4 and 5).

    uv run --with unicorn --with capstone python tools/fx_type_map.py inventory CODE.bin SRAM.bin \\
        --out docs/research/data/fx-type-map.csv
    uv run --with unicorn --with capstone python tools/fx_type_map.py probe CODE.bin SRAM.bin \\
        --module 0x54 --handler 0xC020506C --types 12 --variant 2
"""

from __future__ import annotations

import argparse
import csv
import struct
import sys
from collections.abc import Callable
from pathlib import Path
from typing import NamedTuple

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import fx_chain_install as fci
import pno_module_chain as pm

#: VERIFIED: docs/research/fx-type-map.md s2 (`inventory`): 26 descriptor lists in this range of the
#: emulated ARM RAM after the static initialisers; the tables they name lie in the next range.
SCAN_LO, SCAN_HI = 0xC0270000, 0xC0540000
TABLE_LO, TABLE_HI = 0xC0540000, 0xC0560000
#: EMPIRICAL: a descriptor is `{table, 0, 1, rows}` with 1..64 rows (the words 1 and 2 are constant
#: in all 26 lists; their meaning is not decoded).
MAX_ROWS = 64
DESCRIPTOR_BYTES = 16
#: VERIFIED (`modules`): offset of the descriptor-slice pointer in a module object.
SLICE_OFFSET = 0x34
#: VERIFIED (`modules`, disassembly `0xC0223708`): the commit of a module object calls vtable slot
#: 24 (byte offset 0x60) when the value changed; slot 7 is `SetValue`, slot 6 the range getter.
CHANGE_SLOT, SET_SLOT, RANGE_SLOT = 24, 7, 6
CSV_FIELDS = "list_addr,entry,table,rows,kind,target,core,pc,size,tag".split(",")


class Descriptor(NamedTuple):
    addr: int
    table: int
    rows: int


class CodeRow(NamedTuple):
    target: int
    core: int
    pc: int
    size: int
    tag: int


def find_descriptor_lists(
    read32: Callable[[int], int], lo: int = SCAN_LO, hi: int = SCAN_HI
) -> list[list[Descriptor]]:
    """Runs of consecutive `{table, 0, 1, rows}` descriptors in `[lo, hi)` (4-byte steps)."""
    lists: list[list[Descriptor]] = []
    addr = lo
    while addr < hi:
        table, w1, w2, rows = (read32(addr + 4 * i) for i in range(4))
        if TABLE_LO <= table < TABLE_HI and w1 == 0 and w2 == 1 and 1 <= rows <= MAX_ROWS:
            d = Descriptor(addr, table, rows)
            if lists and lists[-1][-1].addr + DESCRIPTOR_BYTES == addr:
                lists[-1].append(d)
            else:
                lists.append([d])
            addr += DESCRIPTOR_BYTES
        else:
            addr += 4
    return lists


def code_rows(read32: Callable[[int], int], table: int, rows: int) -> list[CodeRow]:
    """FX code records among the `rows` channel records at `table` (targets 0 and 1 only)."""
    out = []
    for k in range(rows):
        target, base, size, tag = (read32(table + 16 * k + 4 * j) for j in range(4))
        hit = fci.base_to_pc(base)
        if hit is not None and fci.FX_TARGETS.get(target) == hit[0]:
            out.append(CodeRow(target, hit[0], hit[1], size, tag))
    return out


def slice_at(read32: Callable[[int], int], addr: int) -> list[Descriptor]:
    """Consecutive descriptors starting at `addr` (a module object's `+0x34` slice pointer)."""
    out = []
    while True:
        table, w1, w2, rows = (read32(addr + 4 * i) for i in range(4))
        if not (TABLE_LO <= table < TABLE_HI and w1 == 0 and w2 == 1 and 1 <= rows <= MAX_ROWS):
            return out
        out.append(Descriptor(addr, table, rows))
        addr += DESCRIPTOR_BYTES


def inventory_rows(
    read32: Callable[[int], int], lo: int = SCAN_LO, hi: int = SCAN_HI
) -> list[dict]:
    """CSV rows: one per code record (kind `code`) or per state-block descriptor (kind `state`)."""
    rows: list[dict] = []
    for dlist in find_descriptor_lists(read32, lo, hi):
        for n, d in enumerate(dlist):
            head = {
                "list_addr": f"{dlist[0].addr:#x}",
                "entry": n,
                "table": f"{d.table:#x}",
                "rows": d.rows,
            }
            code = code_rows(read32, d.table, d.rows)
            if not code:
                rows.append({**head, "kind": "state"})
                continue
            for c in code:
                rows.append(
                    {
                        **head,
                        "kind": "code",
                        "target": c.target,
                        "core": c.core,
                        "pc": f"{c.pc:#08x}",
                        "size": f"{c.size:#x}",
                        "tag": f"{c.tag:#x}",
                    }
                )
    return rows


def class_pcs(rows: list[dict]) -> dict[str, dict[int, list[int]]]:
    """`{list_addr: {core: [pc, ...]}}` of the code records of an inventory (sorted, unique)."""
    out: dict[str, dict[int, set[int]]] = {}
    for r in rows:
        if r["kind"] != "code":
            continue
        out.setdefault(r["list_addr"], {}).setdefault(int(r["core"]), set()).add(int(r["pc"], 16))
    return {k: {c: sorted(p) for c, p in v.items()} for k, v in out.items()}


def f32(word: int) -> float:
    return struct.unpack("<f", struct.pack("<I", word & 0xFFFFFFFF))[0]


def summarize_messages(messages: list[tuple[int, int, int]]) -> list[str]:
    """Group `(system address, value, store pc)` messages: zero runs collapse, others print.

    A run of zero values (a state clear) becomes `zeros xN first..last`; a non-zero value becomes
    `address = word (float)`. Used by `probe` and the tests.
    """
    out: list[str] = []
    run: list[int] = []

    def flush() -> None:
        if run:
            out.append(f"zeros x{len(run)} {run[0]:#x}..{run[-1]:#x}")
            run.clear()

    for addr, value, pc in messages:
        if value == 0:
            run.append(addr)
            continue
        flush()
        out.append(f"{addr:#x} = {value:#010x} ({f32(value):.6g}) from {pc:#x}")
    flush()
    return out


def cmd_inventory(args: argparse.Namespace) -> int:  # pragma: no cover - needs unicorn + firmware
    import arm_boot as ab

    emu = ab._fresh(args.code, args.sram)
    rows = inventory_rows(emu.read32)
    with args.out.open("w", newline="") as fh:
        w = csv.DictWriter(fh, fieldnames=CSV_FIELDS, restval="")
        w.writeheader()
        w.writerows(rows)
    lists = {r["list_addr"] for r in rows}
    print(f"{len(lists)} descriptor lists, {sum(r['kind'] == 'code' for r in rows)} code records")
    if args.listing:
        listings = {}
        for spec in args.listing:
            t, _, path = spec.partition("=")
            listings[int(t)] = pm.load_listing(Path(path))
        for list_addr, per_core in class_pcs(rows).items():
            for core, pcs in sorted(per_core.items()):
                ins = listings.get(core - 1)
                shapes = [pm.sr.signature(pm.routine_at(ins, pc)) if ins else {} for pc in pcs]
                print(
                    list_addr,
                    f"core{core}",
                    [f"{p:#x}:{s.get('n', '?')}i" for p, s in zip(pcs, shapes, strict=True)],
                )
    return 0


#: VERIFIED (`modules`): the six module objects of the Amp layer sit at these `this +` offsets.
LAYER_MODULES = (0x50, 0x54, 0x58, 0x5C, 0x60, 0x64)


def build_amp_layer(ab, code: Path, sram: Path):  # noqa: ANN001, ANN201 - harness objects
    """Emulator with the edit registry and the Amp layer object built; returns `(emu, layer)`."""
    emu = ab._fresh(code, sram)
    ctx = ab.build_edit_registry(emu)
    emu.write32(ab.REGISTRY_GLOBAL + 0x1C, ctx)
    layer = 0x60000000
    emu.uc.mem_map(layer, 0x100000)
    emu.write32(ab.DISPATCHER_GLOBAL, layer + 0x100000 - 0x1000)
    emu.call(fci.LAYER_CTOR, layer, 0, 0, 0, max_insns=50_000_000)
    return emu, layer


def cmd_modules(args: argparse.Namespace) -> int:  # pragma: no cover - needs unicorn + firmware
    import arm_boot as ab

    emu, layer = build_amp_layer(ab, args.code, args.sram)
    for off in LAYER_MODULES:
        obj = emu.read32(layer + off)
        vt = emu.read32(obj)
        rng = emu.call(emu.read32(vt + 4 * RANGE_SLOT), obj, max_insns=200_000)
        getter = emu.read32(vt + 4 * RANGE_SLOT)
        handler = emu.read32(vt + 4 * CHANGE_SLOT)
        print(f"layer+{off:#x} obj {obj:#x} vtable {vt:#x} range-getter {getter:#x} -> {rng}")
        print(f"    change handler {handler:#x}")
        for d in slice_at(emu.read32, emu.read32(obj + SLICE_OFFSET)):
            code = code_rows(emu.read32, d.table, d.rows)
            pcs = " ".join(f"c{c.core}:{c.pc:#x}" for c in code) or "(state blocks)"
            print(f"    {d.table:#x} x{d.rows}  {pcs}")
    return 0


def cmd_probe(args: argparse.Namespace) -> int:  # pragma: no cover - needs unicorn + firmware
    import arm_boot as ab

    emu, layer = build_amp_layer(ab, args.code, args.sram)
    if args.variant:
        emu.add_stub(args.variant_fn, lambda _e: args.variant)
    obj = emu.read32(layer + args.module)
    print(f"module object {obj:#x} (layer + {args.module:#x}), handler {args.handler:#x}")
    for t in range(args.types):
        emu.clear_messages()
        emu.call(args.handler, obj, t, 0, 0, max_insns=5_000_000)
        msgs = [(m.word1, m.word2, m.pc) for m in emu.messages]
        lines = summarize_messages(msgs)
        print(f"type {t}: {len(msgs)} messages, {len(lines)} groups")
        for line in lines[: args.lines]:
            print("   ", line)
    return 0


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    inv = sub.add_parser("inventory", help="class -> routine map from the descriptor lists")
    inv.add_argument("code", type=Path)
    inv.add_argument("sram", type=Path)
    inv.add_argument("--out", type=Path, required=True)
    inv.add_argument("--listing", action="append", default=[], metavar="T=FILE")
    inv.set_defaults(fn=cmd_inventory)
    mo = sub.add_parser("modules", help="module objects of the Amp layer and their routines")
    mo.add_argument("code", type=Path)
    mo.add_argument("sram", type=Path)
    mo.set_defaults(fn=cmd_modules)
    pr = sub.add_parser("probe", help="run a module change handler for types 0..N-1")
    pr.add_argument("code", type=Path)
    pr.add_argument("sram", type=Path)
    pr.add_argument("--module", type=lambda s: int(s, 0), default=0x54)
    pr.add_argument("--handler", type=lambda s: int(s, 0), default=0xC020506C)
    pr.add_argument("--types", type=int, default=12)
    pr.add_argument("--variant", type=int, default=0, help="stub the hardware variant (1 or 2)")
    pr.add_argument("--variant-fn", type=lambda s: int(s, 0), default=0xC0228CB4)
    pr.add_argument("--lines", type=int, default=12)
    pr.set_defaults(fn=cmd_probe)
    args = p.parse_args(argv)
    return args.fn(args)


if __name__ == "__main__":
    sys.exit(main())
