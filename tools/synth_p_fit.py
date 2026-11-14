#!/usr/bin/env python3
"""Empirical fit helpers for the synth oscillator P block (`docs/research/synth-p-fit.md`).

The ORG core 2 stages are exact for any parameter block, but the block a program uses is not
reachable (`docs/research/synth-wave-models.md` s8 to s10). This module holds the measurements
used to fit it from hardware clips: harmonic levels of a steady note against the ideal series, the
stage-model level of a stage under a given block, the level-free spectral score and the analytic
gain. Nothing here reads or writes audio outside the paths it is given (clips stay local).

Subcommands:
  harmonics  WAV T0 T1 F0   harmonic levels (dB re H1) and absolute H1 (dBFS) of a steady note
  stage      NAME F0        model level of a stage (saw|pulse) at F0 Hz with the idle-like block
"""

from __future__ import annotations

import argparse
import sys
import wave
from fractions import Fraction
from pathlib import Path
from typing import Sequence

import numpy as np

SY_FRAME_RATE_HZ = 43_402.34  # VERIFIED: docs/research/dsp-frame-rate.md


def ideal_saw_db(n: int) -> np.ndarray:
    """Harmonic levels 1..n of an ideal saw, dB re H1 (`-20 log10 k`)."""
    return -20.0 * np.log10(np.arange(1, n + 1))


def ideal_square_db(n: int) -> np.ndarray:
    """Harmonic levels 1..n of an ideal square (odd harmonics `1 / k`, even ones absent)."""
    k = np.arange(1, n + 1)
    out = -20.0 * np.log10(k)
    out[k % 2 == 0] = -np.inf
    return out


def read_wav_mono(path: Path) -> tuple[int, np.ndarray]:
    """Mono mix of a 16-bit PCM WAV, scaled to +-1."""
    with wave.open(str(path)) as w:
        ch, sr = w.getnchannels(), w.getframerate()
        raw = np.frombuffer(w.readframes(w.getnframes()), np.int16)
    return sr, raw.reshape(-1, ch).astype(np.float64).mean(axis=1) / 32768.0


def harmonic_levels(
    x: np.ndarray, sr: int, f0: float, n: int = 12, cents: float = 30.0
) -> np.ndarray:
    """Absolute levels (dBFS of a sine of that amplitude) of harmonics 1..n of a steady note.

    A Hann window, zero padded FFT, the strongest bin within +-`cents` of each harmonic.
    """
    w = np.hanning(len(x))
    nfft = 1 << (int(np.ceil(np.log2(len(x)))) + 1)
    sp = np.abs(np.fft.rfft(x * w, nfft)) / (w.sum() / 2.0)
    fr = np.arange(len(sp)) * sr / nfft
    out = []
    for h in range(1, n + 1):
        lo, hi = h * f0 * 2 ** (-cents / 1200.0), h * f0 * 2 ** (cents / 1200.0)
        sel = (fr >= lo) & (fr <= hi)
        out.append(20.0 * np.log10(sp[sel].max() + 1e-12))
    return np.array(out)


def relative_db(levels: np.ndarray) -> np.ndarray:
    """Levels re the first harmonic."""
    return levels - levels[0]


def residual_db(measured: np.ndarray, ideal: np.ndarray, floor_db: float = -60.0) -> float:
    """RMS difference (dB) over harmonics 2.. whose ideal level is above `floor_db`."""
    sel = np.isfinite(ideal) & (ideal > floor_db)
    sel[0] = False
    return float(np.sqrt(np.mean((measured[sel] - ideal[sel]) ** 2)))


def spectral_score(bands: dict[str, float]) -> float:
    """Level-free score: mean |delta| (dB) of the low, mid, presence and brilliance bands."""
    keys = ("low", "mid", "pres", "bril")
    return float(np.mean([abs(bands[k]) for k in keys]))


def fit_gain_db(rms_delta_db: float) -> float:
    """Gain (dB) that cancels an emulator-minus-hardware RMS delta (the voice path is linear)."""
    return -rms_delta_db


def stage_h1_db(name: str, f_hz: float, ncalls: int = 120) -> float:
    """H1 level (dB re an ideal unit saw `2 / pi`) of a stage of `synth_wave_model` at `f_hz`.

    Uses the single-oscillator block (ratio 2 = a played pitch, own-lane weight 1, gain 1, pulse
    word for 50 %), so two stages can be compared under one block.
    """
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import synth_wave_model as m

    fn = {"saw": m.stage_saw, "pulse": m.stage_pulse}[name]

    def lane() -> m.Lane:
        return m.Lane(
            m.f2u(f_hz / SY_FRAME_RATE_HZ), 0, 0, m.f2u(0.5), 0, m.f2u(2.0), 0,
            (0, m.f2u(1.0), 0, 0), m.f2u(1.0), 0x40000000,
        )  # fmt: skip

    lanes = [lane(), lane()]
    out: list[float] = []
    for _ in range(ncalls):
        o, p1, p2 = fn(lanes)
        for i in (0, 1):
            lanes[i].phase1, lanes[i].phase2 = p1[i], p2[i]
        out.extend(float(m.ld(b)) for b in o[0])
    x = np.array(out)
    t = np.arange(len(x)) / SY_FRAME_RATE_HZ
    h1 = 2.0 * abs((x * np.exp(-2j * np.pi * f_hz * t)).mean())
    return float(20.0 * np.log10(h1 / (2.0 / np.pi)))


def exact_fraction(x: float) -> Fraction:
    """Exact rational of a float (helper for tests that compare model words)."""
    return Fraction(x)


def main(argv: Sequence[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    h = sub.add_parser("harmonics")
    h.add_argument("wav", type=Path)
    h.add_argument("t0", type=float)
    h.add_argument("t1", type=float)
    h.add_argument("f0", type=float)
    s = sub.add_parser("stage")
    s.add_argument("name", choices=("saw", "pulse"))
    s.add_argument("f0", type=float)
    a = ap.parse_args(argv)
    if a.cmd == "harmonics":
        sr, x = read_wav_mono(a.wav)
        lev = harmonic_levels(x[int(a.t0 * sr) : int(a.t1 * sr)], sr, a.f0)
        print("abs dBFS", np.round(lev, 1))
        print("rel dB  ", np.round(relative_db(lev), 1))
        print("saw residual %.2f dB" % residual_db(relative_db(lev), ideal_saw_db(len(lev))))
    else:
        print("%s H1 %.2f dB re ideal saw" % (a.name, stage_h1_db(a.name, a.f0)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
