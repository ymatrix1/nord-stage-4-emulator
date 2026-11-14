#!/usr/bin/env python3
"""Who owns each PNO core 1 slot, from a fresh boot, and which chips an organ model talks to.

`docs/research/organ-model-paths.md` swept the organ models from the *settled* state and saw an
idle generator for B3, Farfisa and B3 Bass. The slot allocator (`0xC022F060`, `0xC022C524`)
deduplicates shared generators across chain objects, so a second install of an already present
generator writes only idle routines into the layer's private generator slots. This tool records
every slot write together with the chain object that made it:

    uv run --with unicorn python tools/arm_slot_owners.py boot  CODE SRAM [--csv out.csv]
    uv run --with unicorn python tools/arm_slot_owners.py fresh CODE SRAM [--models 0,1,2,3,5]
    uv run --with unicorn python tools/arm_slot_owners.py chips CODE SRAM --cache settled.snap

* `boot`   slot writes of the boot and the first settling windows (default model B3 on both
           organ layers): `(chain, state, event, kind, chan, slot, routine)`.
* `fresh`  starts from the state right after the boot (before any chain step ran), applies
           `Edt_Org_kModel = m` on organ layer A and records the slot writes of the next
           140 M instructions.
* `chips`  from the settled snapshot: model install, key-down and update of the bank engine;
           counts the DSP messages per `(chip, core, path)` to show which chips are addressed.

Evidence and interpretation: `docs/research/b3-tone-source.md`.
"""

from __future__ import annotations

import argparse
import csv
import io
import sys
from collections import Counter
from dataclasses import dataclass
from pathlib import Path

IDLE_ROUTINE = 0x121B33  # VERIFIED (organ-model-paths.md): `r0 = 0; rts`, the empty slot
SLOT_TABLE_DM = 0xC01D6  # VERIFIED (pno-module-chain.md s3): `DM(0xC01D6 + 4 slot)` routine
SLOT_COUNT = 8
GENERATOR_SLOTS = range(0, 4)  # ASSUMED from the pass: slots 0..3 pair with 4..7 on one stream
PROCESSOR_SLOTS = range(4, 8)


@dataclass(frozen=True)
class SlotWrite:
    """One call of the slot setter `0xC022C524(slot, routine)` and the chain step that made it."""

    chain: int
    state: int
    event: int
    kind: int
    chan: int
    slot: int
    routine: int

    @property
    def idle(self) -> bool:
        return self.routine == IDLE_ROUTINE


def final_table(writes: list[SlotWrite]) -> dict[int, int]:
    """Routine per slot after all writes (the last write wins)."""
    table: dict[int, int] = {}
    for w in writes:
        table[w.slot] = w.routine
    return table


def owners(writes: list[SlotWrite]) -> dict[int, int]:
    """Chain object that wrote the last *non-idle* routine of each slot."""
    own: dict[int, int] = {}
    for w in writes:
        if not w.idle:
            own[w.slot] = w.chain
    return own


def shared_generators(table: dict[int, int]) -> dict[int, int]:
    """Non-idle generator slots (0..3) of a table: these serve the whole pass, not one layer."""
    return {s: r for s, r in table.items() if s in GENERATOR_SLOTS and r != IDLE_ROUTINE}


def pairs(table: dict[int, int]) -> list[tuple[int, int, int, int]]:
    """`(generator slot, generator, processor slot, processor)` on the same stream (slot + 4)."""
    out = []
    for g in GENERATOR_SLOTS:
        p = g + 4
        if g in table and p in table:
            out.append((g, table[g], p, table[p]))
    return out


def census(rows: list[tuple[str, int, int]]) -> Counter[tuple[str, int, int]]:
    """Count `(chip, core, path)` of DSP messages."""
    return Counter(rows)


def writes_csv(runs: dict[str, list[SlotWrite]]) -> str:
    buf = io.StringIO()
    w = csv.writer(buf, lineterminator="\n")
    w.writerow(["run", "chain", "state", "event", "kind", "chan", "slot", "routine"])
    for name, writes in runs.items():
        for s in writes:
            w.writerow(
                [name, f"{s.chain:#x}", s.state, s.event, s.kind, s.chan, s.slot, f"{s.routine:#x}"]
            )
    return buf.getvalue()


def table_text(table: dict[int, int]) -> str:
    return " ".join(f"{s}:{table[s]:#x}" for s in sorted(table))


# ---------------------------------------------------------------------------------------------
# Emulator side (needs `unicorn`; exercised by the research run, not by the unit tests).
# ---------------------------------------------------------------------------------------------


