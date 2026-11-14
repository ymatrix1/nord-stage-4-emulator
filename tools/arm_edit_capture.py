#!/usr/bin/env python3
"""Drive edit ids through the real layer clients of the booted firmware and capture DSP messages.

`arm_sweep.py` called the store setter `0xC00D051C` on an isolated layer, which only accepts the
11 ids its flag array marks (drawbars and layer levels). In the steady state
(`arm_steady.py`) every layer client of the dispatcher exists, and each client routes any id of
its layer through the dispatch routine `0xC00CC4F0(store, 0, id, &value)`. The handlers re-read
the value from the layer's value table (`value_cell`), so the tool first writes the value into
that cell, then calls the dispatch for every id and value of
`docs/research/arm-edit-params.csv`, then runs the main loop
with the timer for a short window (deferred senders such as the drawbar record update run from
the tick), and logs every message the fake DSP ports see.

    uv run --with unicorn python tools/arm_edit_capture.py target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --cache target/settled.snap --csv out.csv

Evidence and limits: `docs/research/arm-organ-messages.md` section 10.
"""

from __future__ import annotations

import argparse
import csv
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

import arm_boot as ab
import arm_boot_full as bf
import arm_emu as ae
import arm_steady as ast

# VERIFIED (run): client dispatch `(store, 0, id, &value)`: the drawbar chain of
# `docs/research/arm-organ-messages.md` section 6 enters here (`0xC00CC4F0 -> 0xC011AB8C`), and
# for ids that the store setter filters out (vibrato, percussion, model) it reaches the handlers
# directly. The handlers do NOT use the pointer: they call the store getter `0xC00D04D0`, which
# reads the value from the layer's value table (see `value_cell`).
DISPATCH = 0xC00CC4F0
# VERIFIED (disassembly of the parent getter `0xC00CE594` + run): the store's parent object
# (`[store + 0x14]`) holds the value table pointer at `parent - 0x9C`; the value of edit id `i`,
# index `k` is `[[[[P] + 4 i]] + 4 k]` (null for ids the layer does not own). 64 to 85 of the
# registry defaults per layer read back through this path, 3 to 4 do not (`Fnc_*`, key position).
PARENT_OFFSET = 0x14
TABLE_OFFSET = -0x9C
# VERIFIED (run, `docs/research/arm-organ-messages.md` section 10): a layer client is registered
# in the dispatcher at `layer + 0x8788`; its store pointers sit in `layer + 0x56C0 .. 0x56E8`
# (two per layer) and their vtable slot 8 is the store setter.
CLIENT_OFFSET = 0x8788
STORE_SCAN = range(0x56C0, 0x56E8, 4)
# Two store classes exist: the organ / piano / synth one (`0xC00D051C`) and the one of layers 4..6
# (`0xC011AED0`, vtable `0xC02BC9FC`).
STORE_SETTERS = (0xC00D051C, 0xC011AED0)
# VERIFIED (run): the layer vtable (`[layer]`) set by the layer constructor identifies the kind.
# organ / piano are the constructors `0xC0019BAC` / `0xC00194B8`. EMPIRICAL (value cells owned by
# the layers, `arm-organ-messages.md` section 10): the `0xC02B7C4C` layers own 65 `Edt_Syn*` ids
# (synth); the `0xC027A80C` layers own `Edt_Ext*` (15) and `Edt_Sys*` (17) plus 11 `Edt_Syn*`
# ids (named "ext"; role ASSUMED, not decoded).
LAYER_KINDS = {0xC027AD8C: "organ", 0xC027ACA0: "piano", 0xC02B7C4C: "synth", 0xC027A80C: "ext"}
SCRATCH = 0x98000000
DEFAULT_FILTER = r"Edt_Org|Edt_Pno|Leslie|Rotary|Edt_Effects"
WINDOW = 4_000_000  # ARM instructions run after each value (20 timer ticks)


@dataclass(frozen=True)
class Store:
    """One edit store of a layer client."""

    layer: int  # index in the dispatcher's client list
    kind: str
    address: int


