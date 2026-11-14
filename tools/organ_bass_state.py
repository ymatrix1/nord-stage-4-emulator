#!/usr/bin/env python3
"""Key-down state of the B3 Bass processor from the real ARM code: weight increments and `G`.

`docs/research/organ-farfisa-bass.md` s11. The key-down of `Edt_Org_kModel = 5` (drawbars
`888888888`, velocity 100, ascending keys 24..108, one emulator session) writes

* window 1 `DM 0x951F1 + k` and window 2 `DM 0x95241 + k` of the Bass processor, additively,
* the output gain word `DM 0xB44C9` (`G`), a soft clip `G = phi(S)` of the plain sum `S` of
  per-key steps (`phi` has slope `2^-n` on `S` in `[n/2, (n+1)/2)`).

    uv run --with unicorn --with capstone --with numpy python tools/organ_bass_state.py capture \
        target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin \
        docs/research/data/bass-state.csv --cache target/settled.snap

The CSV is `key,window,index,delta_bits`: window 1 / 2 rows are the f32 increment of a word,
window 0 rows (index 0) are the step of `S`. `verify` replays the rule.
"""

from __future__ import annotations

import argparse
import math
import struct
import sys
from pathlib import Path

W1_BASE, W1_WORDS = 0x951F1, 56
W2_BASE, W2_WORDS = 0x95241, 68
G_WORD = 0xB44C9


def f32(bits: int) -> float:
    return struct.unpack("<f", struct.pack("<I", bits))[0]


def f32_bits(x: float) -> int:
    return struct.unpack("<I", struct.pack("<f", x))[0]


def phi(s: float) -> float:
    """`G` from the plain sum `S` (VERIFIED against six ARM sequences, max error 5e-7)."""
    n = math.floor(2 * s)
    return 1 - 2.0**-n + (s - n / 2) * 2.0**-n


def phi_inv(g: float) -> float:
    """`S` from `G` (inverse of `phi`)."""
    n = math.floor(-math.log2(1 - g))
    return n / 2 + (g - (1 - 2.0**-n)) * 2.0**n


def window_index(addr: int) -> tuple[int, int] | None:
    """`(window, index)` of a weight word the processor reads, `None` otherwise."""
    if W1_BASE <= addr < W1_BASE + W1_WORDS:
        return 1, addr - W1_BASE
    if W2_BASE <= addr < W2_BASE + W2_WORDS:
        return 2, addr - W2_BASE
    return None


def key_rows(
    key: int, words: list[tuple[int, int]], state: dict[tuple[int, int], float], g_prev: float
) -> tuple[list[tuple[int, int, int, float]], float, float]:
    """`(rows, step of S, new G)` of one key-down; `state` holds the word values before the key."""
    new: dict[tuple[int, int], float] = {}
    g = g_prev
    for addr, bits in words:
        pos = window_index(addr)
        if pos is not None:
            new[pos] = f32(bits)
        if addr == G_WORD:
            g = f32(bits)
    rows = []
    for pos, value in sorted(new.items()):
        rows.append((key, pos[0], pos[1], value - state.get(pos, 0.0)))
        state[pos] = value
    step = phi_inv(g) - (phi_inv(g_prev) if g_prev > 0 else 0.0)
    return rows, step, g


def csv_lines(rows: list[tuple[int, int, int, float]], steps: dict[int, float]) -> list[str]:
    out = ["key,window,index,delta_bits"]
    out += [f"{k},{w},{i},{f32_bits(d):#010x}" for k, w, i, d in rows]
    out += [f"{k},0,0,{f32_bits(s):#010x}" for k, s in sorted(steps.items())]
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
    rec = emu.read32(ob.RECORD_TABLE)
    for k, v in enumerate(int(c) for c in a.drawbars):
        emu.write32(rec + ob.DRAWBAR_WORDS + 4 * k, v)
    emu.uc.mem_write(rec + ob.DIRTY, bytes([1]))
    emu.uc.mem_write(rec + ob.BUSY, bytes([1]))
    ast.call_preserving(emu, ob.RECORD_UPDATE, rec, max_insns=20_000_000)
    state: dict[tuple[int, int], float] = {}
    rows: list[tuple[int, int, int, float]] = []
    steps: dict[int, float] = {}
    g = 0.0
    for key in range(a.first, a.last + 1):
        words = []
        for off in (ob.SUB_DRAWBARS, ob.SUB_PERCUSSION):
            emu.clear_messages()
            ast.call_preserving(
                emu, ob.ITEM_KEYDOWN, emu.read32(rec + off), key, 100, 0, max_insns=20_000_000
            )
            words += [
                (int(m.to_dict()["dm"], 16), int(m.to_dict()["w2"], 16))
                for m in emu.messages
                if m.to_dict()["chip"] == "PNO"
            ]
        r, steps[key], g = key_rows(key, words, state, g)
        rows += r
    a.csv.write_text("\n".join(csv_lines(rows, steps)) + "\n", encoding="utf-8")
    print(f"{len(rows)} word increments, final G {g:.6f}")
    return 0


def cmd_verify(a: argparse.Namespace) -> int:
    steps = {}
    for line in a.csv.read_text().splitlines()[1:]:
        key, win, _idx, bits = line.split(",")
        if win == "0":
            steps[int(key)] = f32(int(bits, 16))
    for seq in ([60], [60, 64, 67], [67, 64, 60], list(range(24, 109))):
        print(seq[:3], f"G = {phi(sum(steps[k] for k in seq)):.6f}")
    return 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("capture")
    c.add_argument("code", type=Path)
    c.add_argument("sram", type=Path)
    c.add_argument("csv", type=Path)
    c.add_argument("--drawbars", default="888888888")
    c.add_argument("--first", type=int, default=24)
    c.add_argument("--last", type=int, default=108)
    c.add_argument("--cache", type=Path)
    v = sub.add_parser("verify")
    v.add_argument("csv", type=Path)
    ns = ap.parse_args(argv)
    return cmd_capture(ns) if ns.cmd == "capture" else cmd_verify(ns)


if __name__ == "__main__":
    sys.exit(main())
