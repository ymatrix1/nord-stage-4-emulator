#!/usr/bin/env python3
"""Kernel-selection rule of the SY voice sender: the real ARM instructions, run for every key.

The ARM voice update function `0xC01E00AC` picks the SY1SY2 interpolation kernel of a voice with
a compare chain on one voice-object field (`[object + 8]`, ASSUMED to be the MIDI key: the pitch
code of the same function subtracts 0x3C = 60 from the same field). Listing (OS v1.66):

    0xC01E1430  ldr r3, [r3, #8]          ; key
    0xC01E1434  cmp r3, #0x5F ; ldrgt r2, =0xC0544D88 ; bgt done     (kernel 5, DM 0xE7000)
    0xC01E1440  cmp r3, #0x53 ; ldrgt r2, =0xC0544D78 ; bgt done     (kernel 4, DM 0xE6400)
    0xC01E144C  cmp r3, #0x47 ; ldrgt r2, =0xC0544D68 ; bgt done     (kernel 3, DM 0xE5800)
    0xC01E1458  cmp r3, #0x3B ; ldrgt r2, =0xC0544D58 ; bgt done     (kernel 2, DM 0xE4C00)
    0xC01E1464  cmp r3, #0x2F ; ldrgt r2, =0xC0544D48 ; ldrle r2, =0xC0544D98
    0xC01E1470  done: ldm r2, {r1, r5}   ; record {target 6 (SY), DM base of the kernel}

The records are 16-byte channel-table entries `{target, base, size, tag}` built by the static
initialiser `0xC00002A4` (`0xC0001BAC` is the store of the last one, `0xE3400`, not a sample-start
path). The voice sender then writes `base` to DM `0x92406 + 0x35 + 0x38 k` (record word `+0x3B`,
sender `0xC01E1798..0xC01E17B4`).

    uv run --with unicorn python tools/arm_sy_kernel_rule.py bank0_CODE.bin bank0_SRAM.bin
    uv run --with unicorn python tools/arm_sy_kernel_rule.py bank0_CODE.bin bank0_SRAM.bin --json
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import arm_emu as ae

# VERIFIED (run, docs/research/synth-sample-kernel-rule.md): first and last chain instruction.
CHAIN_START = 0xC01E1434
CHAIN_END = 0xC01E1470
# VERIFIED: the sender stores the kernel base at DM base 0x92406 + 0x35 (record word 0x3B).
KERNEL_FIELD_INDEX = 0x35
VOICE_BASE = 0x92406
# VERIFIED (sy-dsp-map section 2): kernel k sits at DM 0xE3400 + 0xC00 k.
KERNEL_DM0 = 0xE3400
KERNEL_STRIDE = 0xC00
SY_TARGET = 6


def kernel_index(dm_base: int) -> int:
    """Kernel number `k` of a kernel DM base `0xE3400 + 0xC00 k`; raises for any other address."""
    k, rem = divmod(dm_base - KERNEL_DM0, KERNEL_STRIDE)
    if rem or not 0 <= k < 6:
        raise ValueError(f"{dm_base:#x} is not a kernel base")
    return k


def run_chain(
    emu: ae.ArmEmu, key: int, start: int = CHAIN_START, end: int = CHAIN_END
) -> tuple[int, int, int]:
    """Runs the compare chain with `r3 = key`; returns `(record address, target, base)`."""
    emu.set_reg(3, key)
    emu.set_reg(13, ae.STACK_TOP - 0x1000)
    emu.run(start, end, max_insns=64)
    rec = emu.reg(2)
    return rec, emu.read32(rec), emu.read32(rec + 4)


def key_table(emu: ae.ArmEmu, keys: range = range(128)) -> list[int]:
    """Kernel number for every key (the firmware's own instructions decide)."""
    return [kernel_index(run_chain(emu, k)[2]) for k in keys]


def boundaries(table: list[int]) -> list[tuple[int, int]]:
    """`(first key, kernel)` of every run of equal kernels."""
    out: list[tuple[int, int]] = []
    for key, k in enumerate(table):
        if not out or out[-1][1] != k:
            out.append((key, k))
    return out


def load_static_init(code: Path, sram: Path) -> ae.ArmEmu:
    emu = ae.load_firmware(code, sram)
    ae.run_static_init(emu)
    return emu


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("code", type=Path)
    ap.add_argument("sram", type=Path)
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args(argv)
    emu = load_static_init(args.code, args.sram)
    table = key_table(emu)
    runs = boundaries(table)
    if args.json:
        print(json.dumps({"kernel_by_key": table, "runs": runs}))
    else:
        ends = [r[0] for r in runs[1:]] + [128]
        for (lo, k), nxt in zip(runs, ends, strict=True):
            print(f"keys {lo:3d}..{nxt - 1:3d}  kernel {k}  DM {KERNEL_DM0 + KERNEL_STRIDE * k:#x}")
    return 0


if __name__ == "__main__":  # pragma: no cover
    sys.exit(main())