@dataclass(frozen=True)
class Param:
    edit_id: int
    name: str
    minimum: int
    maximum: int


@dataclass
class CaptureRow:
    param: Param
    value: int
    store: Store
    stage: str  # "call" (sent during the dispatch) or "tick" (sent by the following window)
    message: ae.Message
    error: str = ""
    extra: dict[str, int] = field(default_factory=dict)


def find_stores(emu: ae.ArmEmu) -> list[Store]:
    """Every store object of every layer client registered in the dispatcher."""
    g = emu.read32(ab.DISPATCHER_GLOBAL)
    out = []
    for i in range(emu.read32(g + 0x14C)):
        layer = emu.read32(g + 0x150 + 4 * i) - CLIENT_OFFSET
        kind = LAYER_KINDS.get(emu.read32(layer), "other")
        for off in STORE_SCAN:
            ptr = emu.read32(layer + off)
            try:
                if emu.read32(emu.read32(ptr) + 0x20) in STORE_SETTERS:
                    out.append(Store(i, kind, ptr))
            except Exception:  # not an object: unmapped or a plain integer
                continue
    return out


def first_store_per_layer(stores: list[Store]) -> list[Store]:
    """The two stores of a layer share one client dispatch; one of them is enough."""
    seen: set[int] = set()
    out = []
    for s in stores:
        if s.layer not in seen:
            seen.add(s.layer)
            out.append(s)
    return out


def load_params(csv_path: Path, pattern: str = DEFAULT_FILTER) -> list[Param]:
    rx = re.compile(pattern)
    with csv_path.open(encoding="utf-8") as fh:
        return [
            Param(int(r["id"], 0), r["name"], int(r["min"]), int(r["max"]))
            for r in csv.DictReader(fh)
            if rx.search(r["name"])
        ]


