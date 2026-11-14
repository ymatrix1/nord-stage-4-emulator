"""Tests for the Ladder24 stage model `0x12549D` (no firmware needed)."""

from __future__ import annotations

import json
import sys
from fractions import Fraction
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import synth_filter_ladder as lad  # noqa: E402
import synth_filter_ladder_golden as gold  # noqa: E402
import synth_filter_ladder_oracle as orc  # noqa: E402

CASES = Path(__file__).resolve().parents[2] / "docs/research/data/synth-ladder-oracle-cases.json"


def test_recorded_oracle_cases_are_reproduced_word_for_word() -> None:
    cases = json.loads(CASES.read_text(encoding="utf-8"))["cases"]
    assert len(cases) == 6
    for case in cases:
        got = orc.model_run(case)
        assert got == case["oracle"]


def test_random_case_is_deterministic_per_seed() -> None:
    assert orc.random_case(3) == orc.random_case(3)
    assert orc.random_case(3) != orc.random_case(4)


def test_tables_have_the_documented_size_and_shape() -> None:
    assert [len(r) for r in lad.ROWS] == [129] * 4
    assert len(lad.SHAPER) == 1025
    assert lad.SHAPER[512] == 0
    assert lad.s.ld(lad.SHAPER[513]) == -lad.s.ld(lad.SHAPER[511])


def test_position_splits_the_cutoff_word() -> None:
    idx, fr = lad.position(0x3F000000)  # 0.5 -> index 64
    assert (idx, fr) == (64, Fraction(0))
    idx, fr = lad.position(0x3E99999A)  # 0.3
    assert idx == 38 and 0 < fr < 1


def test_coefficients_follow_the_rows() -> None:
    c18, c16, c14, c12 = lad.coefficients(0x3F000000, 0x40000000)
    assert c18 == lad.ROWS[0][64] and c16 == lad.ROWS[1][64] and c14 == lad.ROWS[2][64]
    # q = 0.5 (index 64), kf = row4 * q / 4
    want = lad.s.fmul(lad.s.ld(lad.ROWS[3][64]), Fraction(1, 8))
    assert c12 == lad.s.st(want)


def test_filter_without_feedback_has_unit_dc_gain() -> None:
    coefs = lad.coefficients(0x3E99999A, 0)
    ln = lad.Lane()
    x = lad.s.st(Fraction(1, 2))
    out = 0
    for _ in range(2000):
        out = lad.process(ln, coefs, [x] * 4)[-1]
    assert abs(float(lad.s.ld(out)) - 0.5) < 1e-3


def test_generators_print_the_rust_files() -> None:
    tables = gold.tables()
    assert "pub(super) const ROW0: [u32; 129]" in tables
    assert "pub(super) const SHAPER: [u32; 1025]" in tables
    golden = gold.golden()
    assert "pub(super) const COEFS: [u32; 20]" in golden
    assert f"pub(super) const OUTPUT: [u32; {gold.SAMPLES * len(gold.ROWS)}]" in golden
    assert gold.main(["bogus"]) == 2
