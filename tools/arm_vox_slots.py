#!/usr/bin/env python3
"""Static decode of the Vox level-slot setter `0xC0228E00(this, i, value)` of the host firmware.

The setter jumps through a 9 entry table at `0xC0228E28` (drawbar index `i` = 0..8); each case
stores `value` into level slots `[this + 0xD18 + 8 slot]` (`str r2, [r0, #off]`) and then calls the
weight sender `0xC0224468`. This tool lists the stores of every case:

    python tools/arm_vox_slots.py target/fw_unpacked/bank0_CODE.bin

Result (docs/research/organ-vox-start-state.md section 4): drawbar index 0..3 -> slots 0, 2, 5, 10
(the default program's 16', 8', 4', 2'), 4 -> 3, 13; 5 -> 8, 11, 16; 6 -> 15, 17, 18, 19; 7 -> none;
8 -> not a level (`[this + 0xDE4]`). Slots 1, 4, 6, 7, 9, 12, 14 are not written by this setter.
Needs `capstone`.
"""

from __future__ import annotations

import argparse
import struct
import sys
from pathlib import Path

LOAD_BASE = 0xC0000000
SETTER, TABLE, CASES = 0xC0228E00, 0xC0228E28, 9
LEVEL_BASE, SLOT_STRIDE = 0xD18, 8


def slot_of(offset: int) -> int:
    """Level slot of a store to `[this + offset]`."""
    return (offset - LEVEL_BASE) // SLOT_STRIDE


def case_slots(offsets: list[int]) -> list[int]:
    """Slots written by one case, from its `str r2, [r0, #off]` offsets in the level range."""
    return [slot_of(o) for o in offsets if LEVEL_BASE <= o < LEVEL_BASE + 20 * SLOT_STRIDE]


def word(code: bytes, addr: int) -> int:
    return struct.unpack_from("<I", code, addr - LOAD_BASE)[0]


def case_stores(code: bytes, addr: int, limit: int = 40) -> list[int]:  # pragma: no cover
    import capstone

    md = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_ARM)
    md.detail = False
    offsets: list[int] = []
    start = addr - LOAD_BASE
    for ins in md.disasm(code[start : start + 4 * limit], addr):
        if ins.mnemonic == "str" and ins.op_str.startswith("r2, [r0, #"):
            offsets.append(int(ins.op_str.split("#")[1].rstrip("]"), 16))
        if ins.mnemonic == "b" and ins.op_str == "#0xc0224468" or ins.mnemonic == "pop":
            break
    return offsets


LEVEL_TABLE = 0xC0443AF4 + 0x278  # indexed by the stored drawbar value 0..8


def levels(code: bytes) -> list[int]:
    """The 23-bit level of drawbar values 0..8 (`[0xC0443AF4 + 0x278 + 4 v]`)."""
    return [struct.unpack_from("<i", code, LEVEL_TABLE - LOAD_BASE + 4 * v)[0] for v in range(9)]


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("code")
    ap.add_argument("what", nargs="?", default="slots", choices=("slots", "levels"))
    ns = ap.parse_args(argv)
    code = Path(ns.code).read_bytes()
    if ns.what == "levels":
        print(levels(code))
        return 0
    for i in range(CASES):
        target = word(code, TABLE + 4 * i)
        print(i, hex(target), case_slots(case_stores(code, target)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
