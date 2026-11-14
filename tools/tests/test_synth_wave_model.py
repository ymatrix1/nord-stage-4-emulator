"""Tests for the stage `0x1212D3` model and the oracle driver helpers (no firmware needed)."""

from __future__ import annotations

import json
import random
import sys
from fractions import Fraction
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import synth_wave_golden as g  # noqa: E402
import synth_wave_model as m  # noqa: E402
import synth_wave_oracle as o  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
CASES = ROOT / "docs/research/data/synth-shape-oracle-cases.json"
RUST = ROOT / "crates/nord-core/src/synth/tests_fw_shape_golden.rs"


def lanes_of(case: dict) -> list[m.Lane]:
    return [
        m.Lane(
            ln["pitch"],
            ln["phase1"],
            ln["phase2"],
            ln["shape_a"],
            ln["shape_b"],
            ln["ratio1"],
            ln["ratio2"],
            tuple(ln["weights"]),
            ln["out_gain"],
        )
        for ln in case["lanes"]
    ]


def test_model_reproduces_the_recorded_oracle_words_exactly() -> None:
    cases = json.loads(CASES.read_text())["cases"]
    assert len(cases) == 10
    for case in cases:
        out, _, _ = m.stage(lanes_of(case))
        assert out == case["oracle_out"]


def test_float_helpers_round_trip_and_round_like_the_40_bit_registers() -> None:
    assert m.ld(m.f2u(1.5)) == Fraction(3, 2)
    assert m.st(m.ld(0x3F7FFFFF)) == 0x3F7FFFFF
    # a 40-bit register keeps 8 extra bits that a store truncates
    x = m.sub(m.ONE, Fraction(1, 2**28))
    assert m.st(x) == 0x3F7FFFFF and x != m.ld(0x3F800000)
    assert m.flt(0x40000000, -31) == Fraction(1, 2) and m.fix(Fraction(1, 2), 31) == 0x40000000
    assert m.clip(Fraction(3), Fraction(-1)) == 1 and m.clip(Fraction(-3), Fraction(1)) == -1


def test_recips_seed_and_refinement_give_the_reciprocal() -> None:
    for x in (Fraction(3, 100), Fraction(1, 7), Fraction(9, 10)):
        approx = m.recip_refined(m.rnd(x), m.ONE)
        assert abs(float(approx) * float(m.rnd(x)) - 1.0) < 1e-6


def test_stage_advances_the_phase_by_four_increments() -> None:
    one, zero = m.f2u(1.0), m.f2u(0.0)
    lane = m.Lane(m.f2u(0.04), 0, 0, m.f2u(0.5), zero, one, one, (zero, one, zero, zero), one)
    out, p1, _ = m.stage([lane, lane])
    inc = m.fix(m.ld(lane.pitch), 31)
    assert len(out[0]) == 4 and p1[0] == (inc * 4) & 0xFFFFFFFF


def test_oracle_pokes_cover_the_surviving_words_of_both_lanes() -> None:
    lanes = o.random_lanes(random.Random(1))
    pk = o.pokes(lanes)
    assert len(lanes) == 2 and len(pk) == 2 * 9 + 1
    assert pk[o.RECORD - 21] == lanes[0].phase1 and pk[o.RECORD - 20] == lanes[1].phase1


def test_rust_golden_file_is_what_the_generator_prints() -> None:
    assert RUST.read_text() == g.rust(), "regenerate: python tools/synth_wave_golden.py > <file>"
    assert len(g.grid()) == 384


STAGE_CASES = ROOT / "docs/research/data/synth-stage-oracle-cases.json"


def test_saw_and_pulse_stages_reproduce_the_recorded_oracle_words() -> None:
    data = json.loads(STAGE_CASES.read_text())["stages"]
    models = {
        "0x1215fb": m.stage_saw,
        "0x121b99": m.stage_pulse,
        "0x120cd0": m.stage_sine,
        "0x121183": m.stage_triangle,
        "0x12169d": m.stage_shape_saw,
    }
    assert set(data) == set(models)
    for name, model in models.items():
        assert len(data[name]) == 6
        for case in data[name]:
            lanes = [
                m.Lane(
                    ln["pitch"],
                    ln["phase1"],
                    ln["phase2"],
                    ln["shape_a"],
                    0,
                    ln["ratio1"],
                    ln["ratio2"],
                    tuple(ln["weights"]),
                    ln["out_gain"],
                    ln["pulse"],
                )
                for ln in case["lanes"]
            ]
            out, _, _ = model(lanes)
            assert out == case["oracle_out"], name


def test_saw_helper_is_a_falling_band_limited_ramp() -> None:
    inc = m.fix(Fraction(1, 200), 31)
    ys = [float(v) for v in m.saw_helper(0x20000000, inc)[0]]
    assert ys[0] > ys[1] > ys[2] > ys[3] and all(abs(y) < 1.2 for y in ys)


def test_pulse_helper_is_flat_between_the_edges() -> None:
    inc = m.fix(Fraction(1, 200), 31)
    a, _ = m.pulse_helper(0x20000000, inc, 0x80000000)
    assert max(float(v) for v in a) - min(float(v) for v in a) < 1e-5
