#!/usr/bin/env python3
"""Which PNO core 1 routines does each organ model install? (Edt_Org_kModel -> slot table)

The organ layer handler of edit id `0x166` (`Edt_Org_kModel`, `0xC011C9BC`) stores the model value
as the *event* of its chain object (`[store + 0x98]`, field `+0x90`), clears the state (`+0xB0`)
and queues the chain step `0xC022F060`. The step runs three times (state 0 tear down, 1 generator
slot, 2 processor slot, then state 3); its slot writes go through `0xC022C524(slot, routine)` to
`DM(0xC01D6 + 4 slot)` of PNO core 1. This tool applies each model to the booted firmware
(`arm_steady.py` snapshot), steps the chain and records the slot writes, the slot buffer pointer
(`DM(0xC01D6 + 4 slot + 2)`) and the other PNO core 1 words the install burst sends.

    uv run --with unicorn python tools/arm_model_paths.py target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --cache target/settled.snap \
        --csv docs/research/data/organ-model-paths.csv

Evidence, tables and limits: `docs/research/organ-model-paths.md`.
"""

from __future__ import annotations

import argparse
import csv
import io
import sys
from dataclasses import dataclass, field
from pathlib import Path

import arm_edit_capture as aec
import arm_emu as ae
import arm_steady as ast

# VERIFIED (disassembly 0xC011C9BC..0xC011CA70 + run): the organ layer client handler. For
# `[r2] == 0x166` it stores `r3` (the model) at `[client + 0xC0]` and, through `r4 = [client +
# 0x98]` (the chain object), `[r4 + 0x90] = r3` (event), byte `[r4 + 0xB4] = 1`, `[r4 + 0xB0] = 0`
# (state), then schedules the object (`0xC009CFF8`, +0x120 ticks).
MODEL_HANDLER = 0xC011C9BC
MODEL_ID = 0x166
CHAIN_FIELD = 0x98
# VERIFIED (disassembly + run): the chain step `0xC022F060(chain)` and its fields.
CHAIN_STEP = 0xC022F060
FIELD_STATE, FIELD_KIND, FIELD_EVENT, FIELD_CHANNEL = 0xB0, 0x8C, 0x90, 0x34
STATE_DONE = 3
# VERIFIED (run, `docs/research/pno-module-chain.md` section 3): `0xC022C524(slot, routine)` writes
# `DM(0xC01D6 + 4 slot)` of PNO core 1; `0xC022AB50` writes the slot buffer pointer at `+2`.
SLOT_SETTER = 0xC022C524
SLOT_TABLE_DM = 0xC01D6
SLOT_STRIDE = 4
IDLE_ROUTINE = 0x121B33  # `r0 = 0; rts`, the empty slot
# VERIFIED (strings in `bank0_CODE.bin`, same order as the edit value 0..5 by the VibratoType ids
# `0x168..0x16A` = B3, Vox, Farfisa): "B3, Vox, Farfisa, Pipe1, Pipe2, B3 Bass, <empty>".
NAMES_ADDRESS = 0xC02C75F4
NAME_COUNT = 6
# Names of the PNO core 1 routines (ASSUMED roles, `docs/research/pno-module-chain.md` section 4).
ROUTINES = {
    IDLE_ROUTINE: "idle",
    0x121E2B: "resonator A (event 0 processor)",
    0x1229B3: "output stage (84 osc + 12 bucket weights)",
    0x122B85: "FIR/IIR loop 0.9977 (event 2 processor)",
    0x122620: "wavetable bank (184 oscillators)",
    0x12268F: "bank mix stage",
    0x122296: "resonator B (event 5 processor)",
    0x12272D: "oscillator bank with pitch LFO (84 + 36)",
}
SCRATCH = aec.SCRATCH
MAX_STEPS = 8


@dataclass(frozen=True)
class SlotWrite:
    """One call of the slot setter: `routine` into `slot` while the chain was in `state`."""

    state: int
    slot: int
    routine: int


