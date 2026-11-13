"""Tests for tools/synth_velocity.py."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import synth_velocity as sv  # noqa: E402


def test_table_is_monotone_with_85_distinct_values() -> None:
    g = sv.load_g()
    assert len(g) == 127
    assert g == sorted(g)
    assert len(set(g)) == 85
    assert [g[v - 1] for v in (1, 16, 32, 48, 64, 80, 100, 127)] == [0, 3, 8, 17, 32, 58, 98, 127]


def test_smoothstep_fit_is_within_one_step() -> None:
    exact, worst = sv.fit_quality(sv.load_g(), sv.load_curve())
    assert worst <= 1
    assert exact >= 90


def test_smoothstep_endpoints() -> None:
    assert sv.smoothstep_g(0) == 0
    assert sv.smoothstep_g(10_000) == 127


def test_rust_table_equals_the_arm_run() -> None:
    rust = sv.parse_rust_table(sv.RUST_TABLE.read_text(encoding="utf-8"))
    assert rust == sv.load_g()


def test_parse_rust_table_rejects_missing_table() -> None:
    try:
        sv.parse_rust_table("nothing")
    except ValueError:
        return
    raise AssertionError("expected ValueError")
