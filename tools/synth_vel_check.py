#!/usr/bin/env python3
"""Which programs of a ``synth_dataset.py`` dataset let velocity change the rendered loudness?

    C:/tmp/ns4-ft-venv/Scripts/python.exe tools/synth_vel_check.py C:/tmp/ns4-ft-data

Per item: for notes with no other onset within ``WIN_S`` before or after, the loudness (RMS in dB)
of the first ``WIN_S`` seconds after the onset is correlated (Spearman) with the note velocity.
Per program the item correlations are averaged. A program with a low correlation has velocity
labels that the audio barely carries, so no model can learn them from the loudness alone.
Writes ``velocity_response.json`` (``{program: rho}``) into the dataset folder; ``synth_eval.py``
uses it to report velocity for responsive and weak programs separately.

The measure sees loudness only. Velocity can also change brightness or timbre, which a model may
use, so it underestimates the cue. ASSUMED: the window (0.30 s, long enough for slow attacks, a
100 ms window gave the same picture) and the ``RESPONSIVE_RHO`` threshold are choices, not fitted.
"""

from __future__ import annotations

import argparse
import json
import sys
import wave
from collections import defaultdict
from collections.abc import Sequence
from multiprocessing import Pool
from pathlib import Path
from typing import Any

import numpy as np

WIN_S = 0.30
MIN_NOTES = 8
MIN_VEL_STD = 5.0  # items played at one velocity (vel_mode uniform) carry no information
RESPONSIVE_RHO = 0.3
OUT_NAME = "velocity_response.json"


def ranks(v: Sequence[float]) -> np.ndarray:
    """Ranks with ties given their average rank."""
    x = np.asarray(v, dtype=float)
    order = np.argsort(x, kind="stable")
    _, first, count = np.unique(x[order], return_index=True, return_counts=True)
    out = np.empty(len(x))
    out[order] = np.repeat(first + (count - 1) / 2, count)
    return out


def spearman(a: Sequence[float], b: Sequence[float]) -> float:
    """Rank correlation; NaN when either side is constant."""
    ra, rb = ranks(a), ranks(b)
    if ra.std() == 0 or rb.std() == 0:
        return float("nan")
    return float(np.corrcoef(ra, rb)[0, 1])


def isolated(onsets: np.ndarray, win: float = WIN_S) -> list[int]:
    """Indices of onsets with no other onset within ``win`` seconds either side."""
    out = []
    for i, t in enumerate(onsets):
        others = np.delete(onsets, i)
        if not np.any(np.abs(others - t) < win):
            out.append(i)
    return out


def item_rho(x: np.ndarray, rate: int, notes: Sequence[dict[str, Any]]) -> float | None:
    """Velocity / loudness correlation of one rendered item, ``None`` when too little to say."""
    if len(notes) < MIN_NOTES:
        return None
    onsets = np.array([n["start"] for n in notes])
    vel = np.array([n["velocity"] for n in notes], dtype=float)
    keep = [i for i in isolated(onsets) if int((onsets[i] + WIN_S) * rate) <= len(x)]
    if len(keep) < MIN_NOTES or vel[keep].std() < MIN_VEL_STD:
        return None
    db = []
    for i in keep:
        seg = x[int(onsets[i] * rate) : int((onsets[i] + WIN_S) * rate)]
        db.append(20 * np.log10(np.sqrt(np.mean(seg.astype(np.float64) ** 2)) + 1e-9))
    rho = spearman(vel[keep], db)
    return None if rho != rho else rho


def read_mono(path: Path) -> tuple[np.ndarray, int]:
    with wave.open(str(path), "rb") as w:
        rate = w.getframerate()
        raw = w.readframes(w.getnframes())
    return np.frombuffer(raw, dtype="<i2").astype(np.float32) / 32768.0, rate


def _one(job: tuple[str, dict[str, Any]]) -> tuple[str, float | None]:
    root, e = job
    x, rate = read_mono(Path(root) / e["audio"])
    return e["program"], item_rho(x, rate, e["notes"])


def program_rho(root: Path, entries: Sequence[dict[str, Any]], workers: int) -> dict[str, float]:
    """Mean item correlation per program (programs without a usable item are left out)."""
    jobs = [(str(root), e) for e in entries]
    if workers > 1:
        with Pool(workers) as pool:
            rows = pool.map(_one, jobs, chunksize=4)
    else:
        rows = [_one(j) for j in jobs]
    per: dict[str, list[float]] = defaultdict(list)
    for program, rho in rows:
        if rho is not None:
            per[program].append(rho)
    return {k: round(float(np.mean(v)), 4) for k, v in sorted(per.items())}


def summarize(rho: dict[str, float]) -> dict[str, int]:
    v = np.array(list(rho.values()))
    return {
        "programs": len(v),
        "weak": int((v < RESPONSIVE_RHO).sum()),
        "middle": int(((v >= RESPONSIVE_RHO) & (v < 0.6)).sum()),
        "strong": int((v >= 0.6).sum()),
    }


def main(argv: Sequence[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("root", help="Dataset folder (dataset.json, audio/).")
    p.add_argument("--workers", type=int, default=8)
    args = p.parse_args(argv)
    root = Path(args.root)
    doc = json.loads((root / "dataset.json").read_text(encoding="utf-8"))
    entries = [e for e in doc["entries"] if not e["silent"]]
    rho = program_rho(root, entries, args.workers)
    (root / OUT_NAME).write_text(json.dumps(rho, indent=1), encoding="utf-8")
    print(json.dumps(summarize(rho)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
