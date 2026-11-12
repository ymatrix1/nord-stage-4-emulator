#!/usr/bin/env python3
"""Measure Synth scales from hardware recordings (glide, vibrato, unison, pitch excursion).

Companion of ``docs/research/synth-hardware-sweep-plan.md``. Record the Nord Stage 4 line out as
mono/stereo WAV with a *Pure Sine* oscillator and the filter off, then:

    python tools/sweep_analyze.py glide     glide_k5.wav
    python tools/sweep_analyze.py vibrato   vibrato_amt5.wav
    python tools/sweep_analyze.py unison    unison2_c6.wav --f0 1046.5
    python tools/sweep_analyze.py unison    clip.wav --f0 392 --harmonic 10
    python tools/sweep_analyze.py excursion osc_env_pitch_amt5.wav
    python tools/sweep_analyze.py level     amp_vel3_v064.wav --from 0.5 --to 2.0

All estimators are pure Python (no numpy) and assume a clean, mostly sinusoidal signal.
"""

from __future__ import annotations

import argparse
import math
import statistics
import sys
from pathlib import Path

import video_ref


def load_mono(path: Path) -> tuple[int, list[float]]:
    """Sample rate and the mono mix of a WAV file."""
    rate, left, right = video_ref.read_wav_stereo(path)
    return rate, [(a + b) * 0.5 for a, b in zip(left, right, strict=True)]


def pitch_track(
    samples: list[float], rate: int, min_hz: float = 40.0, max_hz: float = 4000.0
) -> list[tuple[float, float]]:
    """`(time_s, frequency_hz)` per period from interpolated upward zero crossings."""
    crossings: list[float] = []
    for i in range(1, len(samples)):
        a, b = samples[i - 1], samples[i]
        if a < 0.0 <= b and b != a:
            crossings.append((i - 1 + (-a) / (b - a)) / rate)
    track: list[tuple[float, float]] = []
    for t0, t1 in zip(crossings, crossings[1:]):
        f = 1.0 / (t1 - t0)
        if min_hz <= f <= max_hz:
            track.append(((t0 + t1) / 2.0, f))
    return track


def to_semitones(freq: float, ref: float) -> float:
    return 12.0 * math.log2(freq / ref)


def glide_rate(track: list[tuple[float, float]], lo: float = 0.15, hi: float = 0.85) -> dict:
    """Constant-rate glide: slope of the pitch move between `lo` and `hi` of its extent.

    Returns ``semitones_per_s``, ``sec_per_octave`` (12 / rate), and ``linearity`` (R^2 of the
    pitch-in-semitones vs time fit; ~1.0 for a constant-rate glide).
    """
    if len(track) < 8:
        raise ValueError("track too short")
    f_start = statistics.median(f for _, f in track[:3])
    f_end = statistics.median(f for _, f in track[-3:])
    total = to_semitones(f_end, f_start)
    if abs(total) < 0.5:
        raise ValueError("no glide detected (pitch moved less than half a semitone)")
    pts = []
    for t, f in track:
        frac = to_semitones(f, f_start) / total
        if lo <= frac <= hi:
            pts.append((t, to_semitones(f, f_start)))
    if len(pts) < 4:
        raise ValueError("too few points inside the glide")
    ts, ys = zip(*pts)
    mt, my = statistics.fmean(ts), statistics.fmean(ys)
    sxx = sum((t - mt) ** 2 for t in ts)
    sxy = sum((t - mt) * (y - my) for t, y in zip(ts, ys))
    slope = sxy / sxx
    ss_tot = sum((y - my) ** 2 for y in ys)
    ss_res = sum((y - (my + slope * (t - mt))) ** 2 for t, y in zip(ts, ys))
    return {
        "semitones_per_s": abs(slope),
        "sec_per_octave": 12.0 / abs(slope),
        "linearity": 1.0 - ss_res / ss_tot if ss_tot else 1.0,
        "total_semitones": total,
    }


def vibrato_stats(track: list[tuple[float, float]]) -> dict:
    """Vibrato rate (Hz) and peak depth (cents) of a sustained note."""
    if len(track) < 16:
        raise ValueError("track too short")
    mean_f = statistics.fmean(f for _, f in track)
    cents = [(t, 1200.0 * math.log2(f / mean_f)) for t, f in track]
    mean_c = statistics.fmean(c for _, c in cents)
    centred = [(t, c - mean_c) for t, c in cents]
    if max(c for _, c in centred) - min(c for _, c in centred) < 1.0:
        raise ValueError("no periodic pitch modulation found (under 0.5 cent peak depth)")
    ups = [
        t0 + (t1 - t0) * (-c0) / (c1 - c0)
        for (t0, c0), (t1, c1) in zip(centred, centred[1:])
        if c0 < 0.0 <= c1
    ]
    if len(ups) < 3:
        raise ValueError("no periodic pitch modulation found")
    period = (ups[-1] - ups[0]) / (len(ups) - 1)
    values = [c for _, c in centred]
    return {
        "rate_hz": 1.0 / period,
        "depth_cents_peak": (max(values) - min(values)) / 2.0,
        "mean_hz": mean_f,
    }