def value_set(lo: int, hi: int, full: int = 12, dense: bool = False) -> list[int]:
    """Every value of a small range (or of any range when `dense`), else minimum, quartiles and
    maximum."""
    if hi - lo < full or (dense and hi - lo < 4096):
        return list(range(lo, hi + 1))
    return sorted({lo, lo + (hi - lo) // 4, (lo + hi) // 2, hi - (hi - lo) // 4, hi})


# VERIFIED (run, `docs/research/synth-stage-models-10.md` s1; the FX ids `fx_clients.cell_word`):
# the value cells of the continuous ids are Q16.16 numbers (`knob << 16`); the original capture
# wrote plain integers, so every integer-knob id saw position 0 and looked value independent.
Q16_SHIFT = 16
Q16_MIN_SPAN = 12
Q16_MODES = ("raw", "auto", "q16")


def cell_word(p: Param, value: int, mode: str = "auto") -> int:
    """32-bit cell word of the UI value `value`: Q16.16 (`value << 16`) when `mode` is `q16`, or
    `auto` and the range spans at least `Q16_MIN_SPAN` steps; the plain integer otherwise."""
    q16 = mode == "q16" or (mode == "auto" and p.maximum - p.minimum >= Q16_MIN_SPAN)
    return (value << Q16_SHIFT if q16 else value) & 0xFFFFFFFF


def value_cell(emu: ae.ArmEmu, store: int, edit_id: int, index: int = 0) -> int | None:
    """Address of the value cell of `edit_id` in the layer owning `store`, or None."""
    try:
        parent = emu.read32(store + PARENT_OFFSET)
        table = emu.read32(emu.read32(parent + TABLE_OFFSET))
        array = emu.read32(table + 4 * edit_id)
        cells = emu.read32(array) if array else 0
    except Exception:  # unmapped: not a store of this layout
        return None
    return cells + 4 * index if cells else None


def baseline_keys(emu: ae.ArmEmu, windows: int = 12, window: int = WINDOW) -> set[tuple]:
    """Keys `(chip, core, path, dm_word)` that the idle firmware sends on its own."""
    emu.clear_messages()
    keys: set[tuple] = set()
    for _ in range(windows):
        ast.run_ticking(emu, window)
        keys |= {(m.chip, m.core, m.path, m.dm_word) for m in emu.messages}
        emu.clear_messages()
    return keys


def apply_value(
    emu: ae.ArmEmu, store: int, edit_id: int, value: int, window: int = WINDOW
) -> tuple[list[ae.Message], list[ae.Message], str]:
    """Dispatch one value; returns `(messages during the call, messages in the window, error)`."""
    cell = value_cell(emu, store, edit_id)
    if cell is not None:
        emu.write32(cell, value)
    emu.write32(SCRATCH, value)
    emu.clear_messages()
    error = ""
    try:
        ast.call_preserving(emu, DISPATCH, store, 0, edit_id, SCRATCH)
    except Exception as exc:  # unicorn raises UcError for faults
        error = str(exc)
    during = emu.messages
    emu.clear_messages()
    _, err = ast.run_ticking(emu, window)
    return during, emu.messages, error or err


def sweep(
    emu: ae.ArmEmu,
    snapshot: bytes,
    stores: list[Store],
    params: list[Param],
    window: int = WINDOW,
    noise: set[tuple] | None = None,
    mode: str = "raw",
    dense: bool = False,
) -> list[CaptureRow]:
    """Dispatch every value of every param to every store that owns it, each from the settled
    state (restore after a value that spoke). Messages whose key is in `noise` are dropped. `mode`
    is the cell encoding of `cell_word` (the committed legacy captures are `raw`)."""
    if not any(b <= SCRATCH <= e for b, e, _ in emu.uc.mem_regions()):
        emu.uc.mem_map(SCRATCH, ae.PAGE)
    rows: list[CaptureRow] = []
    for p in params:
        for store in stores:
            cell = value_cell(emu, store.address, p.edit_id)
            if cell is None:
                continue
            original = emu.read32(cell)
            for v in value_set(p.minimum, p.maximum, dense=dense):
                during, later, err = apply_value(
                    emu, store.address, p.edit_id, cell_word(p, v, mode), window
                )
                during = [m for m in during if _key(m) not in (noise or ())]
                later = [m for m in later if _key(m) not in (noise or ())]
                rows += [CaptureRow(p, v, store, "call", m, err) for m in during]
                rows += [CaptureRow(p, v, store, "tick", m, err) for m in later]
                if not (during or later) and err:
                    rows.append(CaptureRow(p, v, store, "error", ae.Message(0, -1, 0, 0), err))
                if during or later or err:
                    # every value starts from the settled state: a value sent twice or after
                    # another one would only show the difference to the previous one
                    ast.restore(emu, snapshot)
                    emu.install_dsp_ports()
                else:
                    emu.write32(cell, original)
    return rows


def _key(m: ae.Message) -> tuple:
    return (m.chip, m.core, m.path, m.dm_word)


def rows_csv(rows: list[CaptureRow]) -> str:
    """One CSV line per message: addresses and values only."""
    out = ["id,name,value,layer,kind,stage,chip,comm,core,path,dm_word,w2,pc,error"]
    for r in rows:
        m = r.message
        out.append(
            f"{r.param.edit_id:#x},{r.param.name},{r.value},{r.store.layer},{r.store.kind},"
            f"{r.stage},{m.chip},{m.comm},{m.core},{m.path},{m.dm_word:#x},{m.word2:#010x},"
            f"{m.pc:#x},{r.error}"
        )
    return "\n".join(out) + "\n"


def summary_csv(rows: list[CaptureRow]) -> str:
    """One line per (id, layer kind, value): message counts, destinations and DM word range."""
    groups: dict[tuple, list[CaptureRow]] = {}
    for r in rows:
        groups.setdefault((r.param.edit_id, r.param.name, r.store.kind, r.value), []).append(r)
    out = ["id,name,kind,value,calls,ticks,destinations,dm_min,dm_max,distinct_dm,nonzero_values"]
    order = sorted(groups, key=lambda k: (k[2], k[0], k[3]))
    for eid, name, kind, value in order:
        g = groups[(eid, name, kind, value)]
        msgs = [r.message for r in g if r.stage != "error"]
        if not msgs:
            continue
        dest = sorted({f"{m.chip}/{m.core}/{m.path}" for m in msgs})
        dms = sorted({m.dm_word for m in msgs})
        nonzero = sum(1 for m in msgs if m.word2)
        out.append(
            f"{eid:#x},{name},{kind},{value},{sum(r.stage == 'call' for r in g)},"
            f"{sum(r.stage == 'tick' for r in g)},{' '.join(dest)},{dms[0]:#x},{dms[-1]:#x},"
            f"{len(dms)},{nonzero}"
        )
    return "\n".join(out) + "\n"


def bank_script(
    rows: list[CaptureRow], name: str, value: int, call: int, layer: int | None = None
) -> str:
    """`sharc-bank --script` lines `CALL ADDR VALUE` for the PNO core 1 words of one capture."""
    lines = []
    for r in rows:
        m = r.message
        if r.param.name != name or r.value != value:
            continue
        if layer is not None and r.store.layer != layer:
            continue
        if m.chip == "PNO" and m.core == 1 and m.path == 2:
            lines.append(f"{call} {m.dm_word:#x} {m.word2:#010x}  # {name}={value}")
    return "\n".join(lines) + ("\n" if lines else "")


def settled_emulator(code: Path, sram: Path | None, cache: Path | None) -> tuple[ae.ArmEmu, bytes]:
    """Emulator in the settled steady state, from `cache` when it exists."""
    emu = bf.prepare(code, sram)
    if cache is not None and cache.exists():
        ast.restore(emu, cache.read_bytes())
        emu.install_dsp_ports()
        ast.enable_real_timer(emu)
    else:
        pc, _n, err = ast.boot_to_main_loop(emu)
        if pc != ast.MAIN_LOOP:
            raise SystemExit(f"boot stopped at {pc:#x} {err}")
        emu.install_dsp_ports()
        ast.enable_real_timer(emu)
        ast.settle(emu)
    blob = ast.snapshot(emu)
    if cache is not None and not cache.exists():
        cache.write_bytes(blob)
    return emu, blob


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path, nargs="?")
    ap.add_argument("--cache", type=Path, help="settled snapshot to load or create")
    ap.add_argument("--params", type=Path, default=Path("docs/research/arm-edit-params.csv"))
    ap.add_argument("--filter", default=DEFAULT_FILTER)
    ap.add_argument("--window", type=int, default=WINDOW)
    ap.add_argument("--layers", help="comma separated client indices to sweep (default: all)")
    ap.add_argument("--cell", choices=Q16_MODES, default="auto", help="value cell encoding")
    ap.add_argument("--dense", action="store_true", help="every value of ranges below 4096")
    ap.add_argument("--csv", type=Path)
    ap.add_argument("--summary", type=Path, help="per (id, kind, value) summary table")
    ns = ap.parse_args(argv)
    emu, blob = settled_emulator(ns.image, ns.sram, ns.cache)
    stores = first_store_per_layer(find_stores(emu))
    if ns.layers:
        wanted = {int(x) for x in ns.layers.split(",")}
        stores = [s for s in stores if s.layer in wanted]
    params = load_params(ns.params, ns.filter)
    print(f"{len(stores)} stores, {len(params)} ids", flush=True)
    noise = baseline_keys(emu)
    ast.restore(emu, blob)
    emu.install_dsp_ports()
    rows = sweep(emu, blob, stores, params, ns.window, noise, ns.cell, ns.dense)
    if ns.csv:
        ns.csv.write_text(rows_csv(rows), encoding="utf-8")
    if ns.summary:
        ns.summary.write_text(summary_csv(rows), encoding="utf-8")
    print(f"{len(rows)} messages from {len({r.param.edit_id for r in rows})} ids")
    return 0


if __name__ == "__main__":
    sys.exit(main())
