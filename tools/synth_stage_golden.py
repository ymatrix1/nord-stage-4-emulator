#!/usr/bin/env python3
"""Golden grids of the further firmware oscillator stages for the Rust port (`synth/fw_stages.rs`).

    python tools/synth_stage_golden.py > crates/nord-core/src/synth/tests_fw_stage_golden.rs

Four tables of derived words from the bit-exact models (checked against the SHARC oracle by
`tools/synth_wave_oracle.py`): the inline saw of the unison stages, the noise generator, the SUPER
spread law and one oscillator of Shape Square. Only derived values appear.
"""

from __future__ import annotations

import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import synth_stage_noise as noise
import synth_stage_shape_sine as shape_sine
import synth_stage_shapes as shapes
import synth_stage_unison as unison
import synth_wave_model as m

INCREMENTS = (0.002, 0.01, 0.03, 0.06)  # cycles per sample
SQUARE_SHAPES = (0.5, 0.25, -0.3)
STARTS = 8


def f32(x: float) -> float:
    return float(m.ld(m.f2u(x)))


def phase_word(k: int) -> int:
    """A phase word with 24 significant bits (exact as a float32 phase in cycles)."""
    return ((k * 0x2F1B3C5 + 12345) & 0xFFFFFF00) & 0xFFFFFFFF


def saw_rows() -> list[tuple[int, int, int]]:
    """`(phase bits, increment bits, sample bits)` of the unison inline saw."""
    rows = []
    for dt in INCREMENTS:
        dt32 = f32(dt)
        inc = int(dt32 * 2**32)
        for k in range(16):
            ph = phase_word(k)
            r7 = m.recip_refined(m.flt(inc, -31), m.HALF)
            y = unison.saw_diff([ph], r7)[0]
            rows.append((m.f2u(ph / 2**32), m.f2u(dt32), m.f2u(float(y))))
    return rows


def noise_rows() -> list[tuple[int, int]]:
    """`(state word, sample bits)` of the first 16 generator steps from state 3."""
    state, rows = 3, []
    for _ in range(16):
        state = noise.noise_states(state)[0]
        rows.append((state, m.st(m.flt(state, -31))))
    return rows


def spread_rows() -> list[tuple[int, int]]:
    """`(shape bits, u bits)` of the SUPER spread law."""
    out = []
    for i in range(16):
        s = i / 16 - 0.25
        u, _ = unison.spread_factor(m.f2u(s))
        out.append((m.f2u(s), m.f2u(float(u))))
    return out


