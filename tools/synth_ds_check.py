#!/usr/bin/env python3
"""Validation and statistics for a ``tools/synth_dataset.py`` dataset.

    python tools/synth_ds_check.py C:/tmp/ns4-ft-data --sample 40 --out stats.json

Prints (and optionally writes) split hours, program coverage, velocity / pitch / duration
histograms, phrase kinds, velocity modes, pedal and augmentation shares, and silent files.

Alignment check: for a sample of items, for isolated notes (no other note within +-0.3 s) the
energy near the fundamental (STFT bin of ``midi`` Hz, 2048 window, 256 hop) must rise at the
note's onset. Reported per item: share of isolated notes whose energy in the 200 ms after the
onset exceeds the 200 ms before it by 6 dB (``rise``), and the median lag in ms from the listed
onset to the first frame at 50 % of the 300 ms peak (``lag``). Soft-attack pads have a late lag;
the check catches gross misalignment, not synth attack times.

Provenance: thresholds (6 dB, 0.3 s isolation, 50 %) are ASSUMED screening values.
"""

from __future__ import annotations

import argparse
import json
import random
import sys
from collections import Counter
from collections.abc import Sequence
from pathlib import Path
from typing import Any

import numpy as np
import synth_wav

WIN, HOP = 2048, 256
ISOLATION_S = 0.3
RISE_DB = 6.0


def midi_hz(m: int) -> float:
    return 440.0 * 2.0 ** ((m - 69) / 12.0)


def isolated(notes: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Notes with no other onset within +-ISOLATION_S (any pitch) and a length of 0.3 s or more."""
    out = []
    for i, n in enumerate(notes):
        near = any(
            j != i
            and (
                abs(m["start"] - n["start"]) < ISOLATION_S
                or m["start"] < n["start"] < m["end"] + ISOLATION_S
            )
            for j, m in enumerate(notes)
        )
        if not near and n["end"] - n["start"] >= 0.3:
            out.append(n)
    return out


def bin_energy(x: np.ndarray, midi: int, rate: int) -> np.ndarray:
    """Magnitude at the fundamental bin (and its +-1 neighbours, max) per STFT frame."""
    k = int(round(midi_hz(midi) * WIN / rate))
    frames = np.lib.stride_tricks.sliding_window_view(x, WIN)[::HOP] * np.hanning(WIN)
    spec = np.abs(np.fft.rfft(frames, axis=1))
    return spec[:, max(0, k - 1) : k + 2].max(axis=1)


def check_item(x: np.ndarray, notes: list[dict[str, Any]], rate: int) -> dict[str, Any]:
    rises, lags = [], []
    for n in isolated(notes):
        t0 = int(n["start"] * rate)
        lo, hi = max(0, t0 - rate // 3), min(len(x), t0 + int(0.6 * rate))
        if hi - lo < WIN * 2 or t0 < rate // 5:
            continue
        env = bin_energy(x[lo:hi], n["pitch"], rate)
        t = (np.arange(len(env)) * HOP + WIN // 2 + lo) / rate - n["start"]
        before, after = env[(t > -0.2) & (t < 0)], env[(t >= 0) & (t < 0.2)]
        if before.size == 0 or after.size == 0:
            continue
        ratio = 20 * np.log10((after.max() + 1e-9) / (before.max() + 1e-9))
        rises.append(ratio >= RISE_DB)
        win = env[(t >= -0.05) & (t < 0.3)]
        tw = t[(t >= -0.05) & (t < 0.3)]
        if win.size and win.max() > 0:
            lags.append(1000 * float(tw[np.argmax(win >= 0.5 * win.max())]))
    return {
        "isolated": len(rises),
        "rise": float(np.mean(rises)) if rises else None,
        "lag_ms": float(np.median(lags)) if lags else None,
    }


def hist(values: Sequence[float], edges: Sequence[float]) -> list[int]:
    return np.histogram(values, bins=edges)[0].tolist()


def dataset_stats(doc: dict[str, Any]) -> dict[str, Any]:
    ents = doc["entries"]
    notes = [n for e in ents for n in e["notes"]]
    progs = Counter(e["program"] for e in ents)
    by_split = {s: [e for e in ents if e["split"] == s] for s in ("train", "val")}
    aug = [e["augment"] for e in ents]
    return {
        "items": len(ents),
        "hours": {s: round(sum(e["duration"] for e in v) / 3600, 3) for s, v in by_split.items()},
        "programs": {s: len({e["program"] for e in v}) for s, v in by_split.items()},
        "items_per_program": {"min": min(progs.values()), "max": max(progs.values())},
        "overlap_programs": len(
            {e["program"] for e in by_split["train"]} & {e["program"] for e in by_split["val"]}
        ),
        "notes": len(notes),
        "notes_per_second": round(len(notes) / max(1e-9, sum(e["duration"] for e in ents)), 2),
        "velocity_hist_0_127_by_16": hist([n["velocity"] for n in notes], range(0, 129, 16)),
        "velocity_median": int(np.median([n["velocity"] for n in notes])),
        "pitch_hist_by_octave_from_21": hist([n["pitch"] for n in notes], range(21, 118, 12)),
        "kinds": dict(Counter(e["kind"] for e in ents)),
        "vel_modes": dict(Counter(e["vel_mode"] for e in ents)),
        "pedal_share": round(sum(bool(e["pedal"]) for e in ents) / len(ents), 3),
        "codec_share": round(sum(a["codec"] is not None for a in aug) / len(ents), 3),
        "noise_share": round(sum(a["noise_snr_db"] is not None for a in aug) / len(ents), 3),
        "silent": sum(e["silent"] for e in ents),
        "silent_programs": sorted({e["program"] for e in ents if e["silent"]}),
    }


def alignment(root: Path, doc: dict[str, Any], sample: int, seed: int) -> dict[str, Any]:
    ents = [e for e in doc["entries"] if not e["silent"]]
    picked = random.Random(seed).sample(ents, min(sample, len(ents)))
    rows = []
    for e in picked:
        x, rate = synth_wav.read_wav(root / e["audio"])
        r = check_item(x[:, 0], e["notes"], rate)
        rows.append({"id": e["id"], "program": e["program"], **r})
    rises = [r["rise"] for r in rows if r["rise"] is not None]
    lags = [r["lag_ms"] for r in rows if r["lag_ms"] is not None]
    return {
        "items_checked": len(rows),
        "median_rise_share": round(float(np.median(rises)), 3) if rises else None,
        "median_lag_ms": round(float(np.median(lags)), 1) if lags else None,
        "worst": sorted((r for r in rows if r["rise"] is not None), key=lambda r: r["rise"])[:5],
    }


def main(argv: Sequence[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("root", help="Dataset folder with dataset.json and audio/.")
    p.add_argument("--sample", type=int, default=40, help="Items for the alignment check.")
    p.add_argument("--seed", type=int, default=1)
    p.add_argument("--out", help="Write the result JSON here.")
    args = p.parse_args(argv)
    root = Path(args.root)
    doc = json.loads((root / "dataset.json").read_text(encoding="utf-8"))
    result = {
        "stats": dataset_stats(doc),
        "alignment": alignment(root, doc, args.sample, args.seed),
    }
    text = json.dumps(result, indent=1)
    if args.out:
        Path(args.out).write_text(text, encoding="utf-8")
    print(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
