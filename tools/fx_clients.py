#!/usr/bin/env python3
"""FX clients of the booted firmware: the 12 FX unit objects, and edit ids driven through them.

`arm_edit_capture.py` sweeps the 10 layer clients of the dispatcher (organ, piano, synth, ext).
The effects have **no client in that list**. They live in 6 FX *parts* (constructor `0xC01177A8`,
vtable `0xC02BB930`: 1 organ part, 2 piano parts, 3 synth parts), each of which builds **two FX
unit objects** with `0xC01155EC` (vtable `0xC02BAD68`, the unit index `L` = 0..11 at `+0x10`). The
unit object *is* the store (its vtable slot 8 is the store setter `0xC00D051C`), so the dispatch
`0xC00CC4F0(unit, 0, id, &value)` of `arm_edit_capture.apply_value` drives the effect handlers of
the unit. The units exist after `arm_steady.boot_to_main_loop`; they are only missing from the
dispatcher's client list, which is why the id sweep of `arm-organ-messages.md` section 10 found no
value cell for Fx2, Dly, Rvb, Cmp and Leslie.

    uv run --with unicorn python tools/fx_clients.py target/fw_unpacked/bank0_CODE.bin \\
        target/fw_unpacked/bank0_SRAM.bin --cache target/settled.snap --csv fx-capture.csv

Evidence and limits: `docs/research/fx-contexts.md`.
"""

from __future__ import annotations

import argparse
import struct
import sys
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import arm_edit_capture as cap
import arm_emu as ae

# VERIFIED (run, hook on 0xC01155EC and 0xC01177A8 during the boot, `docs/research/fx-contexts.md`
# s2): the unit class vtable (written by the unit constructor) and the part class vtable.
UNIT_VTABLE = 0xC02BAD68
PART_VTABLE = 0xC02BB930
# VERIFIED (run): `unit + 0x10` is the unit index L (0..11); `unit + 0x14` is the owning part.
UNIT_INDEX_OFFSET = 0x10
UNIT_PART_OFFSET = 0x14
# VERIFIED (run): the module objects of a unit (`docs/research/fx-type-map.md` s3) sit at
# `unit + 0x208 .. 0x250` (19 pointers; two of them have no registered row).
MODULE_FIRST, MODULE_LAST = 0x208, 0x254
HEAP_LO, HEAP_HI = 0x90000000, 0x92000000
# EMPIRICAL (value cells, run): a unit owns exactly one of these section-mode ids, which names the
# kind of its part (organ parts have none of them but own `Edt_OrgCom_kEffectsLeslieEnable`).
KIND_MARKERS = {
    "Edt_OrgCom_kEffectsLeslieEnable": "organ",
    "Edt_PnoCom_kEffectsSectionModePno_private": "piano",
    "Edt_SynCom_kEffectsSectionModeSyn_private": "synth",
}
DEFAULT_FILTER = r"Edt_Effects|Leslie|Edt_Com_kEffects|SectionMode"


@dataclass(frozen=True)
class Unit:
    """One FX unit object: index `L`, address, owning part, kind (organ / piano / synth)."""

    index: int
    address: int
    part: int
    kind: str


def scan_word(emu: ae.ArmEmu, word: int, lo: int = HEAP_LO, hi: int = HEAP_HI) -> list[int]:
    """Word-aligned addresses in `[lo, hi)` whose 32-bit value is `word`."""
    pattern = struct.pack("<I", word)
    found: list[int] = []
    for begin, end, _perms in emu.uc.mem_regions():
        if end < lo or begin >= hi:
            continue
        raw = bytes(emu.uc.mem_read(begin, end - begin + 1))
        i = raw.find(pattern)
        while i >= 0:
            if (begin + i) % 4 == 0 and lo <= begin + i < hi:
                found.append(begin + i)
            i = raw.find(pattern, i + 1)
    return found


def unit_kind(emu: ae.ArmEmu, unit: int, markers: dict[str, int]) -> str:
    """Kind of a unit from the section-mode id its value table owns (`markers`: name -> id)."""
    for name, kind in KIND_MARKERS.items():
        if name in markers and cap.value_cell(emu, unit, markers[name]) is not None:
            return kind
    return "other"


def find_units(emu: ae.ArmEmu, markers: dict[str, int] | None = None) -> list[Unit]:
    """All FX unit objects of the emulator, sorted by unit index."""
    units = []
    for address in scan_word(emu, UNIT_VTABLE):
        index = emu.read32(address + UNIT_INDEX_OFFSET)
        if index > 11:  # not a unit object: a stored copy of the vtable pointer
            continue
        kind = unit_kind(emu, address, markers or {})
        units.append(Unit(index, address, emu.read32(address + UNIT_PART_OFFSET), kind))
    return sorted(units, key=lambda u: u.index)