@dataclass
class ModelInstall:
    model: int
    name: str
    layer: int
    event: int
    writes: list[SlotWrite] = field(default_factory=list)
    words: dict[int, int] = field(default_factory=dict)  # PNO core 1 DM word -> last value

    def routine(self, state: int) -> int:
        """Last non-idle routine written in `state` (0 when the slot stayed idle)."""
        found = [w.routine for w in self.writes if w.state == state and w.routine != IDLE_ROUTINE]
        return found[-1] if found else 0

    def slot(self, state: int) -> int:
        """Slot index of the last non-idle write in `state`, else of the last write, else -1."""
        real = [w for w in self.writes if w.state == state and w.routine != IDLE_ROUTINE]
        any_ = [w for w in self.writes if w.state == state]
        pick = real or any_
        return pick[-1].slot if pick else -1

    def record(self, state: int = 2) -> int | None:
        """Slot buffer pointer (`I1` of the routine) written for the slot of `state`."""
        return self.words.get(SLOT_TABLE_DM + SLOT_STRIDE * self.slot(state) + 2)


def parse_names(code: bytes, address: int = NAMES_ADDRESS, count: int = NAME_COUNT) -> list[str]:
    """The model names: consecutive NUL-terminated strings (padding NULs skipped)."""
    pos = address - ae.LOAD_BASE
    names: list[str] = []
    while len(names) < count and pos < len(code):
        end = code.index(b"\0", pos)
        if end > pos:
            names.append(code[pos:end].decode("ascii"))
        pos = end + 1
    return names


def ranges(words: list[int]) -> list[tuple[int, int]]:
    """Maximal runs of consecutive word addresses as `(first, last)`."""
    out: list[tuple[int, int]] = []
    for w in sorted(set(words)):
        if out and w == out[-1][1] + 1:
            out[-1] = (out[-1][0], w)
        else:
            out.append((w, w))
    return out


def pno_words(messages: list[ae.Message]) -> dict[int, int]:
    """DM word -> last value of the PNO core 1 path 2 messages."""
    return {m.dm_word: m.word2 for m in messages if m.chip == "PNO" and m.core == 1 and m.path == 2}


def ensure_scratch(emu: ae.ArmEmu) -> None:
    if not any(b <= SCRATCH <= e for b, e, _ in emu.uc.mem_regions()):
        emu.uc.mem_map(SCRATCH, ae.PAGE)


class SlotRecorder:
    """Code hooks on the chain step and the slot setter: `(state, slot, routine)` per write.

    The state is read at the entry of `0xC022F060` (the step advances it itself), so a write is
    tagged with the state the step was *called* in: 1 = generator slot, 2 = processor slot.
    """

    def __init__(self, emu: ae.ArmEmu) -> None:
        self.emu = emu
        self.state = -1
        self.log: list[tuple[int, int, int]] = []
        self.steps = 0
        self._handles: list[int] = []

    def __enter__(self) -> SlotRecorder:
        u = self.emu._u

        def on_step(_uc, _address, _size, _user) -> None:  # noqa: ANN001
            self.state = self.emu.read32(self.emu.reg(0) + FIELD_STATE)
            self.steps += 1

        def on_slot(_uc, _address, _size, _user) -> None:  # noqa: ANN001
            self.log.append((self.state, self.emu.reg(0), self.emu.reg(1)))

        uc = self.emu.uc
        self._handles = [
            uc.hook_add(u.UC_HOOK_CODE, on_step, begin=CHAIN_STEP, end=CHAIN_STEP),
            uc.hook_add(u.UC_HOOK_CODE, on_slot, begin=SLOT_SETTER, end=SLOT_SETTER),
        ]
        return self

    def __exit__(self, *_exc: object) -> None:
        for h in self._handles:
            self.emu.uc.hook_del(h)


def step_chain(emu: ae.ArmEmu, chain: int) -> int:
    """Call `0xC022F060(chain)` until the state reaches `STATE_DONE`; returns the calls made."""
    calls = 0
    while emu.read32(chain + FIELD_STATE) != STATE_DONE and calls < MAX_STEPS:
        ast.call_preserving(emu, CHAIN_STEP, chain, max_insns=5_000_000)
        calls += 1
    return calls