def square_rows() -> list[tuple[int, int, int, int]]:
    """`(phase bits, increment bits, shape bits, sample bits)` of one Shape Square oscillator."""
    one, zero = m.f2u(1.0), m.f2u(0.0)
    rows = []
    for dt in INCREMENTS:
        for shape in SQUARE_SHAPES:
            for s in range(STARTS // 2):
                ph0 = phase_word(s)
                lane = m.Lane(
                    pitch=m.f2u(2.0 * f32(dt)), phase1=ph0, phase2=0,
                    shape_a=m.f2u(shape), shape_b=zero, ratio1=one, ratio2=one,
                    weights=(zero, one, zero, zero), out_gain=one,
                )  # fmt: skip
                out, _, _ = shapes.stage_shape_square([lane, lane])
                inc = m.fix(m.mul(m.ld(lane.pitch), m.ld(lane.ratio1)), 31)
                for k in range(1, 5):
                    ph = ((ph0 + k * inc) & 0xFFFFFFFF) / 2**32
                    rows.append((m.f2u(ph), m.f2u(inc / 2**32), m.f2u(shape), out[0][k - 1]))
    return rows


SINE_SHAPES = (0.0, 0.25, 0.5, 0.9)


def shape_sine_rows() -> list[tuple[int, int, int, int]]:
    """`(phase bits, increment bits, shape bits, sample bits)` of one SHAPE SINE 0 oscillator."""
    one, zero = m.f2u(1.0), m.f2u(0.0)
    rows = []
    for dt in INCREMENTS:
        for shape in SINE_SHAPES:
            for s in range(STARTS // 2):
                ph0 = phase_word(s)
                lane = m.Lane(
                    pitch=m.f2u(2.0 * f32(dt)), phase1=ph0, phase2=0,
                    shape_a=zero, shape_b=zero, ratio1=one, ratio2=one,
                    weights=(zero, one, zero, zero), out_gain=one, spread=m.f2u(shape),
                )  # fmt: skip
                out, _, _ = shape_sine.stage_shape_sine([lane, lane])
                inc = m.fix(m.mul(m.ld(lane.pitch), m.ld(lane.ratio1)), 31)
                for k in range(1, 5):
                    ph = ((ph0 + k * inc) & 0xFFFFFFFF) / 2**32
                    rows.append((m.f2u(ph), m.f2u(inc / 2**32), m.f2u(shape), out[0][k - 1]))
    return rows


def words_rust(name: str, words: list[int]) -> str:
    body = "".join(
        "    " + ", ".join(f"{w:#010x}" for w in words[i : i + 8]) + ",\n"
        for i in range(0, len(words), 8)
    )
    return f"pub(super) const {name}: [u32; {len(words)}] = [\n{body}];\n"


def stage_rows(stage_fn) -> list[tuple[int, int, int, int]]:
    """`(phase bits, increment bits, shape bits, sample bits)` of one oscillator of a SHAPE SINE
    stage (own weight 1, gain 1, spread word = shape): 2 starts x 3 shapes x 4 increments."""
    one, zero = m.f2u(1.0), m.f2u(0.0)
    rows = []
    for dt in INCREMENTS:
        for shape in (0.25, 0.5, 0.9):
            for s in range(STARTS // 4):
                ph0 = phase_word(s + 3)
                lane = m.Lane(
                    pitch=m.f2u(2.0 * f32(dt)), phase1=ph0, phase2=0,
                    shape_a=zero, shape_b=zero, ratio1=one, ratio2=one,
                    weights=(zero, one, zero, zero), out_gain=one, spread=m.f2u(shape),
                )  # fmt: skip
                out, _, _ = stage_fn([lane, lane])
                inc = m.fix(m.mul(m.ld(lane.pitch), m.ld(lane.ratio1)), 31)
                for k in range(1, 5):
                    ph = ((ph0 + k * inc) & 0xFFFFFFFF) / 2**32
                    rows.append((m.f2u(ph), m.f2u(inc / 2**32), m.f2u(shape), out[0][k - 1]))
    return rows


def rust_part3() -> str:
    """Rust golden file part 3: SHAPE SINE waves 1, 2 and 5."""
    parts = [
        "//! Firmware parity of SHAPE SINE waves 1, 2 and 5 (`fw_stages.rs`), part 3.\n"
        "//!\n"
        "//! Derived values only, made by `python tools/synth_stage_golden.py 3 > <file>`\n"
        "//! from the bit-exact models of stages `0x1235C9`, `0x1232A5` and `0x122F1F`\n"
        "//! (`docs/research/synth-stage-models-3.md`).\n"
    ]
    for tag, fn in (
        ("F", shape_sine.stage_shape_sine_1),
        ("G", shape_sine.stage_shape_sine_2),
        ("R", shape_sine.stage_shape_sine_5),
    ):
        rows = stage_rows(fn)
        for i, name in enumerate(("PHASE", "DT", "SHAPE", "OUT")):
            parts.append(words_rust(f"SS{tag}_{name}", [r[i] for r in rows]))
    tail = (
        "\n#[test]\nfn golden_tables_have_the_documented_shape() {\n"
        "    assert_eq!(SSF_PHASE.len(), 96);\n    assert_eq!(SSG_OUT.len(), 96);\n"
        "    assert!(SSR_OUT.iter().all(|&w| f32::from_bits(w).is_finite()));\n}\n"
    )
    return "\n".join(parts) + tail


def rust(part: int = 1) -> str:
    """Rust golden file text: part 1 (saw, noise, spread) or part 2 (Shape Square, SHAPE SINE 0)."""
    if part == 3:
        return rust_part3()
    if part == 1:
        saw, noi, spr = saw_rows(), noise_rows(), spread_rows()
        parts = [
            "//! Firmware parity of the unison and noise stages (`fw_stages.rs`), part 1.\n"
            "//!\n"
            "//! Derived values only, made by `python tools/synth_stage_golden.py 1 > <file>`\n"
            "//! from the bit-exact models of stages `0x121D6D` and `0x121FD8`\n"
            "//! (`docs/research/synth-stage-models.md`).\n",
            words_rust("SAW_PHASE", [r[0] for r in saw]),
            words_rust("SAW_DT", [r[1] for r in saw]),
            words_rust("SAW_OUT", [r[2] for r in saw]),
            words_rust("NOISE_STATE", [r[0] for r in noi]),
            words_rust("NOISE_OUT", [r[1] for r in noi]),
            words_rust("SPREAD_IN", [r[0] for r in spr]),
            words_rust("SPREAD_OUT", [r[1] for r in spr]),
        ]
        tail = """
#[test]
fn golden_tables_have_the_documented_shape() {
    assert_eq!(SAW_PHASE.len(), 64);
    assert_eq!(NOISE_STATE.len(), 16);
    assert_eq!(SPREAD_IN.len(), 16);
}
"""
        return "\n".join(parts) + tail
    sq, ss = square_rows(), shape_sine_rows()
    parts = [
        "//! Firmware parity of Shape Square and SHAPE SINE 0 (`fw_stages.rs`), part 2.\n"
        "//!\n"
        "//! Derived values only, made by `python tools/synth_stage_golden.py 2 > <file>`\n"
        "//! from the bit-exact models of stages `0x121AB5` and `0x123427`\n"
        "//! (`docs/research/synth-stage-models.md`, `synth-stage-models-2.md`).\n",
        words_rust("SQUARE_PHASE", [r[0] for r in sq]),
        words_rust("SQUARE_DT", [r[1] for r in sq]),
        words_rust("SQUARE_SHAPE", [r[2] for r in sq]),
        words_rust("SQUARE_OUT", [r[3] for r in sq]),
        words_rust("SSINE_PHASE", [r[0] for r in ss]),
        words_rust("SSINE_DT", [r[1] for r in ss]),
        words_rust("SSINE_SHAPE", [r[2] for r in ss]),
        words_rust("SSINE_OUT", [r[3] for r in ss]),
    ]
    tail = """
#[test]
fn golden_tables_have_the_documented_shape() {
    assert_eq!(SQUARE_PHASE.len(), 192);
    assert_eq!(SSINE_PHASE.len(), 256);
    assert!(SQUARE_OUT.iter().all(|&w| f32::from_bits(w).is_finite()));
}
"""
    return "\n".join(parts) + tail


if __name__ == "__main__":
    sys.stdout.write(rust(int(sys.argv[1]) if len(sys.argv) > 1 else 1))
