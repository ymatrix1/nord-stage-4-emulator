#!/usr/bin/env python3
"""Golden grid of the seventh batch of stages for the Rust ports (`synth/fw_sync3.rs`).

    python tools/synth_stage_golden6.py > crates/nord-core/src/synth/tests_fw_stage_golden6.rs

Derived values of the bit-exact model `tools/synth_stage_sync3.py` (checked against the SHARC
oracle by `tools/synth_wave_oracle.py --stage 0x122C5C`): one SYNC 3..5 oscillator before the
output high pass. Only derived values appear.
"""

from __future__ import annotations

import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import synth_stage_golden as g
import synth_stage_sync3 as sync3
import synth_wave_model as m

IDLE_PULSE = 0x40000000  # VERIFIED idle word `DM(0xC15AE)` (`fw_sync3.PULSE_WORD`)


def sync3_rows() -> list[tuple[int, int, int, int]]:
    """`(phase bits, increment bits, shape bits, sample bits)` of one SYNC 3..5 oscillator (own
    weight 1, gain 1): 2 starts x 5 shapes x 4 increments x 4 samples (shape = spread word)."""
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
                    pulse=IDLE_PULSE,
                )  # fmt: skip
                out, _, _ = sync3.stage_sync3([lane, lane], high_pass=False)
                inc = m.fix(m.mul(m.ld(lane.pitch), m.ld(lane.ratio1)), 31)
                for k in range(1, 5):
                    ph = ((ph0 + k * inc) & 0xFFFFFFFF) / 2**32
                    rows.append((m.f2u(ph), m.f2u(inc / 2**32), m.f2u(shape), out[0][k - 1]))
    return rows


def rust() -> str:
    parts = [
        "//! Firmware parity of the seventh stage batch (`fw_sync3.rs`).\n"
        "//!\n"
        "//! Derived values only, made by `python tools/synth_stage_golden6.py > <file>` from the\n"
        "//! bit-exact model (`docs/research/synth-stage-models-6.md`).\n"
    ]
    rows = sync3_rows()
    for i, name in enumerate(("PHASE", "DT", "SHAPE", "OUT")):
        parts.append(g.words_rust(f"SY3_{name}", [r[i] for r in rows]))
    tail = (
        "\n#[test]\nfn golden_tables_have_the_documented_shape() {\n"
        f"    assert_eq!(SY3_PHASE.len(), {len(rows)});\n"
        "    assert!(SY3_OUT.iter().all(|&w| f32::from_bits(w).is_finite()));\n}\n"
    )
    return "\n".join(parts) + tail


if __name__ == "__main__":
    sys.stdout.write(rust())
