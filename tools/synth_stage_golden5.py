#!/usr/bin/env python3
"""Golden grids of the sixth batch of stages for the Rust ports (`synth/fw_super1.rs`, ...).

    python tools/synth_stage_golden5.py > crates/nord-core/src/synth/tests_fw_stage_golden5.rs

Derived values of the bit-exact models (checked against the SHARC oracle by
`tools/synth_wave_oracle.py`): the SUPER wave 1 inline square. Only derived values appear.
"""

from __future__ import annotations

import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import synth_stage_golden as g
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
    tail = (
        "\n#[test]\nfn golden_tables_have_the_documented_shape() {\n"
        f"    assert_eq!(FOLD_PHASE.len(), {len(fold)});\n"
        "    assert!(FOLD_OUT.iter().all(|&w| f32::from_bits(w).is_finite()));\n}\n"
    )
    return "\n".join(parts) + tail


if __name__ == "__main__":
    sys.stdout.write(rust())