def install_model(
    emu: ae.ArmEmu, store: aec.Store, model: int, name: str = "", ticks: int = 0
) -> ModelInstall:
    """Apply `Edt_Org_kModel = model` on the organ layer of `store` and record the install.

    The handler is called like the dispatcher does (`r0` = store, `r2` -> `{0x166, 1}`, `r3` =
    model). `ticks` > 0 first runs the main loop for that many instructions so the queued chain
    step and the deferred senders (bank programmer, layer re-apply) run as in the firmware
    (100 M instructions cover the +0x120 tick delay); a chain that has not finished is then
    stepped directly, which gives the same slot writes (VERIFIED: 120 M instruction run).
    """
    ensure_scratch(emu)
    chain = emu.read32(store.address + CHAIN_FIELD)
    emu.write32(SCRATCH, MODEL_ID)
    emu.write32(SCRATCH + 4, 1)
    emu.clear_messages()
    with SlotRecorder(emu) as rec:
        ast.call_preserving(emu, MODEL_HANDLER, store.address, 0, SCRATCH, model)
        event = emu.read32(chain + FIELD_EVENT)
        if ticks:
            ast.run_ticking(emu, ticks)
        step_chain(emu, chain)
    out = ModelInstall(model, name, store.layer, event)
    out.writes = [SlotWrite(s, slot, r) for s, slot, r in rec.log]
    out.words = pno_words(emu.messages)
    return out


def sweep(
    emu: ae.ArmEmu,
    blob: bytes,
    stores: list[aec.Store],
    models: list[int],
    names: list[str],
    ticks: int = 0,
) -> list[ModelInstall]:
    """Every model on every organ layer, each from the settled state."""
    rows = []
    for store in (s for s in stores if s.kind == "organ"):
        for model in models:
            ast.restore(emu, blob)
            emu.install_dsp_ports()
            ast.enable_real_timer(emu)
            name = names[model] if model < len(names) else ""
            rows.append(install_model(emu, store, model, name, ticks))
    return rows


def extra_words(row: ModelInstall, base: ModelInstall) -> list[int]:
    """Words the model burst sends that the baseline model does not send with that value."""
    return [w for w, v in row.words.items() if base.words.get(w) != v]


def routine_name(address: int) -> str:
    return ROUTINES.get(address, f"{address:#x}") if address else "idle"


def table_rows(installs: list[ModelInstall], base_model: int = 0) -> list[dict[str, str]]:
    """One CSV row per (layer, model); `extra_dm` lists runs of words that differ from model 0."""
    out = []
    for layer in sorted({i.layer for i in installs}):
        mine = [i for i in installs if i.layer == layer]
        base = next((i for i in mine if i.model == base_model), mine[0])
        for i in mine:
            runs = ranges(extra_words(i, base)) if i is not base else []
            rec = i.record()
            out.append(
                {
                    "layer": str(layer),
                    "model": str(i.model),
                    "name": i.name,
                    "event": str(i.event),
                    "generator_slot": str(i.slot(1)),
                    "generator": f"{i.routine(1):#x}" if i.routine(1) else "idle",
                    "processor_slot": str(i.slot(2)),
                    "processor": f"{i.routine(2):#x}" if i.routine(2) else "idle",
                    "record_i1": f"{rec:#x}" if rec is not None else "",
                    "extra_dm": ";".join(
                        f"{a:#x}-{b:#x}" if a != b else f"{a:#x}" for a, b in runs
                    ),
                }
            )
    return out


def table_csv(rows: list[dict[str, str]]) -> str:
    buf = io.StringIO()
    w = csv.DictWriter(buf, fieldnames=list(rows[0]), lineterminator="\n")
    w.writeheader()
    w.writerows(rows)
    return buf.getvalue()


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path, nargs="?")
    ap.add_argument("--cache", type=Path, help="settled snapshot to load or create")
    ap.add_argument("--models", default="0,1,2,3,4,5")
    ap.add_argument("--ticks", type=int, default=0, help="main loop instructions before stepping")
    ap.add_argument("--csv", type=Path)
    ns = ap.parse_args(argv)
    emu, blob = aec.settled_emulator(ns.image, ns.sram, ns.cache)
    names = parse_names(ns.image.read_bytes())
    stores = aec.first_store_per_layer(aec.find_stores(emu))
    models = [int(m) for m in ns.models.split(",")]
    installs = sweep(emu, blob, stores, models, names, ns.ticks)
    rows = table_rows(installs)
    text = table_csv(rows)
    if ns.csv:
        ns.csv.write_text(text, encoding="utf-8")
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
