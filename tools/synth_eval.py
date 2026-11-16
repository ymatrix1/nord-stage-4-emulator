#!/usr/bin/env python3
"""Score a Transkun model on the held-out split of a ``synth_dataset.py`` dataset.

    C:/tmp/ns4-ft-venv/Scripts/python.exe tools/synth_eval.py C:/tmp/ns4-ft-data \
        --split val --out C:/tmp/ns4-ft-data/base_val.json

Metrics (pitch must match exactly, one-to-one greedy by smallest onset difference):

- onset F1 / precision / recall at +-``--tol`` seconds (default 0.05, the plan's 50 ms);
- velocity MAE and Pearson r over the matched notes (the model's expected velocity, 0..127,
  against the dataset velocity that the emulator was asked to play);
- the same split by velocity mode and by phrase kind, and, when ``velocity_response.json`` from
  ``synth_vel_check.py`` is in the dataset folder, by programs where velocity does change the
  loudness (``responsive``) and where it hardly does (``weak``).

Only the model run needs torch and transkun (``load_model``); the scoring is numpy. The base
model is the default; an adapter folder from ``synth_train.py`` is scored with ``--adapter``.
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from collections import defaultdict
from collections.abc import Sequence
from pathlib import Path
from typing import Any

import numpy as np
import synth_wav

TOL_S = 0.05
RESPONSIVE_RHO = 0.3  # same threshold as synth_vel_check.RESPONSIVE_RHO


def match(
    truth: Sequence[dict[str, Any]], est: Sequence[dict[str, Any]], tol: float = TOL_S
) -> list[tuple[int, int]]:
    """Greedy one-to-one matching of (start, pitch); returns ``(truth_idx, est_idx)`` pairs."""
    by_pitch: dict[int, list[int]] = defaultdict(list)
    for j, e in enumerate(est):
        by_pitch[e["pitch"]].append(j)
    cand = []
    for i, t in enumerate(truth):
        for j in by_pitch.get(t["pitch"], ()):
            d = abs(est[j]["start"] - t["start"])
            if d <= tol:
                cand.append((d, i, j))
    pairs, used_t, used_e = [], set(), set()
    for _, i, j in sorted(cand):
        if i not in used_t and j not in used_e:
            used_t.add(i)
            used_e.add(j)
            pairs.append((i, j))
    return pairs


class Totals:
    """Accumulates counts and velocity errors over items."""

    def __init__(self) -> None:
        self.n_truth = self.n_est = self.n_match = 0
        self.vel_truth: list[float] = []
        self.vel_est: list[float] = []

    def add(self, truth: Sequence[dict], est: Sequence[dict], tol: float) -> None:
        pairs = match(truth, est, tol)
        self.n_truth += len(truth)
        self.n_est += len(est)
        self.n_match += len(pairs)
        self.vel_truth += [truth[i]["velocity"] for i, _ in pairs]
        self.vel_est += [est[j]["velocity"] for _, j in pairs]

    def summary(self) -> dict[str, Any]:
        p = self.n_match / self.n_est if self.n_est else 0.0
        r = self.n_match / self.n_truth if self.n_truth else 0.0
        out: dict[str, Any] = {
            "notes": self.n_truth,
            "precision": round(p, 4),
            "recall": round(r, 4),
            "f1": round(2 * p * r / (p + r), 4) if p + r else 0.0,
            "velocity_mae": None,
            "velocity_r": None,
        }
        if self.vel_truth:
            a, b = np.array(self.vel_truth), np.array(self.vel_est)
            out["velocity_mae"] = round(float(np.abs(a - b).mean()), 2)
            if a.std() > 0 and b.std() > 0:
                out["velocity_r"] = round(float(np.corrcoef(a, b)[0, 1]), 4)
        return out


def response_group(program: str, response: dict[str, float] | None) -> str | None:
    """``velocity_response=responsive|weak`` from ``synth_vel_check.py``'s per-program map."""
    if not response or program not in response:
        return None
    kind = "responsive" if response[program] >= RESPONSIVE_RHO else "weak"
    return f"velocity_response={kind}"


def score(
    entries: Sequence[dict],
    estimates: dict[str, list[dict]],
    tol: float,
    response: dict[str, float] | None = None,
) -> dict[str, Any]:
    """Overall and per velocity mode / phrase kind (and velocity response) scores."""
    groups: dict[str, Totals] = defaultdict(Totals)
    for e in entries:
        est = estimates[e["id"]]
        keys = ["all", f"vel_mode={e['vel_mode']}", f"kind={e['kind']}"]
        keys += [g for g in (response_group(e.get("program", ""), response),) if g]
        for key in keys:
            groups[key].add(e["notes"], est, tol)
    return {k: v.summary() for k, v in sorted(groups.items())}


def load_response(root: Path) -> dict[str, float] | None:
    path = Path(root) / "velocity_response.json"
    return json.loads(path.read_text(encoding="utf-8")) if path.exists() else None


def load_model(device: str, adapter: str | None = None, scale: float = 1.0) -> Any:
    """Pretrained Transkun 2.0, or base plus the adapter folder (needs torch + transkun).

    ``scale`` weakens the adapter (0 = base model, 1 = as trained), see ``scale_adapter``.
    """
    if adapter is not None:
        import synth_adapter

        model = synth_adapter.load_adapter(Path(adapter), device)
        if scale != 1.0:
            synth_adapter.scale_adapter(model, scale)
        return model
    import transcribe_clip

    return transcribe_clip.load_transkun(device)


def transcribe_file(model: Any, wav: Path, device: str, keep_pedal: bool = False) -> list[dict]:
    """Model notes as dicts ``start, end, pitch, velocity``; pedal (pitch < 0) only if asked."""
    import torch

    x, rate = synth_wav.read_wav(wav)
    audio = torch.from_numpy(np.ascontiguousarray(x[:, :1], dtype=np.float32)).to(device)
    assert rate == model.fs, f"{wav}: {rate} Hz, model expects {model.fs}"
    notes = model.transcribe(audio, discardSecondHalf=False)
    return [
        {"start": n.start, "end": n.end, "pitch": n.pitch, "velocity": n.velocity}
        for n in notes
        if keep_pedal or n.pitch > 0
    ]


def main(argv: Sequence[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("root", help="Dataset folder (dataset.json, audio/).")
    p.add_argument("--split", default="val", choices=("val", "train"))
    p.add_argument("--max-items", type=int, default=0, help="0 = all items of the split.")
    p.add_argument("--tol", type=float, default=TOL_S)
    p.add_argument("--device", default="cuda")
    p.add_argument("--adapter")
    p.add_argument("--scale", type=float, default=1.0, help="Adapter strength, 0..1.")
    p.add_argument("--out")
    args = p.parse_args(argv)
    root = Path(args.root)
    doc = json.loads((root / "dataset.json").read_text(encoding="utf-8"))
    entries = [e for e in doc["entries"] if e["split"] == args.split and not e["silent"]]
    if args.max_items:
        entries = entries[: args.max_items]
    model = load_model(args.device, args.adapter, args.scale)
    t0, est = time.time(), {}
    for e in entries:
        est[e["id"]] = transcribe_file(model, root / e["audio"], args.device)
    result = {
        "split": args.split,
        "items": len(entries),
        "tol_s": args.tol,
        "seconds": round(time.time() - t0, 1),
        "scores": score(entries, est, args.tol, load_response(root)),
    }
    text = json.dumps(result, indent=1)
    if args.out:
        Path(args.out).write_text(text, encoding="utf-8")
    print(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
