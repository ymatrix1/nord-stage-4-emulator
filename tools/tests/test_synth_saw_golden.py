"""Tests for tools/synth_saw_golden.py: the Rust golden grid is what the generator prints."""

import os
import sys
from pathlib import Path

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import synth_saw_golden as g  # noqa: E402
import synth_saw_model as m  # noqa: E402

RUST = Path(__file__).resolve().parents[2] / "crates/nord-core/src/synth/tests_hw_saw_golden.rs"


def test_the_rust_golden_file_is_what_the_generator_prints():
    assert RUST.read_text() == g.rust(), (
        "regenerate: python tools/synth_saw_golden.py > crates/nord-core/src/synth/"
        "tests_hw_saw_golden.rs"
    )


def test_grid_has_512_float32_points_and_is_deterministic():
    rows = g.grid()
    assert len(rows) == 512 and rows == g.grid()
    assert all(0.0 <= p < 1.0 and i > 0.0 for p, i, _ in rows)
    assert all(g.f32(y) == y for _, _, y in rows)


def test_expected_values_are_the_model_on_the_float32_inputs():
    for p, inc, y in g.grid()[::37]:
        assert abs(y - m.saw_sample(p, inc)) < 1e-6
    # above the fade end the saw is silent
    assert all(y == 0.0 for _, inc, y in g.grid() if inc > 0.2304)
