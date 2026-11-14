#!/usr/bin/env python3
"""Golden grid of SHAPE SINE wave 4 (`0x120DAB`) for the Rust port (`synth/fw_sine_hp.rs`).

    python tools/synth_stage_golden4.py > crates/nord-core/src/synth/tests_fw_stage_golden4.rs

One oscillator of the bit-exact model (own weight 1, gain 1, output high pass off), spread word
= shape: 2 starts x 3 shapes x 4 increments x 4 samples. Only derived values appear.
"""

from __future__ import annotations

import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import synth_stage_golden as g
import synth_stage_shape_sine2 as sine2
import synth_wave_model as m


def rows() -> list[tuple[int, int, int, int]]:
    """`(phase bits, increment bits, shape bits, sample bits)` before the output high pass."""
    one, zero = m.f2u(1.0), m.f2u(0.0)
    out_rows = []
    for dt in g.INCREMENTS:
        for shape in (0.25, 0.5, 0.9):
            for s in range(g.STARTS // 4):
                ph0 = g.phase_word(s + 3)
                lane = m.Lane(
                    pitch=m.f2u(2.0 * g.f32(dt)), phase1=ph0, phase2=0,
                    shape_a=zero, shape_b=zero, ratio1=one, ratio2=one,
                    weights=(zero, one, zero, zero), out_gain=one, spread=m.f2u(shape),
                )  # fmt: skip
                out, _, _ = sine2.stage_shape_sine_4([lane, lane], high_pass=False)
                inc = m.fix(m.mul(m.ld(lane.pitch), m.ld(lane.ratio1)), 31)
                for k in range(1, 5):
                    ph = ((ph0 + k * inc) & 0xFFFFFFFF) / 2**32
                    out_rows.append((m.f2u(ph), m.f2u(inc / 2**32), m.f2u(shape), out[0][k - 1]))
    return out_rows


def rust() -> str:
    data = rows()
    parts = [
        "//! Firmware parity of SHAPE SINE wave 4 (`fw_sine_hp.rs`).\n"
        "//!\n"
        "//! Derived values only, made by `python tools/synth_stage_golden4.py > <file>` from the\n"
        "//! bit-exact model of stage `0x120DAB` (`docs/research/synth-stage-models-4.md`).\n"
    ]
    for i, name in enumerate(("PHASE", "DT", "SHAPE", "OUT")):
        parts.append(g.words_rust(f"SSH_{name}", [r[i] for r in data]))
    tail = (
        "\n#[test]\nfn golden_tables_have_the_documented_shape() {\n"
        "    assert_eq!(SSH_PHASE.len(), 96);\n"
        "    assert!(SSH_OUT.iter().all(|&w| f32::from_bits(w).is_finite()));\n}\n"
    )
    return "\n".join(parts) + tail


if __name__ == "__main__":
    sys.stdout.write(rust())
