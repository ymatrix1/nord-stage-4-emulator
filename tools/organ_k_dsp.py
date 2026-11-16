#!/usr/bin/env python3
"""Pure measurement helpers for the organ level factor controls (``organ_k_controls.py``, D113).

All levels are dB re full scale unless stated. ``k_needed`` assumes the level path is linear in
``K`` (the assumption that control 2 tests). Nothing here reads files or renders audio.
"""

from __future__ import annotations

import math
from typing import Iterable, Sequence

import numpy as np

HOP_S = 0.05  # frame length of the gating masks (EMPIRICAL: 50 ms spans a few tonewheel cycles)
FLOOR_DB = 45.0  # a frame is active when within this many dB of the clip's loudest frame
RELEASE_PAD_S = 0.25  # note-active mask keeps this long after an offset (rotary / reverb tail)


def band_filter(
    x: np.ndarray,
    sr: int,
    keep: Sequence[tuple[float, float]],
    cut: Sequence[tuple[float, float]] = (),
) -> np.ndarray:
    """Zero-phase FFT band selection on the last axis: keep ``keep``, then zero ``cut``."""
    spec = np.fft.rfft(x, axis=-1)
    f = np.fft.rfftfreq(x.shape[-1], 1.0 / sr)
    m = np.zeros(len(f), dtype=bool)
    for lo, hi in keep:
        m |= (f >= lo) & (f <= hi)
    for lo, hi in cut:
        m &= ~((f >= lo) & (f <= hi))
    return np.fft.irfft(spec * m, n=x.shape[-1], axis=-1)


def rms_db(x: np.ndarray, mask: np.ndarray | None = None) -> float:
    """RMS in dB over the samples selected by ``mask`` (all samples when ``None``).

    A 2-D ``x`` (channels, samples) gives the mean power over both channels, as
    ``video_ref.analyze_wav`` does, so a wide hardware clip is not under-read by a mono downmix.
    """
    y = x if mask is None else x[..., mask]
    if y.size == 0:
        return float("nan")
    return 10.0 * math.log10(max(float(np.mean(y.astype(np.float64) ** 2)), 1e-24))


def crest_db(x: np.ndarray, mask: np.ndarray | None = None) -> float:
    """Peak over RMS in dB over the selected samples."""
    y = x if mask is None else x[..., mask]
    if y.size == 0:
        return float("nan")
    return 20.0 * math.log10(max(float(np.max(np.abs(y))), 1e-12)) - rms_db(y)


def k_needed(k_used: float, delta_db: float) -> float:
    """``K`` that zeroes ``delta_db`` (emulator minus hardware) if the path is linear in K."""
    return k_used * 10.0 ** (-delta_db / 20.0)


def spread_db(values: Iterable[float]) -> float:
    """Max over min of positive ``values`` in dB."""
    v = [x for x in values if x > 0]
    return 20.0 * math.log10(max(v) / min(v)) if len(v) > 1 else 0.0


def drive_slope(k_a: float, db_a: float, k_b: float, db_b: float) -> float:
    """``d(level dB) / d(20 log10 K)`` between two renders: 1.0 for a linear path.

    Below 1 the path compresses (soft clip, drive, compressor): the output grows slower than K.
    """
    return (db_a - db_b) / (20.0 * math.log10(k_a / k_b))


def frame_count(n: int, sr: int) -> int:
    return max(1, int(math.ceil(n / (HOP_S * sr))))


def frames_to_samples(frames: np.ndarray, n: int, sr: int) -> np.ndarray:
    """Expands a per-frame boolean mask to a per-sample mask of length ``n``."""
    hop = int(round(HOP_S * sr))
    return np.repeat(frames, hop)[:n]


def frame_db(x: np.ndarray, sr: int) -> np.ndarray:
    """Mean-square level (dB, channels averaged) of consecutive ``HOP_S`` frames."""
    hop = int(round(HOP_S * sr))
    p = (np.atleast_2d(x).astype(np.float64) ** 2).mean(axis=0)
    nf = frame_count(len(p), sr)
    pad = np.zeros(nf * hop)
    pad[: len(p)] = p
    ms = pad.reshape(nf, hop).mean(axis=1)
    return 10.0 * np.log10(np.maximum(ms, 1e-24))


def active_frames(x: np.ndarray, sr: int, floor_db: float = FLOOR_DB) -> np.ndarray:
    """Frames within ``floor_db`` of the loudest frame (silent gaps gated out)."""
    d = frame_db(x, sr)
    return d >= d.max() - floor_db


def note_frames(
    events: Sequence[dict],
    n: int,
    sr: int,
    lo: int = 0,
    hi: int = 127,
    pad_s: float = RELEASE_PAD_S,
    pure: bool = False,
) -> np.ndarray:
    """Frames where a note in ``lo..hi`` sounds (onset to offset + ``pad_s``).

    ``pure=True`` also demands that no note outside ``lo..hi`` sounds in the frame, so the same
    frames can be applied to the emulator and the hardware clip (a matched note range, since the
    hardware cannot be re-rendered without the out-of-range notes).
    """
    nf = frame_count(n, sr)
    inside = np.zeros(nf, dtype=bool)
    outside = np.zeros(nf, dtype=bool)
    for e in events:
        a = max(0, int(e["onset_s"] / HOP_S))
        b = min(nf, int(math.ceil((e["offset_s"] + pad_s) / HOP_S)))
        (inside if lo <= e["midi"] <= hi else outside)[a:b] = True
    return inside & ~outside if pure else inside


def note_stats(events: Sequence[dict], dur_s: float) -> dict[str, float]:
    """Note density (notes per second), mean duration (s) and mean concurrent notes."""
    n = len(events)
    tot = sum(e["offset_s"] - e["onset_s"] for e in events)
    vel = [e["velocity"] for e in events if "velocity" in e]
    return {
        "mean_velocity": round(sum(vel) / len(vel), 1) if vel else 0.0,
        "notes": n,
        "density_per_s": round(n / dur_s, 3) if dur_s else 0.0,
        "mean_dur_s": round(tot / n, 3) if n else 0.0,
        "mean_poly": round(tot / dur_s, 3) if dur_s else 0.0,
    }


def poly_matched_k(k_needed_t: float, poly_t: float, poly_v: float) -> float:
    """``K`` needed by a replay stream rescaled to the concurrency of a reference stream.

    Incoherent sum of notes: emulator power grows with the mean number of sounding notes, so a
    stream with fewer concurrent notes than the performance under-reads by ``sqrt(poly_v /
    poly_t)`` in amplitude and over-states ``K`` by the same factor. ASSUMED (no inter-note
    phase or Leslie/FX coupling model); the video-confirmed stream is the reference.
    """
    return k_needed_t * math.sqrt(poly_t / poly_v)
