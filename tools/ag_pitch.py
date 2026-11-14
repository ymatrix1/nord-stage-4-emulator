#!/usr/bin/env python3
"""Pitch tracks of a held harmonic note: vibrato rate / depth / onset and pitch-blip decay.

A partial ``h`` of a known note is followed in the STFT (parabolic peak refinement), divided
by ``h`` and expressed in cents re ``fref``. The same functions run on hardware slices and on
emulator renders so the analysis bias cancels in the comparison.
"""

from __future__ import annotations

import argparse
from pathlib import Path
from typing import Sequence

import ag_audio
import numpy as np


def partial_cents(
    x: np.ndarray,
    sr: int,
    fref: float,
    h: int,
    n: int = 2048,
    hop: int = 480,
    span_cents: float = 1500.0,
    lo_cents: float = -200.0,
) -> tuple[np.ndarray, np.ndarray]:
    """Return ``(times_s, cents)`` of partial ``h`` of ``fref`` (window centres)."""
    mag = ag_audio.stft_mag(x, n, hop)
    f = np.arange(mag.shape[1]) * sr / n
    lo = fref * h * 2.0 ** (lo_cents / 1200)
    hi = fref * h * 2.0 ** (span_cents / 1200)
    sel = np.where((f >= lo) & (f <= hi))[0]
    cents = np.empty(len(mag))
    for i, row in enumerate(mag):
        k = sel[int(np.argmax(row[sel]))]
        cents[i] = 1200 * np.log2(ag_audio.refine_peak(row, int(k)) * sr / n / (h * fref))
    t = (np.arange(len(mag)) * hop + n / 2) / sr
    return t, cents


def vibrato_stats(t: np.ndarray, cents: np.ndarray, quiet_frac: float = 0.25) -> dict[str, float]:
    """Rate (Hz), peak depth (cents) and onset (s) of a vibrato track.

    Depth is half the 5-95 percentile spread of the steady part (frames whose local swing is at
    least ``quiet_frac`` of the largest); rate comes from the dominant peak of the detrended
    track; onset is the first time the local half-swing exceeds ``quiet_frac`` of the maximum.
    """
    dt = float(np.median(np.diff(t)))
    y = cents - np.median(cents[: max(3, int(0.2 / dt))])
    win = max(3, int(0.25 / dt))
    swing = np.array([np.ptp(y[i : i + win]) / 2 for i in range(len(y) - win)])
    if len(swing) == 0 or swing.max() <= 0:
        return {"rate_hz": 0.0, "depth_cents": 0.0, "onset_s": float("nan")}
    on = np.where(swing >= quiet_frac * swing.max())[0]
    steady = y[on[0] : on[-1] + win]
    depth = float((np.percentile(steady, 95) - np.percentile(steady, 5)) / 2)
    spec = np.abs(np.fft.rfft((steady - steady.mean()) * np.hanning(len(steady)), 1 << 14))
    fr = np.fft.rfftfreq(1 << 14, dt)
    band = (fr > 2.0) & (fr < 12.0)
    rate = float(fr[band][int(np.argmax(spec[band]))])
    return {"rate_hz": rate, "depth_cents": depth, "onset_s": float(t[on[0]] - t[0])}


def exp_decay_fit(t: np.ndarray, y: np.ndarray, floor: float = 5.0) -> tuple[float, float]:
    """Fit ``y = A exp(-t / tau)`` on the samples with ``y > floor``; returns ``(A, tau)``."""
    ok = y > floor
    if ok.sum() < 3:
        return float("nan"), float("nan")
    slope, ln_a = np.polyfit(t[ok], np.log(y[ok]), 1)
    return float(np.exp(ln_a)), float(-1.0 / slope)


def main(argv: Sequence[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("wav", type=Path)
    ap.add_argument("--start", type=float, required=True)
    ap.add_argument("--end", type=float, required=True)
    ap.add_argument("--fref", type=float, required=True, help="note frequency in Hz")
    ap.add_argument("--harmonic", type=int, default=4)
    ap.add_argument("--n", type=int, default=2048)
    a = ap.parse_args(argv)
    sr, x = ag_audio.load_wav(a.wav)
    seg = x.mean(axis=1)[int(a.start * sr) : int(a.end * sr)]
    t, c = partial_cents(seg, sr, a.fref, a.harmonic, a.n)
    print({k: round(v, 3) for k, v in vibrato_stats(t, c).items()})
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
