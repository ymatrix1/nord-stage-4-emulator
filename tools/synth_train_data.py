#!/usr/bin/env python3
"""Training segments from a ``tools/synth_dataset.py`` dataset, in Transkun's convention.

Mirrors ``transkun.Data.DatasetMaestroIterator.fetchData`` with ``notesStrictlyContained=False``:
a note that overlaps the segment is kept, times are shifted to the segment start and clipped to
it, and ``has_onset`` / ``has_offset`` say whether the real onset / offset lies inside. Sustain
pedal presses become notes of pitch -64 and velocity 127, as
``transkun.Data.parseControlChangeSwitch``
builds them. Notes are sorted by ``(start, end, pitch)`` like ``parseEventAll``.

No torch here; the trainer turns the dicts into ``transkun.Data.Note`` objects.
"""

from __future__ import annotations

import json
import random
import wave
from pathlib import Path
from typing import Any

import numpy as np

SEG_S = 16.0  # the shipped conf segment length
PEDAL_PITCH = -64


def load_entries(root: Path, split: str) -> list[dict[str, Any]]:
    doc = json.loads((Path(root) / "dataset.json").read_text(encoding="utf-8"))
    return [e for e in doc["entries"] if e["split"] == split and not e["silent"]]


def read_segment(path: Path, begin: float, end: float) -> np.ndarray:
    """Float32 mono samples in ``[begin, end)`` seconds, zero padded outside the file."""
    with wave.open(str(path), "rb") as w:
        rate, n = w.getframerate(), w.getnframes()
        b, e = round(begin * rate), round(end * rate)
        lo, hi = max(b, 0), min(e, n)
        w.setpos(lo) if hi > lo else None
        raw = w.readframes(hi - lo) if hi > lo else b""
    x = np.frombuffer(raw, dtype="<i2").astype(np.float32) / 32768.0
    return np.pad(x, (max(-b, 0), max(e - n, 0)))[: e - b]


def segment_notes(entry: dict[str, Any], begin: float, end: float) -> list[dict[str, Any]]:
    """Notes and pedal presses overlapping ``[begin, end)``, shifted and clipped."""
    out = []
    items = [(n["start"], n["end"], n["pitch"], n["velocity"]) for n in entry["notes"]]
    items += [(p["start"], p["end"], PEDAL_PITCH, 127) for p in entry.get("pedal", [])]
    for s, e, pitch, vel in items:
        if e <= begin or s >= end:
            continue
        out.append(
            {
                "start": max(s, begin) - begin,
                "end": min(e, end) - begin,
                "pitch": pitch,
                "velocity": vel,
                "has_onset": s >= begin,
                "has_offset": e < end,
            }
        )
    out.sort(key=lambda n: (n["start"], n["end"], n["pitch"]))
    return out


class SegmentSampler:
    """Random segments, items weighted by duration, seeded."""

    def __init__(
        self, root: Path, entries: list[dict[str, Any]], seg_s: float = SEG_S, seed: int = 0
    ):
        self.root, self.entries, self.seg_s = Path(root), entries, seg_s
        self.rng = random.Random(seed)
        self.weights = [e["duration"] for e in entries]

    def sample(self) -> tuple[np.ndarray, list[dict[str, Any]]]:
        e = self.rng.choices(self.entries, self.weights)[0]
        begin = self.rng.uniform(0.0, max(0.0, e["duration"] - self.seg_s))
        return self.at(e, begin)

    def at(self, entry: dict[str, Any], begin: float) -> tuple[np.ndarray, list[dict[str, Any]]]:
        end = begin + self.seg_s
        audio = read_segment(self.root / entry["audio"], begin, end)
        return audio, segment_notes(entry, begin, end)

    def batch(self, size: int) -> tuple[np.ndarray, list[list[dict[str, Any]]]]:
        got = [self.sample() for _ in range(size)]
        n = min(len(a) for a, _ in got)
        return np.stack([a[:n] for a, _ in got])[:, :, None], [notes for _, notes in got]

    def fixed(self, count: int) -> list[tuple[np.ndarray, list[dict[str, Any]]]]:
        """``count`` reproducible segments (their own seed), for validation loss."""
        rng = random.Random(1234)
        picks = rng.sample(self.entries, min(count, len(self.entries)))
        return [self.at(e, rng.uniform(0.0, max(0.0, e["duration"] - self.seg_s))) for e in picks]
