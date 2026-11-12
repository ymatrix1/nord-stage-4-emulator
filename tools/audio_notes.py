"""Polyphonic note detection (harmonic salience) for keyboard reference recordings.

Used by ``video_ref.py notes`` to find which keys are played in a hardware clip so the emulator
can be
rendered with the same notes (instead of one fixed chord).

Method (EMPIRICAL / heuristic, validated only on synthetic tones in
``tools/tests/test_audio_notes.py``):
Hann STFT, per-note harmonic-sum salience over MIDI 28..100 (Nord Stage 4 Compact 73 spans E1..E7),
iterative pick-and-suppress for up to ``max_poly`` notes per frame, then temporal smoothing into
note events. Expect octave and low-register errors on real recordings; treat results as a note
*set* estimate, not a transcription.
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Any

import numpy as np

MIDI_LO = 28
MIDI_HI = 100
HARMONICS = 8
HARMONIC_DECAY = 0.82


def midi_to_hz(midi: float) -> float:
    return 440.0 * 2.0 ** ((midi - 69.0) / 12.0)


@dataclass(frozen=True)
class NoteEvent:
    midi: int
    onset_s: float
    offset_s: float
    strength: float

    def to_dict(self) -> dict[str, Any]:
        return {
            "midi": self.midi,
            "onset_s": round(self.onset_s, 3),
            "offset_s": round(self.offset_s, 3),
            "strength": round(self.strength, 3),
        }


def _bin_tables(sr: int, nfft: int) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Returns (lo, hi, weight) bin ranges ``[lo, hi]`` for each (note, harmonic) pair."""
    notes = MIDI_HI - MIDI_LO + 1
    lo = np.zeros((notes, HARMONICS), dtype=np.int64)
    hi = np.zeros((notes, HARMONICS), dtype=np.int64)
    wt = np.zeros((notes, HARMONICS), dtype=np.float64)
    nyq_bin = nfft // 2 - 1
    for i in range(notes):
        f0 = midi_to_hz(MIDI_LO + i)
        for h in range(HARMONICS):
            f = f0 * (h + 1)
            if f > 0.45 * sr:
                continue
            # +-35 cents search window
            f_lo = f * 2.0 ** (-35.0 / 1200.0)
            f_hi = f * 2.0 ** (35.0 / 1200.0)
            b_lo = int(math.floor(f_lo * nfft / sr))
            b_hi = int(math.ceil(f_hi * nfft / sr))
            lo[i, h] = max(1, min(nyq_bin, b_lo))
            hi[i, h] = max(lo[i, h], min(nyq_bin, b_hi))
            wt[i, h] = HARMONIC_DECAY**h
    return lo, hi, wt


def _salience(mag: np.ndarray, lo: np.ndarray, hi: np.ndarray, wt: np.ndarray) -> np.ndarray:
    notes = lo.shape[0]
    sal = np.zeros(notes, dtype=np.float64)
    for i in range(notes):
        total = 0.0
        for h in range(HARMONICS):
            if wt[i, h] == 0.0:
                continue
            total += wt[i, h] * float(mag[lo[i, h] : hi[i, h] + 1].max())
        sal[i] = total
    return sal


def detect_frame_notes(
    mag: np.ndarray,
    tables: tuple[np.ndarray, np.ndarray, np.ndarray],
    max_poly: int = 8,
    rel_threshold: float = 0.30,
    abs_floor: float = 0.0,
) -> list[tuple[int, float]]:
    """Picks up to ``max_poly`` notes from one compressed magnitude spectrum."""
    lo, hi, wt = tables
    work = mag.copy()
    picked: list[tuple[int, float]] = []
    first = 0.0
    for _ in range(max_poly):
        sal = _salience(work, lo, hi, wt)
        i = int(sal.argmax())
        s = float(sal[i])
        if s <= abs_floor:
            break
        if not picked:
            first = s
        elif s < rel_threshold * first:
            break
        # Fundamental (or 2nd harmonic) must be present to avoid sub-octave ghosts
        f1 = float(work[lo[i, 0] : hi[i, 0] + 1].max()) if wt[i, 0] else 0.0
        f2 = float(work[lo[i, 1] : hi[i, 1] + 1].max()) if wt[i, 1] else 0.0
        if max(f1, f2) < 0.25 * s / max(1e-12, wt[i, 0] + wt[i, 1] + 1e-12):
            work[lo[i, 0] : hi[i, 0] + 1] *= 0.0
            continue
        picked.append((MIDI_LO + i, s))
        for h in range(HARMONICS):
            if wt[i, h]:
                work[lo[i, h] : hi[i, h] + 1] *= 0.15
    return picked


