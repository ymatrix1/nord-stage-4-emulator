#!/usr/bin/env python3
"""Replay scripts for the complete PNO core 1 pass (`sharc-pass`): organ model installs and keys.

The booted ARM firmware (`arm_steady.py` snapshot) sends everything the PNO core 1 needs as
`(system address, value)` messages. This tool drives the same ARM entry points as the firmware and
writes the messages in the order they were sent as `PASS ADDR VALUE` lines (`sharc-pass --script`,
`sharc-org --msg-script --msg-base 0`; addresses are data memory words of core 1):

* `model N`   `Edt_Org_kModel = N` on organ layer `--layer` (handler `0xC011C9BC`, then the chain
              step `0xC022F060` until it is done): call table writes `DM(0xC01D6 + 4 slot)`,
              slot buffer pointers, parameter blocks. `--previous M` first applies model M, so
              the install runs with the chain `kind` of a model switch instead of the settled
              state (Vox after Vox zeroes the 96 weights `DM(0x95E46..0x95EA5)`). `--ticks N`
              runs the ARM main loop first so that deferred senders fire.

The bank increments and the per-key weights come from `arm_bank.py` and `arm_organ_weights.py`
(`script` command); `sharc-pass --script` reads all of them.

    uv run --with unicorn python tools/arm_pass_script.py model target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --cache target/settled.snap --model 1 --out vox.script

Evidence, tables and limits: `docs/research/pno-full-pass.md`.
"""

from __future__ import annotations

import argparse
import sys
from collections.abc import Iterable
from pathlib import Path

import arm_edit_capture as aec
import arm_emu as ae
import arm_steady as ast

# VERIFIED (disassembly 0xC011C9BC..0xC011CA70 + run, `organ-model-paths.md`): the organ layer
# client handler `(store, _, &{id, 1}, model)`; for id `0x166` it stores the model as the event of
# the chain object `[store + 0x98]` and queues the step.
MODEL_HANDLER = 0xC011C9BC
MODEL_ID = 0x166
CHAIN_FIELD = 0x98
# VERIFIED: chain step and its fields (state 0..3, kind = previous event, event = model).
CHAIN_STEP = 0xC022F060
FIELD_STATE, FIELD_KIND, FIELD_EVENT = 0xB0, 0x8C, 0x90
STATE_DONE = 3
MAX_STEPS = 8
SCRATCH = aec.SCRATCH


def script_line(m: ae.Message, note: str = "") -> str:
    """`0 ADDR VALUE  # note` for a PNO core 1 path 2 message (pass 0)."""
    tail = f"  # {note}" if note else ""
    return f"0 {m.dm_word:#x} {m.word2:#010x}{tail}"


def core1_words(messages: Iterable[ae.Message]) -> list[ae.Message]:
    """The messages that land in PNO core 1 data memory, in send order."""
    return [m for m in messages if m.chip == "PNO" and m.core == 1 and m.path == 2]


def script_text(messages: Iterable[ae.Message], note: str = "", shift: int = 0) -> str:
    """Script lines of the PNO core 1 messages; `shift` is added to the pass number."""
    lines = []
    for m in core1_words(messages):
        lines.append(
            script_line(m, note).replace("0 ", f"{shift} ", 1) if shift else script_line(m, note)
        )
    return "\n".join(lines) + ("\n" if lines else "")


def dedup_last(messages: Iterable[ae.Message]) -> list[ae.Message]:
    """Messages with a repeated address reduced to the last one (order of the last writes kept)."""
    last: dict[int, ae.Message] = {}
    for m in core1_words(messages):
        last.pop(m.dm_word, None)
        last[m.dm_word] = m
    return list(last.values())


def ensure_scratch(emu: ae.ArmEmu) -> None:
    if not any(b <= SCRATCH <= e for b, e, _ in emu.uc.mem_regions()):
        emu.uc.mem_map(SCRATCH, ae.PAGE)


def step_chain(emu: ae.ArmEmu, chain: int) -> int:
    """Call `0xC022F060(chain)` until the state reaches `STATE_DONE`; returns the calls made."""
    calls = 0
    while emu.read32(chain + FIELD_STATE) != STATE_DONE and calls < MAX_STEPS:
        ast.call_preserving(emu, CHAIN_STEP, chain, max_insns=5_000_000)
        calls += 1
    return calls


def apply_model(emu: ae.ArmEmu, store: aec.Store, model: int, ticks: int = 0) -> int:
    """`Edt_Org_kModel = model` on the organ layer of `store`, chain stepped to the end.

    `ticks` > 0 first runs the main loop for that many ARM instructions so that the queued chain
    step and the deferred senders (bank programmer, layer re-apply) run as in the firmware
    (100 M instructions cover the +0x120 tick delay).
    """
    ensure_scratch(emu)
    chain = emu.read32(store.address + CHAIN_FIELD)
    emu.write32(SCRATCH, MODEL_ID)
    emu.write32(SCRATCH + 4, 1)
    ast.call_preserving(emu, MODEL_HANDLER, store.address, 0, SCRATCH, model)
    if ticks:
        ast.run_ticking(emu, ticks)
    step_chain(emu, chain)
    return emu.read32(chain + FIELD_KIND)


def model_messages(
    emu: ae.ArmEmu, store: aec.Store, model: int, previous: int | None = None, ticks: int = 0
) -> list[ae.Message]:
    """Messages of the install of `model` (after `previous`, whose messages are dropped)."""
    if previous is not None:
        apply_model(emu, store, previous, ticks)
    emu.clear_messages()
    apply_model(emu, store, model, ticks)
    return list(emu.messages)


def organ_store(emu: ae.ArmEmu, layer: int) -> aec.Store:
    stores = [s for s in aec.first_store_per_layer(aec.find_stores(emu)) if s.kind == "organ"]
    return stores[layer]


def fresh(emu: ae.ArmEmu, blob: bytes) -> None:
    ast.restore(emu, blob)
    emu.install_dsp_ports()
    ast.enable_real_timer(emu)
    emu.clear_messages()


def cmd_model(ns: argparse.Namespace) -> str:
    emu, blob = aec.settled_emulator(ns.image, ns.sram, ns.cache)
    fresh(emu, blob)
    msgs = model_messages(emu, organ_store(emu, ns.layer), ns.model, ns.previous, ns.ticks)
    return script_text(msgs, f"Edt_Org_kModel={ns.model} layer {ns.layer}")


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("command", choices=("model",))
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path, nargs="?")
    ap.add_argument("--cache", type=Path, help="settled snapshot to load or create")
    ap.add_argument("--model", type=int, default=1)
    ap.add_argument("--previous", type=int)
    ap.add_argument("--layer", type=int, default=0)
    ap.add_argument("--ticks", type=int, default=0, help="main loop instructions before stepping")
    ap.add_argument("--out", type=Path)
    ns = ap.parse_args(argv)
    text = {"model": cmd_model}[ns.command](ns)
    if ns.out:
        ns.out.write_text(text, encoding="utf-8")
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
