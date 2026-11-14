#!/usr/bin/env python3
"""Organ presence-gap diagnostics for `emu_vs_hw.py` work directories.

Three note-independent views of an emulator WAV against the hardware clip:

* `third_band_table`: long-term power in 12 bands (100 Hz to 12 kHz), dB re the clip total. Unlike
  the 6-band score it shows where the energy sits (a 2 to 4 kHz hump versus a flat roll-off).
* `tonal_peakiness_db`: 95th percentile minus median of the per-bin level inside a band, averaged
  over the louder half of the frames. A bank that lacks harmonics (or notes) reads 8 to 14 dB where
  the hardware reads 19 to 24 dB, whatever the band level; noise, reverb and rotary smearing read
  low.
* `register_histogram`: detected notes per octave of an `events.json`, confirmed and all. It shows
  whether a replay contains the register that carries a band (the B3 solo is mostly unconfirmed).

Usage: `python tools/organ_gap.py WORKDIR/organ_b3 emu_fw_conf.wav emu_default_conf.wav`
(`hw.wav` and `events.json` are read from the same directory). Audio never leaves the directory.
"""

from __future__ import annotations

import argparse
import collections
import json
import sys
from pathlib import Path
from typing import Sequence

import numpy as np
import video_ref

EDGES_HZ = (100, 200, 300, 400, 600, 800, 1000, 1500, 2000, 3000, 4000, 6000, 8000, 12000)
PEAK_BANDS_HZ = ((100, 400), (400, 800), (800, 1500), (1500, 3000), (3000, 6000))
FFT_N = 16384
FLOOR = 1e-15


def read_mono(path: Path) -> tuple[int, np.ndarray]:
    sr, left, right = video_ref.read_wav_stereo(path)
    return sr, 0.5 * (np.asarray(left) + np.asarray(right))


def power_frames(x: np.ndarray, sr: int, n: int = FFT_N) -> tuple[np.ndarray, np.ndarray]:
    """Hann-windowed power spectra (frames x bins, 50 % overlap) and the bin frequencies."""
    w = np.hanning(n)
    frames = [np.abs(np.fft.rfft(x[i : i + n] * w)) ** 2 for i in range(0, len(x) - n + 1, n // 2)]
    if not frames:
        return np.zeros((0, n // 2 + 1)), np.fft.rfftfreq(n, 1 / sr)
    return np.asarray(frames), np.fft.rfftfreq(n, 1 / sr)


def third_band_table(x: np.ndarray, sr: int) -> list[tuple[int, int, float]]:
    """`(lo, hi, dB re total)` for the consecutive bands of `EDGES_HZ`."""
    p, f = power_frames(x, sr)
    total = p.sum() + FLOOR
    return [
        (lo, hi, float(10 * np.log10(p[:, (f >= lo) & (f < hi)].sum() / total + FLOOR)))
        for lo, hi in zip(EDGES_HZ[:-1], EDGES_HZ[1:])
    ]


def tonal_peakiness_db(x: np.ndarray, sr: int, lo: float, hi: float) -> float | None:
    """95th percentile minus median bin level (dB) in `lo..hi`, mean over the louder half of frames.

    EMPIRICAL heuristic: a harmonic line spectrum reads high, a noise-like one low (about 10 dB
    for stationary noise at this FFT size). Returns None when the clip is shorter than one frame.
    """
    p, f = power_frames(x, sr)
    if len(p) == 0:
        return None
    sel = (f >= lo) & (f < hi)
    band = 10 * np.log10(p[:, sel] / (p.sum() + FLOOR) * len(p) + FLOOR)
    peak = np.percentile(band, 95, axis=1) - np.median(band, axis=1)
    level = p[:, sel].sum(axis=1)
    loud = level >= np.median(level)
    return float(peak[loud].mean())


def register_histogram(events: Path) -> dict[str, list[tuple[int, int]]]:
    """Notes per octave (`midi // 12`) of an `events.json`: `{"all": [...], "confirmed": [...]}`."""
    ev = json.loads(events.read_text(encoding="utf-8"))["events"]
    out = {}
    for name, sel in (("all", ev), ("confirmed", [e for e in ev if e.get("confirmed", True)])):
        out[name] = sorted(collections.Counter(e["midi"] // 12 for e in sel).items())
    return out


def report(segdir: Path, names: Sequence[str]) -> str:
    """Markdown tables: band levels and peakiness of `hw.wav` and each of `names`."""
    clips = ["hw.wav", *names]
    mono = {n: read_mono(segdir / n) for n in clips}
    tables = {n: third_band_table(x, sr) for n, (sr, x) in mono.items()}
    head = "| band Hz | " + " | ".join(n.removesuffix(".wav") for n in clips) + " |"
    lines = [head, "|" + "---|" * (len(clips) + 1)]
    for i, (lo, hi, _) in enumerate(tables["hw.wav"]):
        lines.append(f"| {lo}-{hi} | " + " | ".join(f"{tables[n][i][2]:.1f}" for n in clips) + " |")
    lines += ["", "| peakiness dB | " + " | ".join(n.removesuffix(".wav") for n in clips) + " |"]
    lines.append("|" + "---|" * (len(clips) + 1))
    for lo, hi in PEAK_BANDS_HZ:
        vals = [tonal_peakiness_db(x, sr, lo, hi) for sr, x in (mono[n] for n in clips)]
        lines.append(
            f"| {lo}-{hi} | " + " | ".join("-" if v is None else f"{v:.1f}" for v in vals) + " |"
        )
    events = segdir / "events.json"
    if events.exists():
        lines += ["", f"notes per octave: {register_histogram(events)}"]
    return "\n".join(lines)


def main(argv: Sequence[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("segdir", type=Path, help="emu_vs_hw work directory of one segment")
    p.add_argument("wavs", nargs="+", help="emulator WAV names inside segdir")
    args = p.parse_args(argv)
    for name in ("hw.wav", *args.wavs):
        if not (args.segdir / name).exists():
            print(f"missing {args.segdir / name}", file=sys.stderr)
            return 2
    print(report(args.segdir, args.wavs))
    return 0


if __name__ == "__main__":
    sys.exit(main())
