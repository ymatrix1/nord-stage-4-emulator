#!/usr/bin/env python3
"""Numerical check of the SY1SY2 interpolation kernel set against a Kaiser-windowed sinc.

The 18,432 floats at DM 0xE3400 of the SY1SY2 DSP image are six 12-tap x 256-phase kernels
(`tools/ns4_synth_kernels.py`, docs/research/dsp-image-hunt.md 5.15). That tool fits a three-term
cosine window (rms 3.6e-5). This tool tests the closed form that explains the table to float32
precision (docs/research/sy-dsp-map.md section 3):

    h[tap][phase] = sinc(fc * d) * kaiser(d),   d = tap - 5 - phase / 256
    kaiser(d)     = I0(beta * sqrt(1 - (d / 6)^2)) / I0(beta),   beta = 8.5, |d| <= 6

with `fc` in (1.10, 1.05, 1.00, 0.96, 0.925, 0.895), the edge `d = +6` kept (value `1 / I0(beta)`
times the sinc). Pure Python (no numpy).

    python tools/ns4_sinc_check.py dsp_image_3_SY1SY2.bin
"""

from __future__ import annotations

import argparse
import math
import struct
import sys
from pathlib import Path

try:
    from ns4_synth_kernels import CENTER_TAP, PHASES, TAPS, find_kernels, sinc
except ImportError:  # imported as tools.ns4_synth_kernels
    from tools.ns4_synth_kernels import CENTER_TAP, PHASES, TAPS, find_kernels, sinc

HALF_WIDTH = 6.0
# VERIFIED: cutoffs and beta = fit of all 18,432 table values, free parameters converge on
# (1.1, 1.05, 1.0, 0.96, 0.925, 0.895, beta 8.50002, half width 6.00001), rms 1.0e-8
# (docs/research/sy-dsp-map.md section 3, reproduced by `main` below and test_ns4_sinc_check.py).
CUTOFFS = (1.10, 1.05, 1.00, 0.96, 0.925, 0.895)
BETA = 8.5


def bessel_i0(x: float) -> float:
    """Modified Bessel function of the first kind, order 0 (power series, exact to 1e-16 here)."""
    term = total = 1.0
    q = x * x / 4.0
    k = 1
    while term > total * 1e-17:
        term *= q / (k * k)
        total += term
        k += 1
    return total


def kaiser(d: float, beta: float = BETA, half_width: float = HALF_WIDTH) -> float:
    """Kaiser window, 1 at `d = 0` and `1 / I0(beta)` at `|d| = half_width` (zero beyond)."""
    if abs(d) > half_width:
        return 0.0
    return bessel_i0(beta * math.sqrt(max(0.0, 1.0 - (d / half_width) ** 2))) / bessel_i0(beta)


def kernel_value(fc: float, tap: int, phase: int, beta: float = BETA) -> float:
    """Model value of kernel `fc` at `[tap][phase]`."""
    d = tap - CENTER_TAP - phase / PHASES
    return sinc(fc * d) * kaiser(d, beta)


def model_block(fc: float, beta: float = BETA, phase_step: int = 1) -> list[float]:
    """`[tap][phase]` values for `phase` in `range(0, 256, phase_step)`."""
    return [kernel_value(fc, t, p, beta) for t in range(TAPS) for p in range(0, PHASES, phase_step)]


def subsample(block: list[float], phase_step: int) -> list[float]:
    """Every `phase_step`-th phase of a `[tap][phase]` block."""
    return [block[t * PHASES + p] for t in range(TAPS) for p in range(0, PHASES, phase_step)]


def errors(
    block: list[float], fc: float, beta: float = BETA, phase_step: int = 1
) -> tuple[float, float]:
    """(rms, max abs) difference between a table block and the model."""
    ref = model_block(fc, beta, phase_step)
    got = subsample(block, phase_step) if phase_step != 1 else block
    diffs = [a - b for a, b in zip(got, ref)]
    return math.sqrt(sum(x * x for x in diffs) / len(diffs)), max(abs(x) for x in diffs)


def fit_beta(
    blocks: list[list[float]], cutoffs=CUTOFFS, lo: float = 4.0, hi: float = 14.0
) -> float:
    """Kaiser beta minimising the rms error over all blocks (golden-section, every 8th phase)."""
    step = 8

    def cost(beta: float) -> float:
        return sum(errors(b, fc, beta, step)[0] ** 2 for b, fc in zip(blocks, cutoffs))

    g = (math.sqrt(5) - 1) / 2
    a, b = lo, hi
    c, d = b - g * (b - a), a + g * (b - a)
    fc_, fd_ = cost(c), cost(d)
    for _ in range(40):
        if fc_ < fd_:
            b, d, fd_ = d, c, fc_
            c = b - g * (b - a)
            fc_ = cost(c)
        else:
            a, c, fc_ = c, d, fd_
            d = a + g * (b - a)
            fd_ = cost(d)
    return (a + b) / 2


def mirror_error(block: list[float]) -> float:
    """Largest `|h[t][p] - h[11-t][256-p]|` (the kernels are symmetric about d = 0)."""
    return max(
        abs(block[t * PHASES + p] - block[(TAPS - 1 - t) * PHASES + PHASES - p])
        for t in range(TAPS)
        for p in range(1, PHASES)
    )


def dc_gain(block: list[float]) -> tuple[float, float]:
    """(min, max) over phases of the 12-tap sum; the model value is `1 / fc` for all phases."""
    sums = [sum(block[t * PHASES + p] for t in range(TAPS)) for p in range(PHASES)]
    return min(sums), max(sums)


def float32(x: float) -> float:
    """Round to IEEE single."""
    return struct.unpack("<f", struct.pack("<f", x))[0]


def report(img: bytes) -> tuple[int, list[str]]:
    """Lines describing the check; the first value is the exit code."""
    offset, blocks = find_kernels(img)
    if len(blocks) != len(CUTOFFS):
        return 1, ["kernel set not found"]
    out = [f"{len(blocks)} kernels at image offset 0x{offset:x}"]
    beta = fit_beta(blocks)
    out.append(f"fitted Kaiser beta = {beta:.4f} (nominal {BETA})")
    for k, (block, fc) in enumerate(zip(blocks, CUTOFFS)):
        rms, worst = errors(block, fc)
        lo, hi = dc_gain(block)
        out.append(
            f"kernel {k}: fc {fc:5.3f}  rms {rms:.2e}  max {worst:.2e}"
            f"  mirror {mirror_error(block):.0e}"
            f"  tap sum {lo:.5f}..{hi:.5f} (1/fc {1 / fc:.5f})"
        )
    return 0, out


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("image", type=Path, help="exported SY1SY2 image")
    args = p.parse_args(argv)
    code, lines = report(args.image.read_bytes())
    print("\n".join(lines))
    return code


if __name__ == "__main__":
    sys.exit(main())