def module_objects(emu: ae.ArmEmu, unit: int) -> list[tuple[int, int, int, int]]:
    """`(offset, object, vtable, value)` of every module object of a unit (value at `+0x40`)."""
    out = []
    for off in range(MODULE_FIRST, MODULE_LAST, 4):
        obj = emu.read32(unit + off)
        if HEAP_LO <= obj < HEAP_HI:
            out.append((off, obj, emu.read32(obj), emu.read32(obj + 0x40)))
    return out


def module_rows(emu: ae.ArmEmu, unit: int) -> dict[int, int]:
    """Registered state-table row (`module + 4`) of every module object, by unit offset."""
    return {off: emu.read32(obj + 4) for off, obj, _vt, _v in module_objects(emu, unit)}


def stores_of(units: list[Unit], indices: set[int] | None = None) -> list[cap.Store]:
    """`arm_edit_capture.Store` records for `units` (layer = unit index, kind = unit kind)."""
    return [
        cap.Store(u.index, u.kind, u.address) for u in units if not indices or u.index in indices
    ]


@dataclass(frozen=True)
class Slot:
    """An effect slot of a unit: the enum id that selects its type and the ids that tune it."""

    name: str
    type_id: str | None
    params: tuple[str, ...]


# EMPIRICAL (run, `docs/research/fx-contexts.md` s4): the effect slots of a unit and the ids that
# reach their handlers; `Cmp` and the Leslie ids have no type.
SLOTS = (
    Slot(
        "Fx1",
        "Edt_Effects_kFx1Type",
        ("Edt_Effects_kFx1Amt", "Edt_Effects_kFx1RateClkDiv", "Edt_Effects_kFx1ClkEnable"),
    ),
    Slot("Fx2", "Edt_Effects_kFx2Type", ("Edt_Effects_kFx2Amt", "Edt_Effects_kFx2Rate")),
    Slot("Rvb", "Edt_Effects_kRvbType", ("Edt_Effects_kRvbAmt", "Edt_Effects_kRvbColor")),
    Slot(
        "Dly",
        "Edt_Effects_kDlyFbFxType",
        (
            "Edt_Effects_kDlyMix",
            "Edt_Effects_kDlyFbkAmt",
            "Edt_Effects_kDlyFbkFilt",
            "Edt_Effects_kDlyFbkPingPong",
            "Edt_Effects_kDlyRateClkDiv",
            "Edt_Effects_kDlyMode",
            "Edt_Effects_kDlyClkEnable",
        ),
    ),
    Slot("Cmp", None, ("Edt_Effects_kCmpAmt", "Edt_Effects_kCmpResponse")),
    Slot(
        "Amp",
        "Edt_Effects_kAmpTypeNew",
        (
            "Edt_Effects_kAmpTreble",
            "Edt_Effects_kAmpMid",
            "Edt_Effects_kAmpBass",
            "Edt_Effects_kAmpMidFreq",
            "Edt_Effects_kAmpDrive",
        ),
    ),
    Slot(
        "Leslie",
        None,
        (
            "Edt_OrgCom_kEffectsLeslieDrv",
            "Edt_OrgCom_kLeslieRotorAngle",
            "Edt_OrgCom_kLeslieHornAngle",
            "Edt_OrgCom_kLeslieMicDistance_revB",
            "Edt_OrgCom_kEffectsLeslieStopMode",
            "Edt_OrgCom_kEffectsLeslieSpeed",
            "Edt_Sys_kLeslieHornSpeed",
            "Edt_Sys_kLeslieRotorSpeed",
            "Edt_Sys_kLeslieBalance",
        ),
    ),
)