def transcribe(
    mono: np.ndarray,
    sr: int,
    nfft: int = 8192,
    hop: int = 2048,
    max_poly: int = 8,
    min_frames: int = 3,
    gap_frames: int = 2,
) -> list[NoteEvent]:
    """Detects note events in a mono signal (float, ``[-1, 1]``)."""
    mono = np.asarray(mono, dtype=np.float64)
    if mono.size < nfft:
        mono = np.pad(mono, (0, nfft - mono.size))
    window = np.hanning(nfft)
    tables = _bin_tables(sr, nfft)
    n_frames = 1 + (mono.size - nfft) // hop
    frames: list[list[tuple[int, float]]] = []
    # Global magnitude scale: ignore frames far below the loudest one (silence / decay tails)
    mags = np.empty((n_frames, nfft // 2 + 1), dtype=np.float64)
    for f in range(n_frames):
        seg = mono[f * hop : f * hop + nfft] * window
        mags[f] = np.sqrt(np.abs(np.fft.rfft(seg)))  # sqrt compression
    peak = float(mags.max()) or 1.0
    floor = 0.02 * peak
    for f in range(n_frames):
        if float(mags[f].max()) < 0.12 * peak:
            frames.append([])
            continue
        frames.append(detect_frame_notes(mags[f], tables, max_poly=max_poly, abs_floor=floor))

    # Temporal smoothing: fill short gaps, drop notes shorter than ``min_frames``
    events: list[NoteEvent] = []
    active: dict[int, list[Any]] = {}  # midi -> [start_frame, last_frame, strength_sum, count]
    for f, notes in enumerate(frames + [[]] * (gap_frames + 1)):
        present = {m: s for m, s in notes}
        for m in list(active):
            if m in present:
                continue
            if f - active[m][1] > gap_frames:
                start, last, ssum, cnt = active.pop(m)
                if last - start + 1 >= min_frames:
                    events.append(
                        NoteEvent(
                            m,
                            start * hop / sr,
                            (last * hop + nfft) / sr,
                            ssum / max(1, cnt),
                        )
                    )
        for m, s in present.items():
            if m in active:
                active[m][1] = f
                active[m][2] += s
                active[m][3] += 1
            else:
                active[m] = [f, f, s, 1]
    events.sort(key=lambda e: (e.onset_s, e.midi))
    return events


def summarize_events(events: list[NoteEvent], top: int = 6) -> dict[str, Any]:
    """Note histogram (by active time), register percentiles, and a suggested chord note set."""
    if not events:
        return {"events": 0, "histogram": {}, "register": {}, "suggested_notes": []}
    dur: dict[int, float] = {}
    for e in events:
        dur[e.midi] = dur.get(e.midi, 0.0) + (e.offset_s - e.onset_s)
    total = sum(dur.values()) or 1.0
    ordered = sorted(dur.items(), key=lambda kv: -kv[1])
    weights: list[float] = []
    notes: list[int] = []
    for m, d in sorted(dur.items()):
        notes.append(m)
        weights.append(d)
    cum = np.cumsum(weights) / sum(weights)

    def pct(p: float) -> int:
        return int(notes[int(np.searchsorted(cum, p))])

    return {
        "events": len(events),
        "histogram": {str(m): round(d / total, 4) for m, d in sorted(dur.items())},
        "register": {"p10": pct(0.10), "p50": pct(0.50), "p90": pct(0.90)},
        "suggested_notes": sorted(m for m, _ in ordered[:top]),
    }