class WriteRecorder:  # pragma: no cover - needs the firmware emulator
    """Code hooks on the chain step and the slot setter, tagging each write with its chain."""

    def __init__(self, emu) -> None:  # noqa: ANN001
        import arm_model_paths as mp

        self.emu, self.mp = emu, mp
        self.cur: tuple[int, int, int, int, int] = (0, -1, -1, -1, -1)
        self.writes: list[SlotWrite] = []
        self.handles: list[int] = []

    def __enter__(self) -> WriteRecorder:
        u, mp, emu = self.emu._u, self.mp, self.emu

        def on_step(_uc, _a, _s, _u) -> None:  # noqa: ANN001
            ch = emu.reg(0)
            self.cur = (
                ch,
                emu.read32(ch + mp.FIELD_STATE),
                emu.read32(ch + mp.FIELD_EVENT),
                emu.read32(ch + mp.FIELD_KIND),
                emu.read32(ch + mp.FIELD_CHANNEL),
            )

        def on_slot(_uc, _a, _s, _u) -> None:  # noqa: ANN001
            self.writes.append(SlotWrite(*self.cur, emu.reg(0), emu.reg(1)))

        uc = emu.uc
        self.handles = [
            uc.hook_add(u.UC_HOOK_CODE, on_step, begin=mp.CHAIN_STEP, end=mp.CHAIN_STEP),
            uc.hook_add(u.UC_HOOK_CODE, on_slot, begin=mp.SLOT_SETTER, end=mp.SLOT_SETTER),
        ]
        return self

    def __exit__(self, *_exc: object) -> None:
        for h in self.handles:
            self.emu.uc.hook_del(h)


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    import arm_boot_full as bf
    import arm_edit_capture as aec
    import arm_model_paths as mp
    import arm_organ_weights as ow
    import arm_steady as ast

    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("mode", choices=["boot", "fresh", "chips"])
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path)
    ap.add_argument("--models", default="0,1,2,3,5")
    ap.add_argument("--windows", type=int, default=7, help="20 M instruction windows per run")
    ap.add_argument(
        "--layers",
        default="0",
        help="organ layers that get the model in `fresh` mode (0, 1 or 0,1)",
    )
    ap.add_argument("--cache", type=Path)
    ap.add_argument("--csv", type=Path)
    ns = ap.parse_args(argv)

    if ns.mode == "chips":
        emu, blob = aec.settled_emulator(ns.image, ns.sram, ns.cache)
        store = next(
            s for s in aec.first_store_per_layer(aec.find_stores(emu)) if s.kind == "organ"
        )
        drv = ow.BankDriver(emu, blob)
        print("model, phase, chip core path: count")
        for model in (int(m) for m in ns.models.split(",")):
            ast.restore(emu, blob)
            emu.install_dsp_ports()
            ast.enable_real_timer(emu)
            mp.install_model(emu, store, model)
            phases = {"install": list(emu.messages)}
            drv.set_drawbars(0, (8, 8, 8, 0, 0, 0, 0, 0, 0))
            phases["key-down"] = list(drv.note_on(0, 60, 100))
            phases["update"] = list(drv.update(0))
            for phase, msgs in phases.items():
                c = census([(m.chip, m.core, m.path) for m in msgs])
                print(model, phase, dict(sorted(c.items())))
        return 0

    emu = bf.prepare(ns.image, ns.sram)
    ast.boot_to_main_loop(emu)
    emu.install_dsp_ports()
    ast.enable_real_timer(emu)
    runs: dict[str, list[SlotWrite]] = {}
    if ns.mode == "boot":
        with WriteRecorder(emu) as rec:
            for _ in range(ns.windows):
                ast.run_ticking(emu, 20_000_000)
        runs["boot"] = rec.writes
    else:
        blob = ast.snapshot(emu)
        store = next(
            s for s in aec.first_store_per_layer(aec.find_stores(emu)) if s.kind == "organ"
        )
        for model in (int(m) for m in ns.models.split(",")):
            ast.restore(emu, blob)
            emu.install_dsp_ports()
            ast.enable_real_timer(emu)
            mp.ensure_scratch(emu)
            with WriteRecorder(emu) as rec:
                emu.write32(mp.SCRATCH, mp.MODEL_ID)
                emu.write32(mp.SCRATCH + 4, 1)
                ast.call_preserving(emu, mp.MODEL_HANDLER, store.address, 0, mp.SCRATCH, model)
                for _ in range(ns.windows):
                    ast.run_ticking(emu, 20_000_000)
            runs[f"model{model}_layers{ns.layers.replace(',', '+')}"] = rec.writes
    for name, writes in runs.items():
        print(name, table_text(final_table(writes)))
        for w in writes:
            print(
                f"  chain {w.chain:#x} chan {w.chan} state {w.state} event {w.event} "
                f"kind {w.kind}: slot {w.slot} <- {w.routine:#x}"
            )
    if ns.csv:
        ns.csv.write_text(writes_csv(runs), encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
