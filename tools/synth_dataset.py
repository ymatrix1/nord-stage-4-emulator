#!/usr/bin/env python3
"""Synthetic synth-transcription dataset from ``ns4 render-patch`` (Transkun adapter, Phase 1).

Plan and numbers: ``docs/research/transkun-synth-finetune-plan.md``. Two steps:

``extract``  lists the factory ``.ns4p`` programs of a backup and extracts each to a file (one
             backup read of about 9 s per program, so render from files, not from the backup).
``render``   draws seeded phrases (``synth_phrases.py``), renders them with ``ns4 render-patch``
             across worker processes, converts the 48 kHz render to mono 16-bit 44.1 kHz WAV
             (peaks above 0.95 are scaled down) and writes
             ``dataset.json``: per item the audio path, duration, notes (``start``, ``end``,
             ``pitch``, ``velocity``), program, split and the render RMS. Whole programs go to the
             ``val`` split (held out), not phrases.

Everything lands outside the repo (default ``C:/tmp/ns4-ft-data``); audio is never committed.
A re-run skips items whose WAV already exists, so a long render can be resumed.

    python tools/synth_dataset.py extract --backup <file.ns4b> --out C:/tmp/ns4-ft-data
    python tools/synth_dataset.py render --out C:/tmp/ns4-ft-data --hours 20 --workers 24
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import random
import re
import subprocess
import sys
import tempfile
from collections.abc import Callable, Sequence
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from typing import Any

import numpy as np
import synth_augment
import synth_phrases
import synth_wav

DEFAULT_NS4 = "target/release/ns4"
PEDAL_SHARE = 0.25  # ASSUMED: share of items with a sustain pedal stream
SILENT_PEAK = 1e-3  # ASSUMED: a render with raw peak under -60 dBFS is a silent/broken program
LIST_RE = re.compile(r"^(Program/.+\.ns4p) \(")
Runner = Callable[[list[str]], None]


def run_cmd(cmd: list[str]) -> None:
    subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)


def parse_program_list(text: str) -> list[str]:
    """Entry names from ``ns4 backup <b> --list ns4p`` output."""
    return [m.group(1) for line in text.splitlines() if (m := LIST_RE.match(line.strip()))]


def safe_name(entry: str) -> str:
    stem = re.sub(r"[^A-Za-z0-9]+", "_", entry.removesuffix(".ns4p")).strip("_")
    return f"{stem}_{hashlib.sha1(entry.encode()).hexdigest()[:6]}"


def split_of(entry: str, val_percent: int) -> str:
    """Hold out whole programs: stable hash of the entry name."""
    return (
        "val" if int(hashlib.sha1(entry.encode()).hexdigest(), 16) % 100 < val_percent else "train"
    )


def extract_programs(
    ns4: str, backup: str, out: Path, workers: int, run: Runner = run_cmd
) -> list[dict[str, str]]:
    listing = subprocess.run(
        [str(Path(ns4).resolve()), "backup", backup, "--list", "ns4p"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    entries = parse_program_list(listing)
    pdir = out / "programs"
    pdir.mkdir(parents=True, exist_ok=True)

    def one(entry: str) -> dict[str, str]:
        dst = pdir / f"{safe_name(entry)}.ns4p"
        if not dst.exists():
            run([str(Path(ns4).resolve()), "backup", backup, "--extract", entry, str(dst)])
        return {"entry": entry, "file": dst.name}

    with ThreadPoolExecutor(max_workers=workers) as ex:
        programs = list(ex.map(one, entries))
    (out / "programs.json").write_text(json.dumps(programs, indent=1), encoding="utf-8")
    return programs


def _vel_idx(item_seed: int) -> int:
    """Velocity mode index: 55 % skewed, 15 % uniform, 15 % loud, 15 % ramp (ASSUMED mix)."""
    r = random.Random(item_seed ^ 0x7E10).random()
    return 0 if r < 0.55 else 1 if r < 0.70 else 2 if r < 0.85 else 3


def plan_items(
    programs: Sequence[dict[str, str]], n_items: int, seed: int, val_percent: int
) -> list[dict[str, Any]]:
    """Round-robin over programs, one seed per item; ids are stable across re-runs."""
    items = []
    for i in range(n_items):
        prog = programs[i % len(programs)]
        item_seed = seed * 1_000_003 + i
        items.append(
            {
                "id": f"{safe_name(prog['entry'])}_{item_seed}",
                "program": prog["entry"],
                "program_file": prog["file"],
                "seed": item_seed,
                "split": split_of(prog["entry"], val_percent),
                "wide": (i // len(programs)) % 4 == 3,
                "vel_mode": synth_phrases.VEL_MODES[_vel_idx(item_seed)],
                "pedal": random.Random(item_seed ^ 0xBEDA1).random() < PEDAL_SHARE,
            }
        )
    return items


def notes_for_meta(events: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return [
        {"start": e["onset_s"], "end": e["offset_s"], "pitch": e["midi"], "velocity": e["velocity"]}
        for e in events
    ]


def pedal_intervals(pedal: list[dict[str, float]]) -> list[dict[str, float]]:
    """``[{"t", "value"}]`` presses to ``[{"start", "end"}]`` intervals (value >= 64 is down)."""
    out, down = [], None
    for ev in pedal:
        if ev["value"] >= 64 and down is None:
            down = ev["t"]
        elif ev["value"] < 64 and down is not None:
            out.append({"start": down, "end": ev["t"]})
            down = None
    return out


def render_audio(
    item: dict[str, Any],
    events: list,
    pedal: list,
    out: Path,
    ns4: str,
    run: Runner,
    run_aug: synth_augment.Runner,
) -> dict[str, Any]:
    """Renders, resamples, augments and writes one WAV; returns the sidecar facts."""
    plan = synth_augment.plan_augment(item["seed"], item.get("augment", True))
    with tempfile.TemporaryDirectory(prefix="ns4ds_") as tmp:
        ev, raw = Path(tmp) / "ev.json", Path(tmp) / "raw.wav"
        ev.write_text(json.dumps({"events": events, "pedal": pedal}), encoding="utf-8")
        prog = out / "programs" / item["program_file"]
        run(
            [
                str(Path(ns4).resolve()),
                "render-patch",
                str(prog),
                "--events",
                str(ev),
                "--velocity",
                "100",
                "--wav",
                str(raw),
            ]
        )
        audio, rate = synth_wav.read_wav(raw)
    mono = synth_wav.resample_fft(audio.mean(axis=1), rate, synth_wav.SAMPLE_RATE)
    facts = {
        "raw_rms": synth_wav.rms(mono),
        "raw_peak": float(np.max(np.abs(mono))) if mono.size else 0.0,
        "augment": plan,
    }
    mono = synth_augment.apply_plan(mono, synth_wav.SAMPLE_RATE, plan, item["seed"], run_aug)
    wav = out / "audio" / f"{item['id']}.wav"
    wav.parent.mkdir(parents=True, exist_ok=True)
    synth_wav.write_wav_int16(wav, synth_wav.to_mono_int16(mono))
    return facts


def render_item(
    item: dict[str, Any],
    out: Path,
    ns4: str,
    duration_s: float,
    run: Runner = run_cmd,
    run_aug: synth_augment.Runner = synth_augment.run_ffmpeg,
) -> dict[str, Any]:
    kind, events = synth_phrases.make_phrase(
        item["seed"], duration_s=duration_s, wide=item["wide"], vel_mode=item["vel_mode"]
    )
    pedal = synth_phrases.make_pedal(item["seed"], duration_s) if item["pedal"] else []
    wav = out / "audio" / f"{item['id']}.wav"
    side = out / "meta" / f"{item['id']}.json"
    if not (wav.exists() and side.exists()):
        facts = render_audio(item, events, pedal, out, ns4, run, run_aug)
        side.parent.mkdir(parents=True, exist_ok=True)
        side.write_text(json.dumps(facts), encoding="utf-8")
    facts = json.loads(side.read_text(encoding="utf-8"))
    mono, _ = synth_wav.read_wav(wav)
    return {
        "id": item["id"],
        "program": item["program"],
        "split": item["split"],
        "seed": item["seed"],
        "kind": kind,
        "vel_mode": item["vel_mode"],
        "audio": f"audio/{wav.name}",
        "duration": round(len(mono) / synth_wav.SAMPLE_RATE, 3),
        "raw_rms": round(facts["raw_rms"], 6),
        "silent": facts["raw_peak"] < SILENT_PEAK,
        "augment": facts["augment"],
        "pedal": pedal_intervals(pedal),
        "notes": notes_for_meta(events),
    }


def render_dataset(
    out: Path,
    ns4: str,
    hours: float,
    workers: int,
    seed: int,
    val_percent: int,
    phrase_s: float,
    run: Runner = run_cmd,
    run_aug: synth_augment.Runner = synth_augment.run_ffmpeg,
    augment: bool = True,
) -> dict[str, Any]:
    programs = json.loads((out / "programs.json").read_text(encoding="utf-8"))
    n_items = max(1, math.ceil(hours * 3600 / phrase_s))
    items = plan_items(programs, n_items, seed, val_percent)
    for it in items:
        it["augment"] = augment
    with ThreadPoolExecutor(max_workers=workers) as ex:
        entries = list(ex.map(lambda it: render_item(it, out, ns4, phrase_s, run, run_aug), items))
    doc = {
        "format": "ns4-synth-dataset-1",
        "sample_rate": synth_wav.SAMPLE_RATE,
        "seed": seed,
        "entries": entries,
    }
    (out / "dataset.json").write_text(json.dumps(doc), encoding="utf-8")
    return doc


def summarize(doc: dict[str, Any]) -> str:
    ents = doc["entries"]
    hours = {
        s: sum(e["duration"] for e in ents if e["split"] == s) / 3600 for s in ("train", "val")
    }
    silent = sum(e["silent"] for e in ents)
    notes = sum(len(e["notes"]) for e in ents)
    return (
        f"{len(ents)} items, train {hours['train']:.2f} h, val {hours['val']:.2f} h, "
        f"{notes} notes, {silent} silent"
    )


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = p.add_subparsers(dest="cmd", required=True)
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--out", default="C:/tmp/ns4-ft-data", help="Dataset folder (not in git).")
    common.add_argument("--ns4", default=DEFAULT_NS4, help="Release ns4 binary.")
    common.add_argument("--workers", type=int, default=8)
    ex = sub.add_parser("extract", parents=[common])
    ex.add_argument("--backup", required=True, help="Factory .ns4b backup.")
    rd = sub.add_parser("render", parents=[common])
    rd.add_argument("--hours", type=float, default=20.0)
    rd.add_argument("--seed", type=int, default=1)
    rd.add_argument("--val-percent", type=int, default=10, help="Share of programs held out.")
    rd.add_argument("--phrase-s", type=float, default=60.0)
    rd.add_argument("--no-augment", action="store_true", help="Skip tilt, codec and noise.")
    return p


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    out = Path(args.out)
    if args.cmd == "extract":
        programs = extract_programs(args.ns4, args.backup, out, args.workers)
        print(f"extracted {len(programs)} programs to {out / 'programs'}")
    else:
        doc = render_dataset(
            out,
            args.ns4,
            args.hours,
            args.workers,
            args.seed,
            args.val_percent,
            args.phrase_s,
            augment=not args.no_augment,
        )
        print(summarize(doc))
    return 0


if __name__ == "__main__":
    sys.exit(main())
