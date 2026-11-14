#!/usr/bin/env python3
"""Fit a hardware recording as a non-negative power mix of emulator renders (third-octave bands).

Used to isolate one engine inside a multi-section clip: render the same notes once per section
(`emu_vs_hw.py` variants with `global.section_enable_i` edits), then ask how much of each render
the hardware spectrum needs and which bands no mix explains
(`docs/research/pipe-video-gap.md`).

    uv run python tools/mix_fit.py hw.wav organ.wav synth.wav [--min-hz 63] [--bands 21]
"""

from __future__ import annotations

import argparse
import itertools
import sys
from pathlib import Path
from typing import Sequence

import numpy as np
import video_ref

SEGMENT = 8192


def mono_samples(path: Path) -> tuple[int, np.ndarray]:
    """Sample rate and the mean of both channels of a WAV (PCM or float)."""
    sr, left, right = video_ref.read_wav_stereo(path)
    return sr, 0.5 * (np.asarray(left) + np.asarray(right))


def band_centres(min_hz: float, bands: int) -> np.ndarray:
    """Third-octave centre frequencies starting at `min_hz`."""
    return min_hz * 2.0 ** (np.arange(bands) / 3.0)


def third_octave_power(x: np.ndarray, sr: int, centres: np.ndarray) -> np.ndarray:
    """Power per third-octave band from an averaged Hann periodogram (50 % overlap)."""
    win = np.hanning(SEGMENT)
    hop = SEGMENT // 2
    if len(x) < SEGMENT:
        x = np.pad(x, (0, SEGMENT - len(x)))
    starts = range(0, len(x) - SEGMENT + 1, hop)
    psd = np.mean([np.abs(np.fft.rfft(x[s : s + SEGMENT] * win)) ** 2 for s in starts], axis=0)
    freqs = np.fft.rfftfreq(SEGMENT, 1.0 / sr)
    half = 2.0 ** (1.0 / 6.0)
    return np.array([psd[(freqs >= c / half) & (freqs < c * half)].sum() for c in centres])


def nnls_log(target: np.ndarray, parts: np.ndarray) -> np.ndarray:
    """Non-negative `c` minimising the relative error `|parts @ c - target| / target`.

    Exhaustive over active subsets (columns are few), each solved by weighted least squares.
    """
    k = parts.shape[1]
    # bands more than 60 dB under the loudest one are noise floor: leave them out of the fit
    keep = target > 1e-6 * target.max()
    parts, target = parts[keep], target[keep]
    w = 1.0 / np.maximum(target, 1e-30)
    best, best_err = np.zeros(k), np.inf
    for size in range(1, k + 1):
        for subset in itertools.combinations(range(k), size):
            cols = list(subset)
            sol, *_ = np.linalg.lstsq(parts[:, cols] * w[:, None], target * w, rcond=None)
            if np.any(sol < 0):
                continue
            c = np.zeros(k)
            c[cols] = sol
            err = float(np.sum((w * (parts @ c - target)) ** 2))
            if err < best_err:
                best, best_err = c, err
    return best


def fit_report(
    hw: Path, parts: Sequence[Path], min_hz: float = 63.0, bands: int = 21
) -> dict[str, object]:
    """Per-band hardware power, fitted power, error (dB) and each part's share of the fit."""
    centres = band_centres(min_hz, bands)
    sr, x = mono_samples(hw)
    target = third_octave_power(x, sr, centres)
    cols = []
    for p in parts:
        sr_p, y = mono_samples(p)
        cols.append(third_octave_power(y, sr_p, centres))
    mat = np.stack(cols, axis=1)
    coef = nnls_log(target, mat)
    fit = mat @ coef
    share = (mat * coef) / np.maximum(target[:, None], 1e-30)
    return {
        "centres_hz": centres,
        "hw_db": 10 * np.log10(np.maximum(target, 1e-30)),
        "fit_error_db": 10 * np.log10(np.maximum(fit, 1e-30) / np.maximum(target, 1e-30)),
        "coef": coef,
        "share_db": 10 * np.log10(np.maximum(share, 1e-9)),
    }


def main(argv: Sequence[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("hw", type=Path)
    ap.add_argument("parts", type=Path, nargs="+")
    ap.add_argument("--min-hz", type=float, default=63.0)
    ap.add_argument("--bands", type=int, default=21)
    args = ap.parse_args(argv)
    r = fit_report(args.hw, args.parts, args.min_hz, args.bands)
    print("coef (power):", ", ".join(f"{c:.3g}" for c in r["coef"]))  # type: ignore[union-attr]
    head = "centre Hz   hw dB  fit err dB  " + "  ".join(f"{p.stem[:12]:>12}" for p in args.parts)
    print(head)
    for i, c in enumerate(r["centres_hz"]):  # type: ignore[arg-type]
        shares = "  ".join(f"{s:12.1f}" for s in r["share_db"][i])  # type: ignore[index]
        print(f"{c:9.0f} {r['hw_db'][i]:7.1f} {r['fit_error_db'][i]:10.1f}  {shares}")  # type: ignore[index]
    return 0


if __name__ == "__main__":
    sys.exit(main())
