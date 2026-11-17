"""Unit tests for tools/arm_profile.py (the pure helpers; the Unicorn runs need local firmware)."""

from __future__ import annotations

import arm_profile as ap
import pytest


def test_mips() -> None:
    assert ap.mips(20_000_000, 1.0) == pytest.approx(20.0)
    assert ap.mips(1, 0.0) == 0.0


def test_split_tick_clamps_the_rest() -> None:
    s = ap.split_tick(0.018, 0.010, 0.002)
    assert s == {"unicorn": 0.010, "hooks": 0.002, "rest": pytest.approx(0.006)}
    assert ap.split_tick(0.010, 0.010, 0.002)["rest"] == 0.0


def test_realtime_factor_is_wall_over_budget() -> None:
    assert ap.realtime_factor(0.018) == pytest.approx(18.0)
    assert ap.realtime_factor(0.005, 0.0025) == pytest.approx(2.0)


def test_summarise_mean_median_worst() -> None:
    s = ap.summarise([1.0, 2.0, 9.0])
    assert (s["mean"], s["median"], s["worst"]) == (4.0, 2.0, 9.0)


def test_hook_table_orders_by_cost() -> None:
    rows = ap.hook_table({"a": [10, 0.001], "b": [100, 0.010]}, ticks=10)
    assert [r[0] for r in rows] == ["b", "a"]
    assert rows[0][1:] == pytest.approx((10.0, 1.0))


def test_loop_code_is_three_arm_words() -> None:
    assert len(ap.LOOP_CODE) == 12
    assert ap.TICK_INSNS == 200_000
