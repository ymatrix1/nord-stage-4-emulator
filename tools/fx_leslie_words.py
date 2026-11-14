#!/usr/bin/env python3
"""Write the derived numeric words of the Leslie processor as Rust data files.

The processor reads (and writes) a few windows of the PNO core 1 data memory: its row and context
words, the 65-point rotor tables, the delay line state, the cabinet coefficient runs and the table
pointers. The windows below were found by running the verified model for 3,000 calls with the
rotor running (`touched` addresses, merged); this tool reads their words from the flat PNO core 1
image under the boot log (`tools/arm_bulk_tables.py capture`), adds the entry register files of
the oracle and writes `fw_leslie_data_a.rs`, `_b.rs`, `_c.rs` (derived numbers, no code):

    python tools/fx_leslie_words.py PNO_core1_sram.bin pno_core1.script --binary sharc-fx.exe

Provenance: VERIFIED against the oracle by `tests_fw_leslie_golden.rs` (output words of thousands
of calls) and fingerprinted by `tests_fw_leslie_data.rs` (FNV-1a 64 of every window).
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import fx_leslie_check as lc
import fx_leslie_ops as ops
from fx_pe import parse_traces

FX_DIR = Path(__file__).resolve().parents[1] / "crates" / "nord-core" / "src" / "fx"
#: (first, last) data addresses of the touched windows; the signal pair window comes last.
WINDOWS = (
    (0x965FC, 0x9660B), (0x967A6, 0x967D5), (0x96970, 0x96990), (0xB0078, 0xB008F),
    (0xB00E5, 0xB00E5), (0xB0276, 0xB0277), (0xB02F7, 0xB02F8), (0xB0378, 0xB0379),
    (0xB2F0A, 0xB2FEE), (0xB474C, 0xB52A6), (0xB6000, 0xB6003), (0xC0142, 0xC0145),
    (0xE05F4, 0xE060D), (0xE30CC, 0xE31C9), (0xE322C, 0xE322F), (0xE34C4, 0xE35C1),
)  # fmt: skip
#: window index ranges of the three files (the big table alone in `_b`).
SPLIT = ((0, 9), (9, 10), (10, 16))


def fnv(words: list[int]) -> int:
    h = 0xCBF29CE484222325
    for w in words:
        for sh in (0, 8, 16, 24):
            h = ((h ^ ((w >> sh) & 0xFF)) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def window_words(mem, lo: int, hi: int) -> list[int]:
    return [mem[a] for a in range(lo, hi + 1)]


def rust_window(name: str, lo: int, words: list[int]) -> list[str]:
    lines = [f"const {name}: [u32; {len(words)}] = ["]
    for i in range(0, len(words), 8):
        lines.append("    " + ", ".join(f"{w:#x}" for w in words[i : i + 8]) + ",")
    lines.append("];")
    return lines


def render(mem, entry: dict, first: int, last: int, tag: str) -> str:
    out = [
        f"//! Derived words of the Leslie processor, part {tag}: windows of the PNO core 1 DM,",
        "//! (VERIFIED: the boot log of OS v1.66 over the flat image, `tools/fx_leslie_words.py`;",
        "//! checked by `tests_fw_leslie_golden.rs` word for word against the oracle).",
        "",
    ]
    consts = []
    for k in range(first, last):
        lo, hi = WINDOWS[k]
        out += rust_window(f"W{k}", lo, window_words(mem, lo, hi))
        consts.append(f"({lo:#x}, &W{k})")
    out += [
        "",
        f"pub(super) const WINDOWS: [(u32, &[u32]); {last - first}] = [{', '.join(consts)}];",
    ]
    if tag == "a":
        for key, name in zip("rsimlb", ("R", "S", "I", "M", "L", "B"), strict=True):
            out += [
                f"/// Entry {name} file of the oracle call (the pass context of `0x1212E7`).",
                f"pub(super) const ENTRY_{name}: [u32; 16] = [",
                "    " + ", ".join(f"{w:#x}" for w in entry[key][:8]) + ",",
                "    " + ", ".join(f"{w:#x}" for w in entry[key][8:]) + ",",
                "];",
            ]
        out += [
            "/// The slow speed words as the ARM re-sends them (`0xB2F25`, `0xB2F26`, `0xB2FA1`).",
            "pub(super) const SLOW_SPEED: [u32; 3] = [0x22b00, 0x26000, 0x15d00];",
        ]
    return "\n".join(out) + "\n"


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("pno_core1", type=Path)
    ap.add_argument("script", type=Path)
    ap.add_argument("--binary", type=Path, default=Path("target/release/sharc-fx.exe"))
    ns = ap.parse_args(argv)
    overlay = lc.lr_boot(ns.script)
    overlay.update(lc.rotor_words())
    mem = ops.LazyMem(ns.pno_core1.read_bytes(), overlay)
    row0 = parse_traces(lc.oracle(ns.pno_core1, ns.script, 1, ns.binary))[0][0]
    for tag, (a, b) in zip("abc", SPLIT, strict=True):
        text = render(mem, row0, a, b, tag)
        (FX_DIR / f"fw_leslie_data_{tag}.rs").write_text(text, encoding="utf-8", newline="\n")
    for k, (lo, hi) in enumerate(WINDOWS):
        print(f"W{k} {lo:#x}..{hi:#x} fnv {fnv(window_words(mem, lo, hi)):#018x}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
