#!/usr/bin/env python3
"""Closed form for the 1,025-point waveshaper table of the note oscillator bank (`0x12272D`).

The table at `DM(0xE2512 + i)`, `i = -512..512`, is the integral of a Blackman-windowed sinc, i.e.
a band-limited step (an anti-aliased clipper): `T(-512) = -1`, `T(512) = 1`, odd up to a small
even part, overshoot 1.094 at `|i| = 171`. The firmware table is raw image data that cannot be
committed; `blep_table` regenerates it.

    T[i + 512] = -1 + 2 (sum_(j < i) k_j + k_i / 2) / sum_j k_j
    k_j        = sinc(t_j / a) blackman(t_j / 512),   t_j = j - 512 + shift   (j = 0..1024)

Fitted parameters (`fit`, grid search): `a` = 170.58, `shift` = -0.4638, `alpha` = 0.16 give an
rms error of 1.7e-4 (max 3.7e-4); the clean `a = 512 / 3`, `shift = -0.5` give 2.7e-4 (max 6.9e-4).
EMPIRICAL (fit to the image table, `docs/research/organ-model-paths.md` section 6).

    python tools/organ_shaper.py SRAM/core1_sram.bin --exe target/release/sharc-chain
"""

from __future__ import annotations

import argparse
import subprocess
import sys

import numpy as np

POINTS = 1025
HALF = 512
TABLE_BASE = 0xE2512  # DM word of T(0)
# EMPIRICAL (fit, rms 1.7e-4): sinc scale in table steps, half-sample shift, Blackman alpha.
FITTED = (170.58, -0.4638, 0.16)
# EMPIRICAL (rms 2.7e-4): the same with the clean values (zero crossings every 512 / 3 steps).
CLEAN = (HALF / 3, -0.5, 0.16)


def blackman(u: np.ndarray, alpha: float = 0.16) -> np.ndarray:
    """Blackman window on `u` in [-1, 1] (zero outside)."""
    w = (1 - alpha) / 2 + 0.5 * np.cos(np.pi * u) + alpha / 2 * np.cos(2 * np.pi * u)
    return np.where(np.abs(u) <= 1, w, 0.0)


def blep_table(a: float, shift: float, alpha: float = 0.16) -> np.ndarray:
    """The band-limited step table (`POINTS` values from -1 to 1)."""
    t = np.arange(POINTS) - HALF + shift
    k = np.sinc(t / a) * blackman(t / HALF, alpha)
    c = np.cumsum(k) - 0.5 * k
    return -1.0 + 2.0 * c / k.sum()


def error_stats(model: np.ndarray, table: np.ndarray) -> tuple[float, float]:
    """`(rms, max)` of `model - table`."""
    e = model - table
    return float(np.sqrt((e**2).mean())), float(np.abs(e).max())


def fit(table: np.ndarray, alpha: float = 0.16) -> tuple[float, float, float, float]:
    """Grid search over `a` and `shift`; returns `(a, shift, rms, max)` of the best point."""
    best = (0.0, 0.0, float("inf"), float("inf"))
    for a in np.arange(168.0, 174.0, 0.05):
        for shift in np.arange(-0.60, -0.30, 0.01):
            rms, peak = error_stats(blep_table(a, shift, alpha), table)
            if rms < best[2]:
                best = (float(a), float(shift), rms, peak)
    return best


def parse_dump(text: str, count: int = POINTS) -> np.ndarray:
    """Floats from the `DM(0x...): w w w ...` line of `sharc-chain --dump`."""
    line = next(ln for ln in text.splitlines() if ln.startswith("DM("))
    words = np.array([int(w, 16) for w in line.split(":", 1)[1].split()], dtype="<u4")
    if words.size != count:
        raise ValueError(f"expected {count} words, got {words.size}")
    return words.view("<f4").astype(np.float64)


def read_table(exe: str, image: str) -> np.ndarray:
    """The image table through `sharc-chain --dump` (no pass is run)."""
    start = TABLE_BASE - HALF
    out = subprocess.run(
        [exe, image, "--calls", "1", "--dump", f"{start:#x}:{POINTS}"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return parse_dump(out)


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image")
    ap.add_argument("--exe", required=True)
    ns = ap.parse_args(argv)
    table = read_table(ns.exe, ns.image)
    for label, params in (("fitted", FITTED), ("clean", CLEAN)):
        rms, peak = error_stats(blep_table(*params), table)
        print(f"{label} {params}: rms {rms:.3e} max {peak:.3e}")
    a, shift, rms, peak = fit(table)
    print(f"grid fit: a {a:.2f} shift {shift:.2f} rms {rms:.3e} max {peak:.3e}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
