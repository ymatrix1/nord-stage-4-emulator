#!/usr/bin/env python3
# ruff: noqa: E501
"""Start state and sine-bank data of the Vox post stage `0x12272D`, read from the PNO image.

Writes the derived values of `crates/nord-core/src/organ/vox_image.rs` (84 note start phases, the
LFO start phase, the 36-sine bank: ladder, gains, start phases, polynomial, bucket map) and checks
the closed forms that the Rust code uses instead of tables.

    python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DSP
    python tools/ns4_dsp_ldr.py DSP/dsp_image_1_PNO.bin --sram SRAM
    python tools/organ_vox_image.py check SRAM/core1_sram.bin --exe target/release/sharc-chain
    python tools/organ_vox_image.py rust SRAM/core1_sram.bin --exe target/release/sharc-chain \
        > crates/nord-core/src/organ/vox_image.rs

Result of `check` (docs/research/organ-vox-start-state.md): the sine ladder is
`0x1AE89F99 2^(j/12)` for j < 24 (max error 1 count) and the gains and start phases are constant per
octave group; the 84 note start phases are not a closed form and stay a table.
"""

from __future__ import annotations

import argparse
import struct
import subprocess
import sys

STATE, SINE_PH, LADDER, GAINS, BASE_IN = 0xE2B14, 0xE2B98, 0xE2C8E, 0xE2BBC, 0xE2BE0
CONSTS, BUCKET_PAIRS, LFO = 0xE2CB4, 0x95E22, 0xE21F0
NOTES, SINES = 84, 36
LADDER_BASE = 0x1AE89F99


