#!/usr/bin/env python3
"""Resonance word to stage table position in the ORG core 2 pass (SHARC oracle probe).

**Superseded (D121):** this probe reads `W28` after 60 passes, which is the smoother transient
(`128 (1 - 0.995^60) = 33.25`), not the settled value `128 knob / 127`; use
`tools/synth_filter_links.py res`. Kept as the record of the 60 pass reading.

The ARM writes `kFltRes` as the float word `-0.5 + knob / 127` into the layer common block word 7
(`docs/research/synth-knob-laws.md` s3). This tool writes it for channel 0 (with a voice word and
a cutoff, as `tools/synth_cutoff_note_probe.py`), runs 60 passes and reads the stage position word
`W28 = DM(0xC2926)`: byte 3 is the table index, the low 24 bits its fraction. The result is
`idx = 47.37672 + 0.261784 knob = 64 + 33.25 w` (`docs/research/synth-filter-type-corners.md`).

    cargo build --release -p nord-sharc
    uv run python tools/ns4_dsp_ldr.py target/DSP/dsp_image_2_ORG.bin --sram DIR
    NS4_ORG_IMAGE=DIR/core2_sram.bin python tools/synth_filter_res_probe.py
"""

from __future__ import annotations

import struct
import sys

import synth_adr_oracle as adr
from synth_cutoff_note_probe import knob_word, voice_word

PASSES = 60
W28_ADDRESS = 0xC2926
COMMON_RES_WORD = 0x2E9 + 6  # common block word 7 (cutoff is word 1 at 0x2E9)


def res_word(knob: int) -> int:
    """The ARM word of `kFltRes` for a knob position: float32 `-0.5 + knob / 127`."""
    return struct.unpack("<I", struct.pack("<f", -0.5 + knob / 127.0))[0]


def position(word: int) -> float:
    """Table position `idx + fraction` of a `W28` word."""
    return (word >> 24) + (word & 0xFFFFFF) / float(1 << 24)


def model_position(knob: int) -> float:
    """`64 + 33.25 w` with `w = knob / 127 - 0.5` (the fitted law)."""
    return 64.0 + 33.25 * (knob / 127.0 - 0.5)


def probe(org: str, image: str, knob: int) -> float:
    """Table position the pass leaves in `W28` for the resonance knob."""
    msgs = [(0, 3, knob_word(80)), (0, 0x2E9, knob_word(80)), (0, COMMON_RES_WORD, res_word(knob))]
    rows = adr.run_script(
        org,
        image,
        PASSES,
        msgs=msgs,
        pre={adr.CONTROLLER: voice_word(60)},
        bufs=[f"{W28_ADDRESS:#x}:1:f32"],
    )
    return position(rows[(PASSES - 1, 0)][0])


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - needs the firmware image
    org, image = adr.default_paths()
    print("knob   idx_oracle   idx_model")
    for knob in (0, 16, 32, 48, 64, 80, 96, 112, 127):
        print(f"{knob:4d} {probe(org, image, knob):12.4f} {model_position(knob):11.4f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
