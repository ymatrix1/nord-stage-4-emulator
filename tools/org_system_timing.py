#!/usr/bin/env python3
"""Aggregate repeated `sharc-sys` runs into one per-component timing table (D119 system run).

`sharc-sys` prints one block per run; host load from other processes inflates single runs, so the
table reports, per component, the lowest run mean (least disturbed run), the median of the run
means and the worst pass seen in any run. ARM time per pass interval is the 1 ms tick wall time
divided by the passes per tick (`arm_org_stream.passes_per_tick`).

    python tools/org_system_timing.py RUN1.txt RUN2.txt ... [--arm-tick-seconds 0.0177]
"""

from __future__ import annotations

import argparse
import re
import statistics
import sys
from pathlib import Path

LINE = re.compile(
    r"^(sharc|msg|stub|total)\s+mean\s+([\d.]+) us\s+median\s+([\d.]+) us\s+worst\s+([\d.]+) us"
)
STEPS = re.compile(r"steps per pass: first (\d+) min (\d+) max (\d+) mean ([\d.]+)")
COMPONENTS = ("sharc", "msg", "stub", "total")


def parse_run(text: str) -> dict[str, dict[str, float]]:
    """`{component: {mean, median, worst}}` (microseconds) of one `sharc-sys` report."""
    out: dict[str, dict[str, float]] = {}
    for line in text.splitlines():
        m = LINE.match(line)
        if m:
            out[m[1]] = {"mean": float(m[2]), "median": float(m[3]), "worst": float(m[4])}
    steps = STEPS.search(text)
    if steps:
        out["steps"] = {"min": float(steps[2]), "max": float(steps[3]), "mean": float(steps[4])}
    return out


def aggregate(runs: list[dict[str, dict[str, float]]]) -> dict[str, dict[str, float]]:
    """Per component: lowest run mean, median of run means, worst pass over all runs."""
    if not runs:
        raise ValueError("no runs")
    table = {}
    for c in COMPONENTS:
        means = [r[c]["mean"] for r in runs]
        table[c] = {
            "best_mean": min(means),
            "median_mean": statistics.median(means),
            "worst": max(r[c]["worst"] for r in runs),
        }
    return table


def arm_per_pass_us(tick_seconds: float, passes_per_tick: float) -> float:
    """ARM wall time attributed to one pass interval, in microseconds."""
    return tick_seconds / passes_per_tick * 1e6


def format_table(table: dict[str, dict[str, float]], period_us: float) -> str:
    """Markdown rows `component | best mean | median mean | worst | x period`."""
    rows = ["| Component | best run mean (us) | median of run means (us) | worst pass (us) |"]
    rows.append("|---|---:|---:|---:|")
    for c in COMPONENTS:
        t = table[c]
        rows.append(f"| {c} | {t['best_mean']:.2f} | {t['median_mean']:.2f} | {t['worst']:.2f} |")
    t = table["total"]
    rows.append(
        f"\nbest mean / pass period ({period_us:.2f} us) = {t['best_mean'] / period_us:.2f}x, "
        f"worst / period = {t['worst'] / period_us:.2f}x"
    )
    return "\n".join(rows)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("runs", nargs="+", type=Path)
    ap.add_argument("--period-us", type=float, default=46.08)
    ns = ap.parse_args(argv)
    runs = [parse_run(p.read_text(encoding="utf-8")) for p in ns.runs]
    sys.stdout.write(format_table(aggregate(runs), ns.period_us) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