def _fft_peaks(
    samples: list[float], rate: int, f0: float, span: float, size: int
) -> list[tuple[float, float]]:
    n = min(size, len(samples))
    n = 1 << (n.bit_length() - 1)
    win = [samples[i] * (0.5 - 0.5 * math.cos(2.0 * math.pi * i / (n - 1))) for i in range(n)]
    spec = video_ref._fft_radix2(win)
    mags = [abs(c) for c in spec[: n // 2]]
    lo = max(1, int(f0 * (1.0 - span) * n / rate))
    hi = min(n // 2 - 2, int(f0 * (1.0 + span) * n / rate) + 1)
    peaks = []
    for k in range(lo + 1, hi):
        if mags[k] > mags[k - 1] and mags[k] >= mags[k + 1]:
            a, b, c = mags[k - 1], mags[k], mags[k + 1]
            denom = a - 2.0 * b + c
            shift = 0.5 * (a - c) / denom if denom else 0.0
            peaks.append(((k + shift) * rate / n, b))
    return peaks


def unison_spread(
    samples: list[float], rate: int, f0: float, span: float = 0.03, size: int = 1 << 17
) -> dict:
    """Unison detune from the spectral peaks around `f0`.

    Returns the peak list as cents relative to the strongest peak. Needs a steady sustained note
    of at least ``size / rate`` seconds (2.7 s at 48 kHz for the default 2^17).
    """
    peaks = _fft_peaks(samples, rate, f0, span, size)
    if not peaks:
        raise ValueError("no spectral peaks near f0")
    strongest = max(mag for _, mag in peaks)
    freqs = sorted(f for f, mag in peaks if mag > 0.1 * strongest)
    centre = math.sqrt(freqs[0] * freqs[-1])  # geometric middle of the outermost voices
    cents = [1200.0 * math.log2(f / centre) for f in freqs]
    return {"centre_hz": centre, "peak_cents": cents, "spread_cents": cents[-1] - cents[0]}


def pitch_excursion(track: list[tuple[float, float]]) -> dict:
    """Semitone excursion of a note relative to its final (settled) pitch."""
    if len(track) < 8:
        raise ValueError("track too short")
    settled = statistics.median(f for _, f in track[-5:])
    semis = [to_semitones(f, settled) for _, f in track]
    return {"max_semitones": max(semis), "min_semitones": min(semis), "settled_hz": settled}


def rms_dbfs(samples: list[float], rate: int, t0: float, t1: float) -> float:
    """RMS level in dBFS of the window `[t0, t1]` seconds (for velocity / level sweeps)."""
    a, b = int(t0 * rate), min(int(t1 * rate), len(samples))
    if b - a < 16:
        raise ValueError("window too short")
    mean_sq = sum(x * x for x in samples[a:b]) / (b - a)
    return 10.0 * math.log10(mean_sq) if mean_sq > 0.0 else -math.inf


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    for name in ("glide", "vibrato", "excursion"):
        sub.add_parser(name).add_argument("wav", type=Path)
    lv = sub.add_parser("level")
    lv.add_argument("wav", type=Path)
    lv.add_argument("--from", dest="t0", type=float, default=0.5, help="window start, seconds")
    lv.add_argument("--to", dest="t1", type=float, default=1.5, help="window end, seconds")
    u = sub.add_parser("unison")
    u.add_argument("wav", type=Path)
    u.add_argument("--f0", type=float, required=True, help="played note frequency in Hz")
    u.add_argument(
        "--harmonic",
        type=int,
        default=1,
        help="analyse this harmonic of f0 (detune in cents is the same, peaks spread wider)",
    )
    args = p.parse_args(argv)
    rate, mono = load_mono(args.wav)
    if args.cmd == "unison":
        result = unison_spread(mono, rate, args.f0 * args.harmonic)
    elif args.cmd == "level":
        result = {"rms_dbfs": rms_dbfs(mono, rate, args.t0, args.t1)}
    else:
        track = pitch_track(mono, rate)
        result = {"glide": glide_rate, "vibrato": vibrato_stats, "excursion": pitch_excursion}[
            args.cmd
        ](track)
    for key, value in result.items():
        print(f"{key}: {value}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
