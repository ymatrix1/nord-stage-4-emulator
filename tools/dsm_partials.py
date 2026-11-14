#!/usr/bin/env python3
"""Per-partial levels of a held organ note (DSoundman course clips and emulator renders).

Averages the power spectrum over a time window (Welch, Hann) and sums the power in a narrow
band around each drawbar footage of the played note, so a slow rotary speaker (which smears
each partial by a few Hz) does not move the number. Hardware audio is lossy YouTube audio:
the levels are shape evidence (relative dB between partials), not absolute calibration.
"""

from __future__ import annotations

import argparse
import math
from pathlib import Path
from typing import Sequence

import numpy as np
from ag_audio import load_wav

# Drawbar footage -> multiple of the played note's fundamental (B3: 8' = the note itself).
FOOTAGES: list[tuple[str, float]] = [
    ("16'", 0.5),
    ("5 1/3'", 1.5),
    ("8'", 1.0),
    ("4'", 2.0),
    ("2 2/3'", 3.0),
    ("2'", 4.0),
    ("1 3/5'", 5.0),
    ("1 1/3'", 6.0),
    ("1'", 8.0),
]


def read_wav_any(path: Path) -> tuple[int, np.ndarray]:
    """Read a PCM16 or float32 WAV (``ns4`` renders are float) as ``(sr, (n, ch))``."""
    raw = Path(path).read_bytes()
    fmt = int.from_bytes(raw[20:22], "little")
    if fmt != 3:
        return load_wav(path)
    ch = int.from_bytes(raw[22:24], "little")
    sr = int.from_bytes(raw[24:28], "little")
    pos = raw.find(b"data", 12)
    size = int.from_bytes(raw[pos + 4 : pos + 8], "little")
    data = np.frombuffer(raw[pos + 8 : pos + 8 + size], np.float32)
    return sr, data.reshape(-1, ch).copy()


def midi_hz(note: float) -> float:
    """Equal-tempered frequency of MIDI ``note`` (A4 = 69 = 440 Hz)."""
    return 440.0 * 2.0 ** ((note - 69.0) / 12.0)


def welch_power(x: np.ndarray, sr: int, nfft: int = 1 << 16) -> tuple[np.ndarray, np.ndarray]:
    """Return ``(freqs, mean power)`` of ``x`` (Hann, 50 % overlap; short input: smaller FFT)."""
    while nfft > 4096 and len(x) < nfft:
        nfft //= 2
    if len(x) < nfft:
        x = np.pad(x, (0, nfft - len(x)))
    win = np.hanning(nfft)
    acc = np.zeros(nfft // 2 + 1)
    count = 0
    for s in range(0, len(x) - nfft + 1, nfft // 2):
        acc += np.abs(np.fft.rfft(x[s : s + nfft] * win)) ** 2
        count += 1
    return np.fft.rfftfreq(nfft, 1.0 / sr), acc / max(count, 1)


def band_db(freqs: np.ndarray, power: np.ndarray, centre: float, half_bw: float) -> float:
    """Total power (dB) of the bins within ``centre +- half_bw`` Hz."""
    sel = (freqs >= centre - half_bw) & (freqs <= centre + half_bw)
    return 10.0 * math.log10(float(power[sel].sum()) + 1e-20)


def partial_levels(x: np.ndarray, sr: int, note: float, t0: float, t1: float) -> dict[str, float]:
    """dB of each footage partial of ``note`` over ``[t0, t1]`` s (power sum, +-0.8 % band)."""
    freqs, power = welch_power(x[int(t0 * sr) : int(t1 * sr)], sr)
    f0 = midi_hz(note)
    out = {}
    for name, mult in FOOTAGES:
        c = f0 * mult
        out[name] = band_db(freqs, power, c, max(6.0, 0.008 * c)) if c < 0.45 * sr else -200.0
    return out


def relative_to(levels: dict[str, float], ref: str = "8'") -> dict[str, float]:
    """Levels minus the ``ref`` partial (dB)."""
    base = levels[ref]
    return {k: v - base for k, v in levels.items()}


def main(argv: Sequence[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("wav", type=Path)
    ap.add_argument("--start", type=float, required=True)
    ap.add_argument("--end", type=float, required=True)
    ap.add_argument("--note", type=float, required=True, help="MIDI note number")
    ap.add_argument("--ref", default="8'")
    args = ap.parse_args(argv)
    sr, x = read_wav_any(args.wav)
    lv = partial_levels(x[:, 0], sr, args.note, args.start, args.end)
    rel = relative_to(lv, args.ref)
    for name, _ in FOOTAGES:
        print(f"{name:8s} {lv[name]:8.1f} dB  {rel[name]:7.1f} dB re {args.ref}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
