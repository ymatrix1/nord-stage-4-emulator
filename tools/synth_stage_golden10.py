#!/usr/bin/env python3
"""Golden vectors of the twelfth stage batch for the Rust port (`synth/fw_super2.rs`).

    python tools/synth_stage_golden10.py > crates/nord-core/src/synth/tests_fw_stage_golden10.rs

Derived values of the bit-exact model `tools/synth_stage_super2.py` (320 of 320 oracle words,
`tools/synth_wave_oracle.py --stage 0x124985`): the octave band index of 40 pitch words, the FIR
output of 48 six-word buffers (zero-extended as the oracle loads, and sign-extended) and the FNV-1a
fingerprints of the four tables. Only derived values appear.
"""

from __future__ import annotations

import random
import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import synth_stage_golden as g
import synth_stage_super2 as s2
import synth_super24_tables as t24
import synth_wave_model as m

FNV_OFFSET = 0xCBF29CE484222325
FNV_PRIME = 0x100000001B3


def fnv1a(words: list[int]) -> int:
    """FNV-1a 64 over the words, little endian."""
    h = FNV_OFFSET
    for w in words:
        for b in w.to_bytes(4, "little"):
            h = ((h ^ b) * FNV_PRIME) & 0xFFFFFFFFFFFFFFFF
    return h


def pitch_rows() -> list[tuple[int, int]]:
    """`(pitch word bits, band index)`: 40 pitches from 2^-12 to 0.6 (the stage clips at 0.5)."""
    rows = []
    for k in range(40):
        f = m.f2u(0.6 * 2 ** (-12.0 * k / 39))
        rows.append((f, s2.row_index(m.clip(m.ld(f), s2.PITCH_LIMIT))))
    return rows


def fir_rows(signed: bool) -> list[tuple[int, int]]:
    """`(case id, FIR sum)` for 48 seeded buffers: `rel` 0..4, phase 0..511."""
    tab = s2.load_tables()
    rng = random.Random(24)
    out = []
    for n in range(48):
        words = [rng.getrandbits(32) for _ in range(6)]
        rel, phase = n % 5, rng.randrange(512)
        out.append((n, s2.fir_lane(words, rel, 8 * phase, tab.fir, signed)))
    return out


def fir_inputs() -> list[tuple[list[int], int, int]]:
    rng = random.Random(24)
    rows = []
    for n in range(48):
        words = [rng.getrandbits(32) for _ in range(6)]
        rows.append((words, n % 5, rng.randrange(512)))
    return rows


def rust() -> str:
    tab = t24.load()
    ins = fir_inputs()
    pitches = pitch_rows()
    parts = [
        "//! Firmware parity of the twelfth stage batch (`fw_super2.rs`).\n"
        "//!\n"
        "//! Derived values only, made by `python tools/synth_stage_golden10.py > <file>`\n"
        "//! from the bit-exact model (`docs/research/synth-stage-models-12.md`).\n",
        g.words_rust("PITCH", [p[0] for p in pitches]),
        g.words_rust("BAND", [p[1] for p in pitches]),
        g.words_rust("FIR_WORDS", [w for row in ins for w in row[0]]),
        g.words_rust("FIR_REL", [r[1] for r in ins]),
        g.words_rust("FIR_PHASE", [r[2] for r in ins]),
        g.words_rust("FIR_ZERO", [v & 0xFFFFFFFF for _, v in fir_rows(False)]),
        g.words_rust("FIR_SIGNED", [v & 0xFFFFFFFF for _, v in fir_rows(True)]),
    ]
    for name in ("rows", "wave", "fir", "lattice"):
        parts.append(f"pub(super) const FNV_{name.upper()}: u64 = {fnv1a(tab[name]):#018x};\n")
    tail = (
        "\n#[test]\nfn golden_tables_have_the_documented_shape() {\n"
        "    assert_eq!(PITCH.len(), BAND.len());\n"
        "    assert_eq!(FIR_WORDS.len(), 6 * FIR_REL.len());\n"
        "    assert!(FIR_ZERO != FIR_SIGNED);\n}\n"
    )
    return "\n".join(parts) + tail


if __name__ == "__main__":
    sys.stdout.write(rust())
