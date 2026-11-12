#!/usr/bin/env python3
"""Drive firmware senders in the ARM emulator (`tools/arm_emu.py`) and capture the DSP messages.

Scenarios (all against `bank0_CODE.bin` + `bank0_SRAM.bin` of the local firmware; nothing from the
firmware is written out, only message addresses and values):

* `strips`      the strip parameter setter `0xC0017088(this, id, &value)` for ids 0..0x41 and a
                set of Q16.16 inputs; one JSONL record per (id, value) plus a CSV table
* `controllers` the controller-word senders `0xC01CE034` / `0xC01CE128` for the 48 slots
* `script`      JSONL -> `sharc-org --msg-script` text for one chip and core
* `probe`       call any function with up to four arguments

    uv run --with unicorn python tools/arm_capture.py strips  FW/bank0_CODE.bin FW/bank0_SRAM.bin \
        --jsonl OUT.jsonl --csv OUT.csv
    uv run --with unicorn python tools/arm_capture.py controllers FW/bank0_CODE.bin \
        FW/bank0_SRAM.bin --jsonl CTRL.jsonl
    python tools/arm_capture.py script OUT.jsonl ORG 2 > org_core2.msg
    sharc-org core2_sram.bin --msg-base 0 --msg-script org_core2.msg --diff 0xC0000:0xC2FFF

Evidence and limits: `docs/research/arm-fw-emulation.md`.
"""

from __future__ import annotations

import argparse
import csv
import json
import struct
import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import arm_emu as ae

# VERIFIED: tools/arm_params.py: bulk ORG/PNO channel-strip setter `(this, id, &value)`; id < 0x42
# selects a handler through a jump table at 0xC00170A4 (decoded in arm-fw-emulation.md).
STRIP_SETTER = 0xC0017088
STRIP_IDS = range(0x42)
# VERIFIED: sweep in arm-fw-emulation.md: the strip index k is the word at `this + 0x68`.
STRIP_INDEX_OFFSET = 0x68
# VERIFIED: org-stage-map.md: strips are 62 words from DM(0xE0000).
STRIP_BASE = 0xE0000
STRIP_WORDS = 62
# Q16.16 inputs: 0, 0.5, 1, 2, 4, 8, 16, -1, -16 (the setters round to the integer part).
SAMPLE_VALUES = (0, 0x8000, 0x10000, 0x20000, 0x40000, 0x80000, 0x100000, 0xFFFF0000, 0xFFF00000)
# VERIFIED: arm-param-messages.md s6, reproduced by `controllers`: `(this = {[0] = index,
# [8] = state})`, 48 slots in table 0xC05495C8.
CONTROLLER_SENDERS = (0xC01CE034, 0xC01CE128)
CONTROLLER_SLOTS = 48
SCRATCH_OBJ = 0x80010000  # SRAM block 0x80000000.. is mapped; this area is unused by the sweep
SCRATCH_VAL = 0x80020000


def f32(word: int) -> float:
    return struct.unpack("<f", struct.pack("<I", word & 0xFFFFFFFF))[0]


def new_session(code: Path, sram: Path | None) -> ae.ArmEmu:
    """Firmware loaded, static initialiser run, null calls guarded, poll loops broken."""
    emu = ae.load_firmware(code, sram)
    ae.run_static_init(emu)
    emu.guard_null_calls()
    emu.enable_poll_breaker()
    return emu


def strip_call(
    emu: ae.ArmEmu, param_id: int, value: int, strip: int = 0, max_insns: int = 300_000
) -> list[ae.Message]:
    """Call the strip setter on a zeroed object whose strip index is `strip`."""
    emu.write_bytes(SCRATCH_OBJ, bytes(0x800))
    emu.write32(SCRATCH_OBJ + STRIP_INDEX_OFFSET, strip)
    emu.write32(SCRATCH_VAL, value)
    emu.clear_messages()
    try:
        emu.call(STRIP_SETTER, SCRATCH_OBJ, param_id, SCRATCH_VAL, max_insns=max_insns)
    except Exception:  # unicorn error on an unmodelled path: treat as no message
        return []
    return emu.messages


def strip_sweep(
    emu: ae.ArmEmu,
    ids: range = STRIP_IDS,
    values: tuple[int, ...] = SAMPLE_VALUES,
    strip: int = 0,
) -> list[dict[str, object]]:
    """One record per (id, value) that produced at least one message."""
    rows: list[dict[str, object]] = []
    for pid in ids:
        for v in values:
            msgs = strip_call(emu, pid, v, strip)
            if msgs:
                rows.append(
                    {
                        "scenario": "strip",
                        "id": pid,
                        "strip": strip,
                        "input_q16": f"{v:#010x}",
                        "messages": [m.to_dict() for m in msgs],
                    }
                )
    return rows


def strip_table(rows: list[dict[str, object]]) -> list[dict[str, str]]:
    """CSV rows: one per id with the ORG core 2 DM words and the output per sample input.

    `dm_words` are relative to the strip base (`0xE0000 + 62 k`) when they fall inside the strip
    array, absolute otherwise (prefix `abs`). The sample columns show the first ORG core 2 message.
    """
    by_id: dict[int, dict[str, object]] = {}
    for r in rows:
        pid = int(r["id"])  # type: ignore[arg-type]
        ent = by_id.setdefault(pid, {"words": [], "out": {}, "chips": set()})
        for m in r["messages"]:  # type: ignore[union-attr]
            ent["chips"].add(f"{m['chip']}c{m['core']}")  # type: ignore[union-attr]
            if m["chip"] == "ORG" and m["core"] == 2:
                dm = int(m["dm"], 0)
                if dm not in ent["words"]:  # type: ignore[operator]
                    ent["words"].append(dm)  # type: ignore[union-attr]
                ent["out"].setdefault(r["input_q16"], m["w2"])  # type: ignore[union-attr]
    table = []
    for pid in sorted(by_id):
        ent = by_id[pid]
        words = []
        for dm in ent["words"]:  # type: ignore[union-attr]
            rel = dm - STRIP_BASE
            words.append(f"{rel}" if 0 <= rel < STRIP_WORDS * 12 else f"abs{dm:#x}")
        row = {
            "id": f"{pid:#x}",
            "chips": " ".join(sorted(ent["chips"])),  # type: ignore[arg-type]
            "org2_dm_words": " ".join(words),
        }
        for v in SAMPLE_VALUES:
            row[f"in_{v:#x}"] = ent["out"].get(f"{v:#010x}", "")  # type: ignore[union-attr]
        table.append(row)
    return table