def value_grid(lo: int, hi: int) -> list[int]:
    """Values to apply to a param, ascending: a whole small range, else a spread that includes
    both ends, zero, the small values near zero and the default region."""
    if hi - lo < 12:
        return list(range(lo, hi + 1))
    span = hi - lo
    pts = {lo, hi, lo + 1, lo + 2, lo + span // 2}
    pts |= {lo + span * k // 8 for k in range(1, 8)}
    if lo < 0 < hi:
        pts |= {0, 1, -1}
    return sorted(pts)


def apply_series(
    apply: Callable[[int], tuple[list[ae.Message], list[ae.Message], str]], values: list[int]
) -> list[tuple[int, str, ae.Message]]:
    """Apply `values` in order; returns `(value, stage, message)` for every message."""
    out = []
    for v in values:
        during, later, _err = apply(v)
        out += [(v, "call", m) for m in during] + [(v, "tick", m) for m in later]
    return out


def capture_slot(  # pragma: no cover - needs the booted firmware
    emu: ae.ArmEmu,
    blob: bytes,
    unit: Unit,
    slot: Slot,
    params: dict[str, cap.Param],
    window: int = 400_000,
    noise: set[tuple] | None = None,
) -> list[dict[str, object]]:
    """Every message of the type switch (stage `type`) and of each tuning id under each type."""
    import arm_steady as ast

    rows: list[dict[str, object]] = []
    types: list[int | None] = [None]
    if slot.type_id is not None:
        tp = params[slot.type_id]
        types = list(range(tp.minimum, tp.maximum + 1))
    for t in types:
        ast.restore(emu, blob)
        emu.install_dsp_ports()
        if t is not None:
            tp = params[slot.type_id]
            series = apply_series(
                lambda v, tp=tp: cap.apply_value(emu, unit.address, tp.edit_id, v, window), [t]
            )
            rows += _rows(unit, slot, t, slot.type_id, "type", series, noise)
        base = ast.snapshot(emu)
        for name in slot.params:
            p = params.get(name)
            if p is None or cap.value_cell(emu, unit.address, p.edit_id) is None:
                continue
            ast.restore(emu, base)
            emu.install_dsp_ports()
            series = apply_series(
                lambda v, p=p: cap.apply_value(
                    emu, unit.address, p.edit_id, v & 0xFFFFFFFF, window
                ),
                value_grid(p.minimum, min(p.maximum, 0x7FFFFFFF)),
            )
            rows += _rows(unit, slot, t, name, "param", series, noise)
    return rows


def _rows(unit, slot, t, name, stage, series, noise):  # noqa: ANN001, ANN202
    keep = [
        (v, st, m)
        for v, st, m in series
        if (m.chip, m.core, m.path, m.dm_word) not in (noise or ())
    ]
    return [
        {
            "unit": unit.index,
            "kind": unit.kind,
            "slot": slot.name,
            "type": "" if t is None else t,
            "id": name,
            "stage": stage,
            "value": v,
            "when": st,
            "core": m.core,
            "dm_word": f"{m.dm_word:#x}",
            "word": f"{m.word2:#010x}",
            "pc": f"{m.pc:#x}",
        }
        for v, st, m in keep
    ]


CAPTURE_FIELDS = "unit,kind,slot,type,id,stage,value,when,core,dm_word,word,pc".split(",")


def write_capture(path: Path, rows: list[dict[str, object]]) -> None:
    import csv

    with path.open("w", encoding="utf-8", newline="") as fh:
        w = csv.DictWriter(fh, fieldnames=CAPTURE_FIELDS)
        w.writeheader()
        w.writerows(rows)


def fx_snapshot(code: Path, sram: Path | None, cache: Path | None):  # pragma: no cover - firmware
    """Settled emulator and snapshot (same cache as `arm_edit_capture.settled_emulator`)."""
    return cap.settled_emulator(code, sram, cache)


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path, nargs="?")
    ap.add_argument("--cache", type=Path, help="settled snapshot to load or create")
    ap.add_argument("--params", type=Path, default=Path("docs/research/arm-edit-params.csv"))
    ap.add_argument("--filter", default=DEFAULT_FILTER)
    ap.add_argument("--window", type=int, default=2_000_000)
    ap.add_argument("--units", help="comma separated unit indices (default: all 12)")
    ap.add_argument("--csv", type=Path)
    ap.add_argument("--summary", type=Path)
    ap.add_argument("--slots", type=Path, help="per type / per param capture of the effect slots")
    ap.add_argument("--slot", help="comma separated slot names for --slots (default: all)")
    ns = ap.parse_args(argv)
    import arm_steady as ast

    emu, blob = fx_snapshot(ns.image, ns.sram, ns.cache)
    all_params = cap.load_params(ns.params, ".")
    markers = {p.name: p.edit_id for p in all_params if p.name in KIND_MARKERS}
    units = find_units(emu, markers)
    wanted = {int(x, 0) for x in ns.units.split(",")} if ns.units else None
    stores = stores_of(units, wanted)
    if ns.slots:
        by_name = {p.name: p for p in cap.load_params(ns.params, ns.filter + "|Leslie")}
        noise = cap.baseline_keys(emu)
        names = set(ns.slot.split(",")) if ns.slot else {s.name for s in SLOTS}
        out: list[dict[str, object]] = []
        for u in (u for u in units if not wanted or u.index in wanted):
            for slot in (s for s in SLOTS if s.name in names):
                out += capture_slot(emu, blob, u, slot, by_name, ns.window, noise)
                print(f"unit {u.index} {slot.name}: {len(out)} rows", flush=True)
        write_capture(ns.slots, out)
        return 0
    params = cap.load_params(ns.params, ns.filter)
    print(f"{len(units)} units, sweeping {len(stores)}, {len(params)} ids", flush=True)
    noise = cap.baseline_keys(emu)
    ast.restore(emu, blob)
    emu.install_dsp_ports()
    rows = cap.sweep(emu, blob, stores, params, ns.window, noise)
    if ns.csv:
        ns.csv.write_text(cap.rows_csv(rows), encoding="utf-8")
    if ns.summary:
        ns.summary.write_text(cap.summary_csv(rows), encoding="utf-8")
    print(f"{len(rows)} messages from {len({r.param.edit_id for r in rows})} ids")
    return 0


if __name__ == "__main__":
    sys.exit(main())
