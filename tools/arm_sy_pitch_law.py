#!/usr/bin/env python3
"""Pitch law of the SY sample voice sender: the real ARM instructions, run for every key.

The voice update function `0xC01E00AC` turns the voice object (`r6`) into the pitch word of the
voice pair block. Listing (OS v1.66, integer code; the ARM926 has no FPU, the two library calls
are the soft-float helpers `0xC0270BD4` int-to-float and `0xC0270C80` multiply):

    0xC01E14AC  r0 = [r6 + 0x34]                      ; layer object (zero: block skipped)
    0xC01E14B8  cents = (([r6 + 0x14] + [r6 + 0x1C]) << 16) * 0x51EB851F >> 37   ; x / 100
    0xC01E14E4  key   = [r6 + 8] + [r6 + 0x18]  if [r0 + 0x6C] == 1 else [r0 + 0x60]
    0xC01E14FC  x = ((key - 60) << 16) - ([r0 + 0x10] << 8) - (cents + [r6 + 0x74])
    0xC01E150C  m = T1[(x >> 8) & 255] interpolated to (x & 255) / 256, tables 0xC02FA340 ..
                T2[clip((x >> 16) + 128, 0, 255)] (hi, lo 16-bit mantissa), T3[..] = shift
    0xC01E1598  r0 = float(m shifted by T3) * 2^-23 (0x34000000)     ; written to the pair block

    uv run --with unicorn python tools/arm_sy_pitch_law.py bank0_CODE.bin bank0_SRAM.bin
    uv run --with unicorn python tools/arm_sy_pitch_law.py bank0_CODE.bin bank0_SRAM.bin --json

Findings and evidence: `docs/research/synth-sample-kernel-rule.md` section 6.
"""

from __future__ import annotations

import argparse
import json
import math
import struct
import sys
from pathlib import Path

import arm_emu as ae

# VERIFIED (listing, docs/research/synth-sample-kernel-rule.md): first instruction after the
# `[r6 + 0x34]` test and the instruction after the soft-float multiply that leaves the result in r0.
BLOCK_START = 0xC01E14B8
BLOCK_END = 0xC01E15A4
# VERIFIED (listing): field offsets of the voice object `r6` and of the layer object `r0`.
OBJ_KEY, OBJ_TRANSPOSE, OBJ_CENTS_A, OBJ_CENTS_B, OBJ_FINE, OBJ_LAYER = (
    0x08,
    0x18,
    0x14,
    0x1C,
    0x74,
    0x34,
)
LAYER_MODE, LAYER_FIXED_KEY, LAYER_TUNE = 0x6C, 0x60, 0x10
# Scratch objects live in the stack window of the harness (mapped, not firmware data).
OBJ_BASE = ae.STACK_TOP - 0x8000
LAYER_BASE = ae.STACK_TOP - 0x7000


def float_from_bits(bits: int) -> float:
    return struct.unpack("<f", struct.pack("<I", bits & 0xFFFFFFFF))[0]


def run_block(
    emu: ae.ArmEmu,
    key: int,
    *,
    transpose: int = 0,
    cents: int = 0,
    fine: int = 0,
    tune: int = 0,
    fixed_key: int | None = None,
) -> int:
    """Run the pitch instructions for a voice object; returns the float bits left in `r0`.

    `cents` is the sum `[r6 + 0x14] + [r6 + 0x1C]` (hundredths of a semitone: the block divides by
    100 after the 16-bit shift), `fine` is `[r6 + 0x74]` in the same Q16 semitone unit as the key,
    `tune` is `[layer + 0x10]` (shifted left by 8), `fixed_key` replaces `key + transpose` when
    given (`[layer + 0x6C] != 1`).
    """
    emu.write32(OBJ_BASE + OBJ_KEY, key)
    emu.write32(OBJ_BASE + OBJ_TRANSPOSE, transpose)
    emu.write32(OBJ_BASE + OBJ_CENTS_A, cents)
    emu.write32(OBJ_BASE + OBJ_CENTS_B, 0)
    emu.write32(OBJ_BASE + OBJ_FINE, fine)
    emu.write32(OBJ_BASE + OBJ_LAYER, LAYER_BASE)
    emu.write32(LAYER_BASE + LAYER_MODE, 1 if fixed_key is None else 0)
    emu.write32(LAYER_BASE + LAYER_FIXED_KEY, 0 if fixed_key is None else fixed_key)
    emu.write32(LAYER_BASE + LAYER_TUNE, tune)
    emu.set_reg(6, OBJ_BASE)
    emu.set_reg(0, LAYER_BASE)
    emu.set_reg(13, ae.STACK_TOP - 0x1000)
    emu.run(BLOCK_START, BLOCK_END, max_insns=4000)
    return emu.reg(0)


def ratio_table(emu: ae.ArmEmu, keys: range = range(128)) -> list[float]:
    """Pair-block float of every key (no transposition, no fine tune), the firmware's own code."""
    return [float_from_bits(run_block(emu, k)) for k in keys]


def load_static_init(code: Path, sram: Path) -> ae.ArmEmu:
    emu = ae.load_firmware(code, sram)
    ae.run_static_init(emu)
    return emu


def semitone_error(values: list[float], keys: range = range(128)) -> float:
    """Largest deviation (cents) of `log2(values[i] / values[60])` from `(key - 60) / 12`."""
    base = values[60]
    return max(
        abs(1200.0 * (math.log2(v / base) - (k - 60) / 12.0))
        for k, v in zip(keys, values, strict=True)
    )


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("code", type=Path)
    ap.add_argument("sram", type=Path)
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args(argv)
    emu = load_static_init(args.code, args.sram)
    values = ratio_table(emu)
    if args.json:
        print(json.dumps({"float_by_key": values}))
    else:
        for k in (0, 12, 24, 36, 48, 59, 60, 61, 72, 84, 96, 108, 120, 127):
            print(f"key {k:3d}  {values[k]!r:>24}  bits {struct.pack('<f', values[k]).hex()}")
        print(f"max |cents| off the 12-TET law: {semitone_error(values):.4f}")
    return 0


if __name__ == "__main__":  # pragma: no cover
    sys.exit(main())
