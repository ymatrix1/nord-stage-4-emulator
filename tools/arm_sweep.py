#!/usr/bin/env python3
"""Sweep edit ids through the real store setter `0xC00D051C` and record every DSP message.

For each id of `docs/research/arm-edit-params.csv` whose name matches a filter, `sweep` calls
`0xC00D051C(store, 0, &{id, 1}, value)` on the organ layer store (`layer + 0x5AB4`) and logs the
messages the firmware sends to the fake DSP ports (`arm_emu.Message`). Run it on an emulator that
`arm_boot_full.boot_full` has advanced.

    uv run --with unicorn python tools/arm_sweep.py target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --boot-insns 800000000 --csv docs/research/arm-sweep.csv
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

# VERIFIED (docs/research/arm-organ-messages.md section 4): the layer's edit store sits at
# `layer + 0x5AB4`; its vtable slot 8 is the setter `0xC00D051C`.
STORE_OFFSET = 0x5AB4
STORE_SETTER = 0xC00D051C
# Names swept by default: organ, organ-common, Leslie, piano, effects and rotary parameters.
DEFAULT_FILTER = r"Edt_Org|Leslie|Edt_Pno|Rotary|Edt_Effects|Edt_Fx"
SCRATCH_OFFSET = 0xF0000  # inside the layer region, away from the layer object


@dataclass
class SweepRow:
    edit_id: int
    name: str
    value: int
    messages: list[ae.Message] = field(default_factory=list)
    null_calls: int = 0
    error: str = ""


def load_ids(csv_path: Path, pattern: str = DEFAULT_FILTER) -> list[tuple[int, str]]:
    """`(id, name)` pairs of the registry CSV whose name matches `pattern`."""
    rx = re.compile(pattern)
    out = []
    with csv_path.open(encoding="utf-8") as fh:
        for row in csv.DictReader(fh):
            if rx.search(row["name"]):
                out.append((int(row["id"], 0), row["name"]))
    return out


def sweep(
    emu: ae.ArmEmu, store: int, scratch: int, ids: list[tuple[int, str]], values: tuple[int, ...]
) -> list[SweepRow]:
    """Set every `(id, value)` through the store setter and collect the messages."""
    rows = []
    for edit_id, name in ids:
        for value in values:
            emu.write32(scratch, edit_id)
            emu.write32(scratch + 4, 1)
            emu.clear_messages()
            nulls = len(emu.null_calls)
            row = SweepRow(edit_id, name, value)
            try:
                emu.call(STORE_SETTER, store, 0, scratch, value, max_insns=2_000_000)
            except Exception as exc:  # unicorn raises UcError for faults
                row.error = str(exc)
            row.messages = list(emu.messages)
            row.null_calls = len(emu.null_calls) - nulls
            rows.append(row)
    return rows


def rows_csv(rows: list[SweepRow]) -> str:
    """One CSV line per message (or one empty line per silent id/value)."""
    out = ["id,name,value,chip,comm,core,path,dm_word,w2,pc,null_calls,error"]
    for r in rows:
        if not r.messages:
            out.append(f"{r.edit_id:#x},{r.name},{r.value},,,,,,,,{r.null_calls},{r.error}")
        for m in r.messages:
            out.append(
                f"{r.edit_id:#x},{r.name},{r.value},{m.chip},{m.comm},{m.core},{m.path},"
                f"{m.dm_word:#x},{m.word2:#010x},{m.pc:#x},{r.null_calls},{r.error}"
            )
    return "\n".join(out) + "\n"


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path, nargs="?")
    ap.add_argument("--boot-insns", type=int, default=800_000_000)
    ap.add_argument("--params", type=Path, default=Path("docs/research/arm-edit-params.csv"))
    ap.add_argument("--csv", type=Path)
    ap.add_argument("--filter", default=DEFAULT_FILTER)
    ns = ap.parse_args(argv)
    emu = bf.prepare(ns.image, ns.sram)
    r = bf.boot_full(emu, ns.boot_insns)
    print(f"boot stopped pc={r.pc:#x} after {r.instructions} error={r.error!r}", flush=True)
    emu.install_dsp_ports()
    layer = ab.construct_organ_layer(emu, dispatcher=ab.dispatcher(emu))
    ids = load_ids(ns.params, ns.filter)
    rows = sweep(emu, layer + STORE_OFFSET, layer + SCRATCH_OFFSET, ids, (1, 2))
    text = rows_csv(rows)
    if ns.csv:
        ns.csv.write_text(text, encoding="utf-8")
    live = [x for x in rows if x.messages]
    print(f"{len(ids)} ids, {len(rows)} sets, {len(live)} produced messages")
    return 0


if __name__ == "__main__":
    sys.exit(main())
