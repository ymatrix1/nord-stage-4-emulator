#!/usr/bin/env python3
"""Golden grids of the eleventh batch of stages for the Rust ports (`synth/fw_shape34.rs`).

    python tools/synth_stage_golden9.py > crates/nord-core/src/synth/tests_fw_stage_golden9.rs

Derived values of the bit-exact models `tools/synth_stage_shape3.py` and
`tools/synth_stage_shape4.py` (checked against the SHARC oracle by
`tools/synth_wave_oracle.py --stage 0x120F55 / 0x12145E`): one SHAPE 3 or SHAPE 4 oscillator
(own weight 1, gain 1). Only derived values appear.
"""

from __future__ import annotations

import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import synth_stage_golden as g
import synth_stage_shape3 as shape3
import synth_stage_shape4 as shape4
import synth_wave_model as m

SHAPES = (0.1, 0.25, 0.5, 0.7, 0.9)


def shape_rows(stage) -> list[tuple[int, int, int, int]]:
    """`(phase bits, increment bits, shape bits, sample bits)` of one oscillator: 5 shapes x 4
    increments x 2 starts x 4 samples (shape = the spread word `DM(-5, I2)`)."""
    one, zero = m.f2u(1.0), m.f2u(0.0)
    rows = []
    for dt in g.INCREMENTS:
        for shape in SHAPES:
            for s in range(g.STARTS // 4):
                ph0 = g.phase_word(s + 3)
                lane = m.Lane(
                    pitch=m.f2u(2.0 * g.f32(dt)), phase1=ph0, phase2=0,
                    shape_a=zero, shape_b=zero, ratio1=one, ratio2=one,
                    weights=(zero, one, zero, zero), out_gain=one, spread=m.f2u(shape),
                )  # fmt: skip
                out, _, _ = stage([lane, lane])
                inc = m.fix(m.mul(m.ld(lane.pitch), m.ld(lane.ratio1)), 31)
                for k in range(1, 5):
                    ph = ((ph0 + k * inc) & 0xFFFFFFFF) / 2**32
                    rows.append((m.f2u(ph), m.f2u(inc / 2**32), m.f2u(shape), out[0][k - 1]))
    return rows


def rust() -> str:
    parts = [
        "//! Firmware parity of the eleventh stage batch (`fw_shape34.rs`).\n"
        "//!\n"
        "//! Derived values only, made by `python tools/synth_stage_golden9.py > <file>` from the\n"
        "//! bit-exact models (`docs/research/synth-stage-models-11.md`).\n"
    ]
    n = 0
    for tag, stage in (("SH3", shape3.stage_shape3), ("SH4", shape4.stage_shape4)):
        rows = shape_rows(stage)
        n = len(rows)
        for i, name in enumerate(("PHASE", "DT", "SHAPE", "OUT")):
            parts.append(g.words_rust(f"{tag}_{name}", [r[i] for r in rows]))
    tail = (
        "\n#[test]\nfn golden_tables_have_the_documented_shape() {\n"
        f"    assert_eq!(SH3_PHASE.len(), {n});\n"
        f"    assert_eq!(SH4_OUT.len(), {n});\n"
        "    assert!(SH3_OUT.iter().all(|&w| f32::from_bits(w).is_finite()));\n"
        "    assert!(SH4_OUT.iter().all(|&w| f32::from_bits(w).is_finite()));\n}\n"
    )
    return "\n".join(parts) + tail


if __name__ == "__main__":
    sys.stdout.write(rust())
