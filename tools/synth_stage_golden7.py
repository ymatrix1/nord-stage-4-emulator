#!/usr/bin/env python3
"""Golden grid of the eighth batch of stages for the Rust ports (`synth/fw_sync6.rs`).

    python tools/synth_stage_golden7.py > crates/nord-core/src/synth/tests_fw_stage_golden7.rs

Derived values of the bit-exact models `tools/synth_stage_sync1.py`, `synth_stage_sync6.py` and
`synth_stage_sync7.py` (checked against the SHARC oracle by `tools/synth_wave_oracle.py --stage
0x1226DD` / `0x1239BE` / `0x123C7C` / `0x123FE9`): one SYNC 1, 6, 7 or 8 oscillator (SYNC 1 before
the output high pass). Only derived values appear.
"""

from __future__ import annotations

import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import synth_stage_golden as g
import synth_stage_sync1 as sync1
import synth_stage_sync6 as sync6
import synth_stage_sync7 as sync7
import synth_wave_model as m


def sync_rows(stage) -> list[tuple[int, int, int, int]]:
    """`(phase bits, increment bits, shape bits, sample bits)` of one oscillator (own weight 1,
    gain 1): 5 shapes x 4 increments x 2 starts x 4 samples (shape = spread word)."""
    one, zero = m.f2u(1.0), m.f2u(0.0)
    rows = []
    for dt in g.INCREMENTS:
        for shape in (0.1, 0.25, 0.5, 0.7, 0.9):
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
        "//! Firmware parity of the eighth stage batch (`fw_sync1.rs`, `fw_sync6.rs`).\n"
        "//!\n"
        "//! Derived values only, made by `python tools/synth_stage_golden7.py > <file>` from the\n"
        "//! bit-exact models (`docs/research/synth-stage-models-7.md`).\n"
    ]
    n = 0
    stages = (
        ("SY1", lambda lanes: sync1.stage_sync1(lanes, high_pass=False)),
        ("SY6", sync6.stage_sync6),
        ("SY7", sync7.stage_sync7),
        ("SY8", sync6.stage_sync8),
    )
    for tag, stage in stages:
        rows = sync_rows(stage)
        n = len(rows)
        for i, name in enumerate(("PHASE", "DT", "SHAPE", "OUT")):
            parts.append(g.words_rust(f"{tag}_{name}", [r[i] for r in rows]))
    tail = (
        "\n#[test]\nfn golden_tables_have_the_documented_shape() {\n"
        f"    assert_eq!(SY6_PHASE.len(), {n});\n"
        f"    assert_eq!(SY8_OUT.len(), {n});\n"
        "    assert!(SY1_OUT.iter().all(|&w| f32::from_bits(w).is_finite()));\n"
        "    assert!(SY7_OUT.iter().all(|&w| f32::from_bits(w).is_finite()));\n"
        "    assert!(SY6_OUT.iter().all(|&w| f32::from_bits(w).is_finite()));\n"
        "    assert!(SY8_OUT.iter().all(|&w| f32::from_bits(w).is_finite()));\n}\n"
    )
    return "\n".join(parts) + tail


if __name__ == "__main__":
    sys.stdout.write(rust())
