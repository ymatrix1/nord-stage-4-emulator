#!/usr/bin/env python3
"""Spectral tracks for the single-feature lesson videos (numpy only).

Helpers: WAV load, STFT magnitude, spectral centroid / strongest-peak tracks, parabolic
peak refinement, log-frequency spectrogram PNG. The hardware audio is lossy YouTube audio,
so every number is shape / timing evidence.
"""

from __future__ import annotations

import argparse
import wave
from pathlib import Path
from typing import Sequence

import numpy as np
from PIL import Image


def load_wav(path: Path) -> tuple[int, np.ndarray]:
    """Return ``(sample_rate, float32 array (n, channels))`` of a 16-bit PCM WAV."""
    with wave.open(str(path)) as w:
        ch, sr = w.getnchannels(), w.getframerate()
        raw = np.frombuffer(w.readframes(w.getnframes()), np.int16)
    return sr, raw.reshape(-1, ch).astype(np.float32) / 32768.0


def stft_mag(x: np.ndarray, n: int = 4096, hop: int = 480) -> np.ndarray:
    """Hann STFT magnitude, shape ``(frames, n // 2 + 1)``; frame ``i`` starts at ``i * hop``."""
    if len(x) < n:
        x = np.pad(x, (0, n - len(x)))
    win = np.hanning(n).astype(np.float32)
    idx = np.arange(0, len(x) - n + 1, hop)
    out = np.empty((len(idx), n // 2 + 1), np.float32)
    for k, s in enumerate(idx):
        out[k] = np.abs(np.fft.rfft(x[s : s + n] * win))
    return out


def centroid_track(
    mag: np.ndarray, sr: int, n: int, lo: float = 0.0, hi: float | None = None
) -> np.ndarray:
    """Spectral centroid in Hz per frame inside ``[lo, hi]``."""
    f = np.arange(mag.shape[1]) * sr / n
    sel = (f >= lo) & (f <= (hi if hi is not None else sr / 2))
    m = mag[:, sel] ** 2
    return (m * f[sel]).sum(axis=1) / np.maximum(m.sum(axis=1), 1e-20)


def refine_peak(mag_row: np.ndarray, k: int) -> float:
    """Parabolic interpolation (on log magnitude) of bin ``k``; returns fractional bin."""
    if k <= 0 or k >= len(mag_row) - 1:
        return float(k)
    a, b, c = (np.log(max(float(v), 1e-20)) for v in mag_row[k - 1 : k + 2])
    den = a - 2 * b + c
    return float(k) if den == 0 else k + 0.5 * (a - c) / den


def peak_track(mag: np.ndarray, sr: int, n: int, lo: float, hi: float) -> np.ndarray:
    """Frequency (Hz) of the strongest bin per frame inside ``[lo, hi]``, parabola refined."""
    kl, kh = int(lo * n / sr), int(hi * n / sr) + 1
    out = np.empty(len(mag), np.float64)
    for i, row in enumerate(mag):
        k = kl + int(np.argmax(row[kl:kh]))
        out[i] = refine_peak(row, k) * sr / n
    return out


def band_db(mag: np.ndarray, sr: int, n: int, lo: float, hi: float) -> np.ndarray:
    """Band level in dB (relative, sum of squared bins) per frame."""
    kl, kh = int(lo * n / sr), int(hi * n / sr) + 1
    return 10 * np.log10((mag[:, kl:kh] ** 2).sum(axis=1) + 1e-20)


def spectrogram_png(
    mag: np.ndarray,
    sr: int,
    n: int,
    hop: int,
    out: Path,
    fmax: float = 8000.0,
    floor_db: float = 70.0,
    height: int = 480,
) -> None:
    """Write a linear-frequency spectrogram (``fmax`` at the top) as a PNG."""
    kh = int(fmax * n / sr)
    db = 20 * np.log10(mag[:, :kh] + 1e-9)
    db = np.clip(db - (db.max() - floor_db), 0, floor_db) / floor_db
    img = (db.T[::-1] * 255).astype(np.uint8)
    Image.fromarray(img).resize((img.shape[1], height), Image.BILINEAR).save(out)


def main(argv: Sequence[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("wav", type=Path)
    ap.add_argument("--png", type=Path, required=True)
    ap.add_argument("--start", type=float, default=0.0)
    ap.add_argument("--end", type=float)
    ap.add_argument("--fmax", type=float, default=8000.0)
    ap.add_argument("--n", type=int, default=4096)
    ap.add_argument("--hop", type=int, default=480)
    a = ap.parse_args(argv)
    sr, x = load_wav(a.wav)
    s, e = int(a.start * sr), int((a.end or len(x) / sr) * sr)
    mag = stft_mag(x[s:e].mean(axis=1), a.n, a.hop)
    spectrogram_png(mag, sr, a.n, a.hop, a.png, a.fmax)
    print(f"{len(mag)} frames, {a.hop / sr * 1000:.1f} ms hop, {sr / a.n:.2f} Hz bins")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
