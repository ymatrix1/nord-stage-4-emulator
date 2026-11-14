#!/usr/bin/env python3
"""Golden grid of the firmware SHAPE oscillator for the Rust port (`synth/fw_shape.rs`).

    python tools/synth_wave_golden.py > crates/nord-core/src/synth/tests_fw_shape_golden.rs

Points `(phase, increment, shape, sample)` of one oscillator (lane x, mix weight 1, ratio 1) of the
bit-exact model `synth_wave_model.stage` (checked word for word against the SHARC oracle by
`tools/synth_wave_oracle.py`): 4 increments x 3 shapes x 8 block starts x the 4 block samples.
Only derived values appear.
"""

from __future__ import annotations

import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import synth_wave_model as m

INCREMENTS = (0.005, 0.02, 0.06, 0.12)  # cycles per SY frame
SHAPES = (0.5, 0.25, 0.0)
STARTS = 8


def one_oscillator(
    i_sy: float, shape: float, phase0: int
) -> tuple[list[float], float, list[float]]:
    """`(phases in cycles, exact increment in cycles, four samples)` of lane x."""
    one, zero = m.f2u(1.0), m.f2u(0.0)
    lane = m.Lane(
        pitch=m.f2u(2.0 * i_sy), phase1=phase0, phase2=0,
        shape_a=m.f2u(shape), shape_b=zero, ratio1=one, ratio2=one,
        weights=(zero, one, zero, zero), out_gain=one,
    )  # fmt: skip
    out, _, _ = m.stage([lane, lane])
    inc = m.fix(m.mul(m.ld(lane.pitch), m.ld(lane.ratio1)), 31)
    phases = [((phase0 + k * inc) & 0xFFFFFFFF) / 2**32 for k in (1, 2, 3, 4)]
    return phases, inc / 2**32, [float(m.ld(v)) for v in out[0]]


def grid() -> list[tuple[float, float, float, float]]:
    rows = []
    for i_sy in INCREMENTS:
        for shape in SHAPES:
            for s in range(STARTS):
                phase0 = (s * 0x2F1B3C5 + 12345) & 0xFFFFFFFF
                phases, inc, outs = one_oscillator(i_sy, shape, phase0)
                rows += [(p, inc, shape, y) for p, y in zip(phases, outs, strict=True)]
    return rows


def bits(x: float) -> int:
    return m.f2u(x)


def words_rust(name: str, words: list[int]) -> str:
    body = "".join(
        "    " + ", ".join(f"{w:#010x}" for w in words[i : i + 8]) + ",\n"
        for i in range(0, len(words), 8)
    )
    return f"pub(super) const {name}: [u32; {len(words)}] = [\n{body}];\n"


def rust() -> str:
    rows = grid()
    head = (
        "//! Firmware parity of the synth SHAPE oscillator (`fw_shape.rs`): golden grid.\n"
        "//!\n"
        "//! Derived values only (float32 phase, increment, shape word, expected sample), made\n"
        "//! by `python tools/synth_wave_golden.py > <this file>` from the bit-exact model of\n"
        "//! stage `0x1212D3` (`docs/research/synth-wave-models.md`).\n\n"
    )
    cols = [("SHAPE_PHASE", 0), ("SHAPE_INC", 1), ("SHAPE_SHAPE", 2), ("SHAPE_OUT", 3)]
    body = "".join(words_rust(n, [bits(r[i]) for r in rows]) for n, i in cols)
    tests = """
#[test]
fn golden_grid_has_the_documented_shape() {
    assert_eq!(SHAPE_PHASE.len(), 384);
    assert_eq!(SHAPE_OUT.len(), SHAPE_PHASE.len());
    assert!(SHAPE_OUT.iter().all(|&w| f32::from_bits(w).is_finite()));
}
"""
    return head + body + tests


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    sys.stdout.reconfigure(newline="\n")
    print(rust(), end="")
    return 0


if __name__ == "__main__":
    sys.exit(main())
