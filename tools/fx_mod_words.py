#!/usr/bin/env python3
"""Instance words of the Mod 1 routines in the FX image (`crates/nord-core/src/fx/fw_mod1.rs`).

The static DM of the FX image (core 1 flat SRAM from `tools/ns4_dsp_ldr.py --sram`) carries the
context words that neither the ARM type switch nor `kFx1Amt` / `kFx1RateClkDiv` write: the sine
polynomial, the integer-to-float shift, the smoothing pole, the pre / post gains and the one-pole
sections. This tool dumps the two instances of the synth unit L0 and checks them against the
constants the Rust port uses.

    python tools/fx_mod_words.py SRAM/core1_sram.bin

Row bases (`I1`): A-PAN `0xC1528` (`0xC1508 + 0x20`, `docs/research/data/fx-type-words.csv`),
TREM `0xC15BE`. The block count words `c[-3]` (A-PAN) and `c[4]` (TREM) are `2` in the boot log
of `tools/arm_bulk_tables.py capture`, not in the image.
"""

from __future__ import annotations

import struct
import sys

PAN_BASE, TREM_BASE = 0xC1528, 0xC15BE

#: (row base, offset from I1) -> expected word. VERIFIED by the dump of the OS v1.66 image.
EXPECTED = {
    "pan": (
        PAN_BASE,
        {
            -0x19: 0x40000000,
            -0x18: 0x3E800000,
            -0x17: 0x3F800000,
            -0xB: 0x3FC90069,
            -0xA: 0xBF245655,
            -9: 0x3D92AA8A,
            -8: 0xFFFFFFE1,
            -7: 0x3F7DA3C2,
            -6: 0x40800000,
            -5: 0xBF800000,
            -4: 0x3F000000,
        },
    ),  # fmt: skip
    "trem": (
        TREM_BASE,
        {
            -0x13: 0x3FC90069,
            -0x12: 0xBF245655,
            -0x11: 0x3D92AA8A,
            -0x14: 0x3F000000,
            -0x10: 0x3DA40D0A,
            -0xF: 0x3DA40D0A,
            -0xD: 0x3F56FCBD,
            -7: 0x3F6B7E5E,
            -6: 0xBF6B7E5E,
            -4: 0x3F56FCBD,
            3: 0x3F7DA3C2,
        },
    ),  # fmt: skip
}


def word(img: bytes, addr: int) -> int:
    """The DM word `addr` of a core 1 flat SRAM image (`4 * addr` bytes in)."""
    return struct.unpack_from("<I", img, 4 * addr)[0]


def check(img: bytes) -> list[str]:
    """The expected words that differ from the image, as readable lines (empty when all match)."""
    bad = []
    for name, (base, words) in EXPECTED.items():
        for off, want in sorted(words.items()):
            got = word(img, base + off)
            if got != want:
                bad.append(
                    f"{name} c[{off}] @{base + off:#x}: image {got:#010x}, expected {want:#010x}"
                )
    return bad


def main(argv: list[str] | None = None) -> int:
    args = sys.argv[1:] if argv is None else argv
    if len(args) != 1:
        print(__doc__)
        return 2
    img = open(args[0], "rb").read()
    bad = check(img)
    for line in bad:
        print("MISMATCH", line)
    total = sum(len(w) for _, w in EXPECTED.values())
    print(f"{total - len(bad)}/{total} instance words match the image")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