def controller_call(
    emu: ae.ArmEmu, sender: int, index: int, value7: int = 0x55, flags: int = 0x20
) -> list[ae.Message]:
    """Call a controller-word sender with `state = (value7 << 8) | flags`."""
    emu.write_bytes(SCRATCH_OBJ, bytes(0x40))
    emu.write32(SCRATCH_OBJ, index)
    emu.write32(SCRATCH_OBJ + 8, (value7 << 8) | flags)
    emu.clear_messages()
    emu.call(sender, SCRATCH_OBJ, max_insns=10_000)
    return emu.messages


def slot_value(index: int) -> int:
    """Distinct 7-bit test value per slot so a replay shows which slot moved which word."""
    return (index * 8 + 3) & 0x7F


def controller_sweep(emu: ae.ArmEmu) -> list[dict[str, object]]:
    rows = []
    for sender in CONTROLLER_SENDERS:
        for i in range(CONTROLLER_SLOTS):
            value7 = slot_value(i)
            msgs = controller_call(emu, sender, i, value7)
            rows.append(
                {
                    "scenario": "controller",
                    "sender": f"{sender:#010x}",
                    "index": i,
                    "value7": value7,
                    "state_after": f"{emu.read32(SCRATCH_OBJ + 8):#010x}",
                    "messages": [m.to_dict() for m in msgs],
                }
            )
    return rows


def flatten(rows: list[dict[str, object]]) -> list[ae.Message]:
    out = []
    for r in rows:
        for m in r["messages"]:  # type: ignore[union-attr]
            out.append(
                ae.Message(m["path"], m["comm"], int(m["w1"], 0), int(m["w2"], 0), int(m["pc"], 0))
            )  # type: ignore[index]
    return out


def write_jsonl(rows: list[dict[str, object]], out: Path) -> None:
    with out.open("w") as fh:
        for r in rows:
            fh.write(json.dumps(r) + "\n")


def read_messages(
    path: Path,
    scenario: str | None = None,
    input_q16: str | None = None,
    sender: str | None = None,
) -> list[ae.Message]:
    """Messages from a JSONL written by `write_jsonl` (records) or `Message.to_json` (flat).

    `scenario`, `input_q16` (for example `0x00020000`) and `sender` (for example `0xc01ce034`) keep
    only matching records.
    """
    out: list[ae.Message] = []
    for line in path.read_text().splitlines():
        if not line.strip():
            continue
        d = json.loads(line)
        if scenario and d.get("scenario") != scenario:
            continue
        if input_q16 and d.get("input_q16") != input_q16:
            continue
        if sender and d.get("sender") != f"{int(sender, 0):#010x}":
            continue
        if "messages" in d:
            out.extend(flatten([d]))
        else:
            out.extend(ae.messages_from_jsonl(line))
    return out


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    for name in ("strips", "controllers"):
        p = sub.add_parser(name)
        p.add_argument("code", type=Path)
        p.add_argument("sram", type=Path)
        p.add_argument("--jsonl", type=Path)
        p.add_argument("--csv", type=Path)
        p.add_argument("--strip", type=int, default=0)
    p = sub.add_parser("script")
    p.add_argument("jsonl", type=Path)
    p.add_argument("chip", choices=list(ae.COMM_NAMES))
    p.add_argument("core", type=int, choices=(0, 1, 2))
    p.add_argument("--scenario", choices=("strip", "controller"))
    p.add_argument("--input", help="keep strip records with this Q16.16 input, e.g. 0x00020000")
    p.add_argument("--sender", help="keep controller records of this sender, e.g. 0xc01ce034")
    p = sub.add_parser("probe")
    p.add_argument("code", type=Path)
    p.add_argument("sram", type=Path)
    p.add_argument("args", nargs="+")
    ns = ap.parse_args(argv)
    if ns.cmd == "script":
        msgs = read_messages(ns.jsonl, ns.scenario, ns.input, ns.sender)
        sys.stdout.write(ae.to_msg_script(msgs, ns.chip, ns.core))
        return 0
    emu = new_session(ns.code, ns.sram)
    if ns.cmd == "probe":
        vals = [int(x, 0) for x in ns.args]
        emu.call(vals[0], *vals[1:], max_insns=1_000_000)
        for m in emu.messages:
            print(m.to_json())
        return 0
    rows = strip_sweep(emu, strip=ns.strip) if ns.cmd == "strips" else controller_sweep(emu)
    if ns.jsonl:
        write_jsonl(rows, ns.jsonl)
    if ns.csv and ns.cmd == "strips":
        table = strip_table(rows)
        with ns.csv.open("w", newline="") as fh:
            w = csv.DictWriter(fh, fieldnames=list(table[0]))
            w.writeheader()
            w.writerows(table)
    print(f"{len(rows)} records, {sum(len(r['messages']) for r in rows)} messages")  # type: ignore[arg-type]
    return 0


if __name__ == "__main__":
    sys.exit(main())
