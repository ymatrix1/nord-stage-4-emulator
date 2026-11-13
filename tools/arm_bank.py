#!/usr/bin/env python3
"""Run the real ARM routines that program the PNO core 1 oscillator bank and the drawbar curve.

`0xC022D9F0(this, level)` writes the 184 phase increments (`word << 8`, one DM word each, from
the table selected by `level`) straight to the PNO comm port and hands four weight gains to a
virtual setter (`[this + 0x10]`, vtable slot 6) that the real boot builds; the harness replaces
that object graph with a small fake whose setter logs `(index, gain)`. The increments are real
firmware output, the gains are what the real code passes to the setter.

The drawbar curve is the 9-entry table the record update `0xC00BE348` indexes with the edit value.

    uv run --with unicorn python tools/arm_bank.py target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --script bank_level1.script --level 1
"""

from __future__ import annotations

import argparse
import struct
import sys
from dataclasses import dataclass
from pathlib import Path

import arm_emu as ae

# VERIFIED (disassembly + run): bank programming routine, six callers (organ apply
# `0xC01209C4/0xC0120C60`, part routing `0xC0236410`).
BANK_PROGRAM = 0xC022D9F0
# VERIFIED: inline fast path of the per-element write; the routine compares the element writer
# `vtable[4]` against this address and, when equal, stores to the comm port itself.
INLINE_ELEMENT_WRITER = 0xC022A9A8
# VERIFIED (read from the image): increment tables, level 1 (detuned) and the exact 12-TET one.
DRAWBAR_CURVE_TABLE = 0xC02B0EC0
# Harness-only fake object graph (outside every firmware region).
FAKE = 0x9A000000
FAKE_SIZE = 0x10000


@dataclass(frozen=True)
class BankProgram:
    """What one call of `0xC022D9F0` did."""

    level: int
    flag: int
    gains: list[tuple[int, int]]  # (setter index 5..8, gain)
    increments: list[tuple[int, int]]  # (DM word, value)


def drawbar_curve(code: bytes) -> list[int]:
    """The nine 7-bit values the record update maps edit value 0..8 to (image bytes)."""
    off = DRAWBAR_CURVE_TABLE - ae.LOAD_BASE
    return list(struct.unpack_from("<9I", code, off))


def curve_db(curve: list[int]) -> list[float | None]:
    """Level of each curve entry in dB relative to the full-scale entry (None for zero)."""
    import math

    top = max(curve)
    return [None if v == 0 else 20 * math.log10(v / top) for v in curve]


def build_fake_graph(emu: ae.ArmEmu, gains: list[tuple[int, int]]) -> int:
    """Fake `this` for `BANK_PROGRAM`; the setter logs `(r3, [sp])` into `gains`."""
    setter = FAKE + 0x8000
    if not getattr(emu, "_bank_fake", False):
        emu._bank_fake = True
        emu.uc.mem_map(FAKE, FAKE_SIZE)
        emu._bank_gains = gains
        # the stub is installed once; it logs into whatever list `_bank_gains` currently holds
        emu.add_stub(setter, lambda e: e._bank_gains.append((e.reg(3), e.read32(e.reg(13)))))
    emu._bank_gains = gains
    this, bank, vtb, outer, mid, leaf, vto = (FAKE + 0x1000 * i for i in range(1, 8))
    emu.write32(vtb + 0x18, setter)
    emu.write32(bank, vtb)
    emu.write32(vto + 0x10, INLINE_ELEMENT_WRITER)
    emu.write32(leaf, vto)
    emu.write32(mid + 8, leaf)
    emu.write32(outer + 8, mid)
    emu.write32(this + 0x10, bank)
    emu.write32(this + 0xC, outer)
    emu.write32(this + 0x18, 0)
    return this


def program_bank(emu: ae.ArmEmu, level: int, flag: int = 0) -> BankProgram:
    """Call the real routine with vibrato `level` and the byte flag at `this + 0x4E`."""
    gains: list[tuple[int, int]] = []
    this = build_fake_graph(emu, gains)
    emu.write_bytes(this + 0x4E, bytes([flag]))
    emu.clear_messages()
    emu.call(BANK_PROGRAM, this, level, max_insns=200_000)
    incs = [(m.dm_word, m.word2) for m in emu.messages if m.core == 1]
    return BankProgram(level, flag, gains, incs)


def bank_script(prog: BankProgram, call: int = 0) -> str:
    """`sharc-bank --script` lines `CALL ADDR VALUE` for the increments of one program."""
    lines = [f"{call} {addr:#x} {value:#010x}" for addr, value in prog.increments]
    return "\n".join(lines) + ("\n" if lines else "")


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path)
    ap.add_argument("--level", type=int, default=1)
    ap.add_argument("--flag", type=int, default=0)
    ap.add_argument("--script", type=Path)
    ns = ap.parse_args(argv)
    emu = ae.load_firmware(ns.image, ns.sram)
    ae.run_static_init(emu)
    emu.guard_null_calls()
    prog = program_bank(emu, ns.level, ns.flag)
    print(f"gains {[(i, hex(g)) for i, g in prog.gains]}; {len(prog.increments)} increments")
    print("curve", drawbar_curve(ns.image.read_bytes()))
    if ns.script:
        ns.script.write_text(bank_script(prog), encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