def dump(exe: str, image: str, addr: int, n: int) -> list[int]:
    out = subprocess.run(
        [exe, image, "--calls", "1", "--dump", f"{addr:#x}:{n}"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    line = next(ln for ln in out.splitlines() if ln.startswith("DM("))
    return [int(w, 16) for w in line.split(":", 1)[1].split()]


def f32(word: int) -> float:
    return struct.unpack("<f", struct.pack("<I", word))[0]


def ladder_ratio(words: list[int]) -> list[float]:
    """Image ladder over the closed form `LADDER_BASE 2^(j/12)` (1.0 where the form holds)."""
    return [w / (LADDER_BASE * 2 ** (j / 12)) for j, w in enumerate(words)]


def load(exe: str, image: str) -> dict[str, list[int]]:
    return {
        "phases": dump(exe, image, STATE, NOTES),
        "lfo": dump(exe, image, LFO, 2),
        "sine_ph": dump(exe, image, SINE_PH, SINES),
        "ladder": dump(exe, image, LADDER, SINES),
        "gains": dump(exe, image, GAINS, SINES),
        "consts": dump(exe, image, CONSTS, 12),
        "base": dump(exe, image, BASE_IN, 2),
        "pairs": dump(exe, image, BUCKET_PAIRS, 36),
    }


def bucket_of(j: int) -> int:
    """Bucket (0..11) of sine `j`: `BI[j >> 1] + (j & 1)` with the image pair table."""
    return [0, 2, 4, 6, 8, 10, 0, 2, 4, 6, 8, 10, 0, 2, 4, 6, 0, 2][j >> 1] + (j & 1)


def rust_file(d: dict[str, list[int]]) -> str:
    def u32s(name: str, doc: str, ws: list[int]) -> list[str]:
        rows = [
            f"    {', '.join(f'0x{w:08X}' for w in ws[i : i + 6])}," for i in range(0, len(ws), 6)
        ]
        return [
            doc,
            "#[rustfmt::skip]",
            f"pub(super) const {name}: [u32; {len(ws)}] = [",
            *rows,
            "];",
            "",
        ]

    lines = [
        "//! Start state and sine-bank data of the Vox post stage, derived from the PNO image.",
        "//!",
        "//! Regenerate with `python tools/organ_vox_image.py rust SRAM/core1_sram.bin --exe ...`;",
        "//! method and checks in `docs/research/organ-vox-start-state.md`. 84 start words, three",
        "//! sine phase groups, the 36 ladder words and the polynomial constants: derived values,",
        "//! no code or sample data.",
        "",
        "/// VERIFIED: LFO start phase `DM(0xE21F0)` of the image (both lanes), `|s| = 1`: the triangle",
        "/// starts at its top. No ARM message and no DSP instruction other than the per frame",
        "/// `+= 0x905A3` writes it (`tools/organ_vox_image.py check`).",
        f"pub(super) const LFO_START: u32 = 0x{d['lfo'][0]:08X};",
        "",
        *u32s(
            "NOTE_START",
            "/// VERIFIED: note phases `DM(0xE2B14 + k)` of the image (the state the oracle chord starts\n"
            "/// from); the oscillators free-run and nothing resets them at key-down, so on hardware the\n"
            "/// relative phases of a chord are those of the free-running bank at that instant.",
            d["phases"],
        ),
        *u32s(
            "SINE_LADDER",
            "/// VERIFIED: sine phase steps `DM(0xE2C8E + j)` (fraction of 2^31); `0x1AE89F99 2^(j/12)` for\n"
            "/// j < 24 (max 1 count), folded down by 0.75 (j 24..31) and 0.63 (j 32..35) after that.",
            d["ladder"],
        ),
        "/// VERIFIED: sine start phases `DM(0xE2B98 + j)` per octave group (j 0..23, 24..31, 32..35).",
        "pub(super) const SINE_START: [u32; 3] = [",
        f"    0x{d['sine_ph'][0]:08X}, 0x{d['sine_ph'][24]:08X}, 0x{d['sine_ph'][32]:08X},",
        "];",
        "",
        "/// VERIFIED: sine gains `DM(0xE2BBC + j)` as f32 words per group (j 0..11, 12..23, 24..31, 32..35).",
        "pub(super) const SINE_GAIN_WORDS: [u32; 4] = [",
        f"    0x{d['gains'][0]:08X}, 0x{d['gains'][12]:08X}, 0x{d['gains'][24]:08X}, 0x{d['gains'][32]:08X},",
        "];",
        "",
        "/// VERIFIED: sine base word `DM(0xE2BE0)` (fraction of 2^31, 0.917).",
        f"pub(super) const SINE_BASE: u32 = 0x{d['base'][0]:08X};",
        "",
        "/// VERIFIED: sine polynomial words `DM(0xE2CB8..)`: c7, c5, c3, c1 (`c1 = pi / 2`).",
        "pub(super) const SINE_POLY_WORDS: [u32; 4] = [",
        f"    0x{d['consts'][4]:08X}, 0x{d['consts'][6]:08X}, 0x{d['consts'][8]:08X}, 0x{d['consts'][10]:08X},",
        "];",
        "",
        "/// VERIFIED: bucket of each sine, `DM(0x95E22 + 2 (j >> 1)) + (j & 1)`.",
        "pub(super) const SINE_BUCKET: [u8; 36] = [",
        "    " + ", ".join(str(bucket_of(j)) for j in range(36)) + ",",
        "];",
        "",
        "#[test]",
        "fn start_state_is_the_image_state() {",
        "    assert!(NOTE_START.iter().all(|&w| w != 0));",
        "    assert_eq!(LFO_START, 0x8000_0000);",
        "    assert_eq!(SINE_BUCKET.iter().filter(|&&b| b == 0).count(), 4);",
        "}",
    ]
    return "\n".join(lines) + "\n"


def cmd_check(d: dict[str, list[int]]) -> int:
    r = ladder_ratio(d["ladder"])
    print("ladder / closed form, j < 24: max error", max(abs(x - 1) for x in r[:24]))
    print(
        "j 24..31:",
        sorted({round(x, 4) for x in r[24:32]}),
        "j 32..35:",
        [round(x, 4) for x in r[32:]],
    )
    print("sine phases per group:", {hex(w) for w in d["sine_ph"]})
    print("gains:", sorted({round(f32(w), 5) for w in d["gains"]}))
    print(
        "poly:",
        [f32(d["consts"][i]) for i in (4, 6, 8, 10)],
        "floor scale",
        d["consts"][0],
        "offset",
        f32(d["consts"][2]),
    )
    print("lfo start:", [hex(w) for w in d["lfo"]], "base:", [hex(w) for w in d["base"]])
    print("bucket map:", [bucket_of(j) for j in range(36)])
    nz = sum(1 for w in d["phases"] if w)
    print("note phases non-zero:", nz, "of", NOTES)
    return 0


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("cmd", choices=("check", "rust"))
    ap.add_argument("image")
    ap.add_argument("--exe", required=True)
    ns = ap.parse_args(argv)
    d = load(ns.exe, ns.image)
    if ns.cmd == "rust":
        sys.stdout.write(rust_file(d))
        return 0
    return cmd_check(d)


if __name__ == "__main__":
    sys.exit(main())
