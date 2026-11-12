#!/usr/bin/env python3
"""Extract and check the sample interpolation kernels of the SY1SY2 DSP image.

The image holds six blocks of 3072 float32 values (12 taps x 256 phases, stored `[tap][phase]`).
Model fitted to the data (docs/research/dsp-image-hunt.md 5.15):

    value[tap][phase] = sinc(fc * d) * blackman(d),  d = tap - 5 - phase / 256,  |d| < 6
    blackman(d) = 0.42 + 0.5 cos(pi d / 6) + 0.08 cos(2 pi d / 6)

with a cutoff `fc` per block. The per-phase tap sum (DC gain) is `1 / fc`, so `fc` follows from the
block sum without any search.

    python tools/ns4_synth_kernels.py dsp_image_3_SY1SY2.bin
"""

from __future__ import annotations

import argparse
import math
import struct
import sys
from pathlib import Path

TAPS, PHASES = 12, 256
BLOCK = TAPS * PHASES  # 3072 floats
CENTER_TAP = 5
HALF_WIDTH = 6.0


def sinc(x: float) -> float:
    """Normalised sinc, `sin(pi x) / (pi x)`."""
    return 1.0 if abs(x) < 1e-12 else math.sin(math.pi * x) / (math.pi * x)


def blackman(d: float, half_width: float = HALF_WIDTH) -> float:
    """Blackman window centred on 0 and zero outside `|d| >= half_width`."""
    if abs(d) >= half_width:
        return 0.0
    a = math.pi * d / half_width
    return 0.42 + 0.5 * math.cos(a) + 0.08 * math.cos(2 * a)


def model_block(fc: float) -> list[float]:
    """The 3072 values predicted for cutoff `fc`, in `[tap][phase]` order."""
    out = []
    for tap in range(TAPS):
        for phase in range(PHASES):
            d = tap - CENTER_TAP - phase / PHASES
            out.append(sinc(fc * d) * blackman(d))
    return out


def fit_cutoff(block: list[float]) -> float:
    """Cutoff from the mean per-phase tap sum (`1 / fc`)."""
    sums = [sum(block[t * PHASES + p] for t in range(TAPS)) for p in range(PHASES)]
    return 1.0 / (sum(sums) / PHASES)


def rms_error(block: list[float], fc: float) -> float:
    """Root-mean-square difference between `block` and the model with cutoff `fc`."""
    ref = model_block(fc)
    return math.sqrt(sum((a - b) ** 2 for a, b in zip(block, ref)) / len(block))


def find_kernels(img: bytes) -> tuple[int, list[list[float]]]:
    """Locate the kernel run by its peaks (longest chain of 3072-float blocks fitting the model).

    A block starts 1280 floats (tap 5, phase 0) before a value of exactly 1.0. Returns the byte
    offset of the first block and the blocks, or `(-1, [])` when no block matches within 0.01 rms.
    """
    n = len(img) // 4
    words = struct.unpack_from(f"<{n}f", img, 0)
    peak = CENTER_TAP * PHASES
    best: tuple[int, list[list[float]]] = (-1, [])
    for i in range(peak, n - BLOCK + peak + 1):
        if words[i] != 1.0:
            continue
        start = i - peak
        blocks: list[list[float]] = []
        while start + BLOCK <= n:
            block = list(words[start : start + BLOCK])
            if not (block[peak] == 1.0 and rms_error(block, fit_cutoff(block)) < 0.01):
                break
            blocks.append(block)
            start += BLOCK
        if len(blocks) > len(best[1]):
            best = ((i - peak) * 4, blocks)
    return best


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("image", type=Path, help="exported SY1SY2 image")
    args = p.parse_args(argv)
    offset, blocks = find_kernels(args.image.read_bytes())
    if not blocks:
        print("no interpolation kernels found")
        return 1
    print(f"{len(blocks)} kernels of {TAPS} taps x {PHASES} phases at image offset 0x{offset:x}")
    for k, block in enumerate(blocks):
        fc = fit_cutoff(block)
        err = rms_error(block, fc)
        print(f"kernel {k}: fc = {fc:.4f} (gain {1 / fc:.4f}), rms error vs model {err:.5f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
