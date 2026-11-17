"""Unit tests for tools/org_system_timing.py (sharc-sys report parsing and aggregation)."""

from __future__ import annotations

import org_system_timing as ost
import pytest

REPORT = """316 passes (16 warm-up), 912 messages delivered, all idle: true
steps per pass: first 4309 min 3171 max 6377 mean 6279.6
sharc    mean    472.12 us  median    438.90 us  worst   1035.60 us  (n 300)
msg      mean      0.55 us  median      0.40 us  worst      7.90 us  (n 300)
stub     mean      0.30 us  median      0.30 us  worst      1.80 us  (n 300)
total    mean    472.98 us  median    439.70 us  worst   1038.50 us  (n 300)
"""


def run(mean: float, worst: float) -> dict[str, dict[str, float]]:
    return {c: {"mean": mean, "median": mean, "worst": worst} for c in ost.COMPONENTS}


def test_parse_run_reads_components_and_steps() -> None:
    r = ost.parse_run(REPORT)
    assert r["sharc"] == {"mean": 472.12, "median": 438.9, "worst": 1035.6}
    assert r["total"]["worst"] == 1038.5
    assert (r["steps"]["min"], r["steps"]["max"]) == (3171.0, 6377.0)


def test_aggregate_takes_best_median_and_worst() -> None:
    t = ost.aggregate([run(300.0, 900.0), run(500.0, 2000.0), run(400.0, 1000.0)])
    assert t["sharc"] == {"best_mean": 300.0, "median_mean": 400.0, "worst": 2000.0}
    with pytest.raises(ValueError):
        ost.aggregate([])


def test_arm_per_pass_and_table_text() -> None:
    assert ost.arm_per_pass_us(0.0217, 21.7) == pytest.approx(1000.0)
    text = ost.format_table(ost.aggregate([run(46.08, 92.16)]), 46.08)
    assert "| total | 46.08 | 46.08 | 92.16 |" in text
    assert "1.00x" in text and "2.00x" in text
