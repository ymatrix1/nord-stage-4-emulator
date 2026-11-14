#!/usr/bin/env python3
"""Key-down weights of the B3 Bass model (model 5) for every key, from the real ARM code.

`organ_fb_pass.capture_chord` installs `Edt_Org_kModel = 5`, sets the drawbars and sends the
key-down of the drawbar item and the percussion item (`0xC00BF890`) for each key. The PNO core 1
path 2 words land in two weight windows of the Bass processor `0x122296`
(`docs/research/organ-farfisa-bass.md` s10):

* window 1 `DM 0x951F1 + k` (56 words): written by the drawbar item, ring slot `0xB32E8 + k`,
* window 2 `DM 0x95241 + k` (68 words): written by the percussion item, ring slot `0xB32F4 + k`.

    uv run --with unicorn --with capstone python tools/organ_bass_keys.py capture \
        target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin \
        bass_keys.script bass-keydown.csv --cache target/settled.snap

`capture` writes the script (every key-down word, to populate the generator ring) and the CSV
`key,window,index,value_bits` of the weight words in send order.
"""

from __future__ import annotations

import argparse
import csv
import sys
from pathlib import Path

import organ_fb_pass as fb

W1_BASE, W1_WORDS = 0x951F1, 56
W2_BASE, W2_WORDS = 0x95241, 68


def window_of(addr: int) -> tuple[int, int] | None:
    """`(window, index)` of a weight word address, `None` outside both windows."""
    if W1_BASE <= addr < W1_BASE + W1_WORDS:
        return 1, addr - W1_BASE
    if W2_BASE <= addr < W2_BASE + W2_WORDS:
        return 2, addr - W2_BASE
    return None


def weight_rows(key: int, words: list[tuple[int, int]]) -> list[tuple[int, int, int, int]]:
    """`(key, window, index, value_bits)` of the weight words of one key-down, in send order."""
    rows = []
    for addr, value in words:
        pos = window_of(addr)
        if pos is not None:
            rows.append((key, pos[0], pos[1], value))
    return rows


def first_values(rows: list[tuple[int, int, int, int]], window: int) -> dict[int, dict[int, int]]:
    """`key -> {index: first written value}` of one window."""
    out: dict[int, dict[int, int]] = {}
    for key, win, idx, value in rows:
        if win == window:
            out.setdefault(key, {}).setdefault(idx, value)
    return out


def cmd_capture(a: argparse.Namespace) -> int:  # pragma: no cover - needs the ARM harness
    import arm_edit_capture as aec
    import arm_model_paths as mp
    import arm_steady as ast
    import organ_b3_pass as ob

    emu, _ = aec.settled_emulator(a.code, a.sram, a.cache)
    mp.ensure_scratch(emu)
    store = next(s for s in aec.first_store_per_layer(aec.find_stores(emu)) if s.kind == "organ")
    mp.install_model(emu, store, 5)
    msgs = [m.to_dict() for m in emu.messages]
    rec = emu.read32(ob.RECORD_TABLE)
    for k, v in enumerate(int(c) for c in a.drawbars):
        emu.write32(rec + ob.DRAWBAR_WORDS + 4 * k, v)
    emu.uc.mem_write(rec + ob.DIRTY, bytes([1]))
    emu.uc.mem_write(rec + ob.BUSY, bytes([1]))
    emu.clear_messages()
    ast.call_preserving(emu, ob.RECORD_UPDATE, rec, max_insns=20_000_000)
    msgs += [m.to_dict() for m in emu.messages]
    rows: list[tuple[int, int, int, int]] = []
    for key in range(a.first, a.last + 1):
        for off in (ob.SUB_DRAWBARS, ob.SUB_PERCUSSION):
            emu.clear_messages()
            ast.call_preserving(
                emu, ob.ITEM_KEYDOWN, emu.read32(rec + off), key, 100, 0, max_insns=20_000_000
            )
            batch = [m.to_dict() for m in emu.messages]
            msgs += batch
            words = [(int(m["dm"], 16), int(m["w2"], 16)) for m in batch if m["chip"] == "PNO"]
            rows += weight_rows(key, words)
    a.script.write_text("\n".join(fb.script_lines(msgs)) + "\n", encoding="utf-8")
    with open(a.csv, "w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(["key", "window", "index", "value_bits"])
        w.writerows((k, win, i, f"{v:#010x}") for k, win, i, v in rows)
    print(f"{len(rows)} weight words, {len(msgs)} messages")
    return 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("capture")
    c.add_argument("code", type=Path)
    c.add_argument("sram", type=Path)
    c.add_argument("script", type=Path)
    c.add_argument("csv", type=Path)
    c.add_argument("--drawbars", default="888888888")
    c.add_argument("--first", type=int, default=24)
    c.add_argument("--last", type=int, default=108)
    c.add_argument("--cache", type=Path)
    return cmd_capture(ap.parse_args(argv))


if __name__ == "__main__":
    sys.exit(main())
