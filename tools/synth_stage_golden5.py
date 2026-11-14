#!/usr/bin/env python3
"""Golden grids of the sixth batch of stages for the Rust ports (`synth/fw_super1.rs`, ...).

    python tools/synth_stage_golden5.py > crates/nord-core/src/synth/tests_fw_stage_golden5.rs

Derived values of the bit-exact models (checked against the SHARC oracle by
`tools/synth_wave_oracle.py`): the SUPER wave 1 inline square and one SHAPE SINE wave 3 oscillator. Only derived values appear.
"""

from __future__ import annotations

import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import synth_stage_golden as g
import synth_stage_shape_sine3 as sine3
import synth_stage_super1 as super1
import synth_wave_model as m


def fold_rows() -> list[tuple[int, int, int]]:
    """`(phase bits, increment bits, sample bits)` of the SUPER 1 inline square."""
    rows = []
    for dt in g.INCREMENTS:
        dt32 = g.f32(dt)
        inc = int(dt32 * 2**32)
        r7 = m.recip_refined(m.flt(inc, -31), m.HALF)
        for k in range(16):
            ph = g.phase_word(k)
            y = super1.fold_diff([ph], r7)[0]
            rows.append((m.f2u(ph / 2**32), m.f2u(dt32), m.f2u(float(y))))
    # edge points: phases just either side of |x| = 0.5 (the bump of the smoothed edge)
    for dt in g.INCREMENTS:
        dt32 = g.f32(dt)
        inc = int(dt32 * 2**32)
        r7 = m.recip_refined(m.flt(inc, -31), m.HALF)
        for off in (-0.75, -0.25, 0.25, 0.75):
            ph = (int((0.25 + off * dt32) * 2**32) & 0xFFFFFF00) & 0xFFFFFFFF
            y = super1.fold_diff([ph], r7)[0]
            rows.append((m.f2u(ph / 2**32), m.f2u(dt32), m.f2u(float(y))))
    return rows


def sine3_rows() -> list[tuple[int, int, int, int]]:
    """`(phase bits, increment bits, shape bits, sample bits)` of one SHAPE SINE 3 oscillator
    (own weight 1, gain 1, spread word = shape): 2 starts x 3 shapes x 4 increments x 4 samples."""
    one, zero = m.f2u(1.0), m.f2u(0.0)
    rows = []
    for dt in g.INCREMENTS:
        for shape in (0.25, 0.5, 0.9):
            for s in range(g.STARTS // 4):
                ph0 = g.phase_word(s + 3)
                lane = m.Lane(
                    pitch=m.f2u(2.0 * g.f32(dt)), phase1=ph0, phase2=0,
                    shape_a=zero, shape_b=zero, ratio1=one, ratio2=one,
                    weights=(zero, one, zero, zero), out_gain=one, spread=m.f2u(shape),
                )  # fmt: skip
                out, _, _ = sine3.stage_shape_sine_3([lane, lane])
                inc = m.fix(m.mul(m.ld(lane.pitch), m.ld(lane.ratio1)), 31)
                for k in range(1, 5):
                    ph = ((ph0 + k * inc) & 0xFFFFFFFF) / 2**32
                    rows.append((m.f2u(ph), m.f2u(inc / 2**32), m.f2u(shape), out[0][k - 1]))
    return rows


def rust() -> str:
    parts = [
        "//! Firmware parity of the sixth stage batch (`fw_super1.rs`, ...).\n"
        "//!\n"
        "//! Derived values only, made by `python tools/synth_stage_golden5.py > <file>` from the\n"
        "//! bit-exact models (`docs/research/synth-stage-models-5.md`).\n"
    ]
    fold = fold_rows()
    for i, name in enumerate(("PHASE", "DT", "OUT")):
        parts.append(g.words_rust(f"FOLD_{name}", [r[i] for r in fold]))
    sine = sine3_rows()
    for i, name in enumerate(("PHASE", "DT", "SHAPE", "OUT")):
        parts.append(g.words_rust(f"SS3_{name}", [r[i] for r in sine]))
    tail = (
        "\n#[test]\nfn golden_tables_have_the_documented_shape() {\n"
        f"    assert_eq!(FOLD_PHASE.len(), {len(fold)});\n"
        "    assert!(FOLD_OUT.iter().all(|&w| f32::from_bits(w).is_finite()));\n"
        f"    assert_eq!(SS3_PHASE.len(), {len(sine)});\n"
        "    assert!(SS3_OUT.iter().all(|&w| f32::from_bits(w).is_finite()));\n}\n"
    )
    return "\n".join(parts) + tail


if __name__ == "__main__":
    sys.stdout.write(rust())
