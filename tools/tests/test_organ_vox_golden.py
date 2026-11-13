"""Unit tests for tools/organ_vox_golden.py (closed-form tables, no firmware needed)."""

from __future__ import annotations

import re
from pathlib import Path

import numpy as np
import organ_vox_golden as vg
import pytest

ORGAN = Path(__file__).resolve().parents[2] / "crates" / "nord-core" / "src" / "organ"


def rust_array(text: str, name: str) -> list[float]:
    """Numbers of `const NAME: ... = [ ... ];` in a Rust source (tuples and nesting flattened)."""
    m = re.search(rf"const {name}: [^=]+= \[(.*?)\n\];", text, re.S)
    assert m, name
    return [float(x) for x in re.findall(r"-?\d+\.?\d*(?:e-?\d+)?", m.group(1))]


def test_cutoff_ratios_reproduce_the_image_coefficient_range() -> None:
    c = [vg.cutoff_coefficient(k) for k in range(84)]
    assert c[0] == pytest.approx(0.004505, rel=1e-3)
    assert c[83] == pytest.approx(0.30378, rel=1e-3)
    assert c[4] == c[7] and c[4] != c[8]  # four notes share one coefficient
    assert len(vg.CUTOFF_RATIO) == 21


def test_rust_bank_uses_the_same_cutoff_ratios() -> None:
    src = (ORGAN / "vox_bank.rs").read_text(encoding="utf-8")
    m = re.search(r"const CUTOFF_RATIO: \[f32; 21\] = \[(.*?)\];", src, re.S)
    assert m
    ratios = tuple(float(x) for x in re.findall(r"\d\.\d+", m.group(1)))
    assert ratios == vg.CUTOFF_RATIO


def test_table_samples_match_the_embedded_rust_constants() -> None:
    text = (ORGAN / "vox_golden.rs").read_text(encoding="utf-8")
    nums = rust_array(text, "TABLE_SAMPLES")
    pairs = list(zip(nums[0::2], nums[1::2], strict=True))
    table = vg.table_values()
    assert [int(i) for i, _ in pairs] == list(vg.TABLE_INDICES)
    for i, v in pairs:
        assert table[int(i)] == pytest.approx(v, abs=2e-7)


def test_output_signals_match_the_embedded_rust_constants() -> None:
    text = (ORGAN / "vox_golden.rs").read_text(encoding="utf-8")
    xs = vg.output_input()
    assert rust_array(text, "OUT_INPUT") == pytest.approx(xs, abs=1e-7)
    ys = vg.output_signals(xs)
    assert rust_array(text, "OUT_EXPECTED") == pytest.approx(ys, abs=1e-7)
    assert max(abs(v) for v in ys) > 0.01  # the golden is not trivially zero


def test_note_signals_match_the_embedded_rust_constants_for_the_first_records() -> None:
    text = (ORGAN / "vox_golden.rs").read_text(encoding="utf-8")
    flat = rust_array(text, "Y_PLAIN")
    width = len(vg.NOTES_PLAIN)
    first = [flat[i * width : (i + 1) * width] for i in range(2)]
    rec = vg.note_signals(0, vg.NOTES_PLAIN, frames=vg.STRIDE + 1, stride=vg.STRIDE)
    assert len(rec) == 2
    for got, want in zip(rec, first, strict=True):
        assert got == pytest.approx(want, abs=1e-7)


def test_float_formatting_round_trips_and_keeps_a_decimal_point() -> None:
    for v in (0.0, 1.0, -0.3, 1e-9, 0.090075):
        s = vg.fmt(v)
        assert np.float32(float(s)) == np.float32(v)
        assert "." in s or "e" in s


def test_output_stage_has_the_image_sections_and_blocks_dc() -> None:
    ys = vg.output_signals([1.0] * 2000)
    assert abs(ys[-1]) < 0.01 * max(abs(v) for v in ys)  # the DC blocker decays the step
    assert [s[0] for s in vg.SECTIONS] == [0.9891, 0.9514, 0.8348]
