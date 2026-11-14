#!/usr/bin/env python3
"""Synth validation set: legacy vs strip vs strip + P on hardware clips with a synth sound.

Clips and variants live in ``docs/research/data/synth_validation.json``. Two sources:

* ``partA``: a Part A demo program (segment of ``emu_vs_hw_segments.json``): hardware slice and
  the video-confirmed notes from ``tools/emu_vs_hw.py``, rendered with ``ns4 render-patch``.
* ``ag``: a steady note of an Audible Genius / DSoundman lesson clip: hardware slice cut from the
  clip audio, rendered with the ``ag_synth`` example (key=value panel settings of the lesson).

Every variant is scored with the metrics of ``emu_vs_hw.segment_metrics`` (6-band spectrum deltas,
centroid, RMS, attack); ``summary`` compares each variant with ``legacy`` and applies the D22 bar
(mean score better, no clip worse than ``WORSE_DB`` in presence or RMS). Audio stays in
``--workdir`` (never committed); only derived numbers are written.

Usage:
    python tools/synth_validate.py run --video PartA.webm --backup factory.ns4b --workdir W \
        [--ag-dir DIR_WITH_WAVS] [--only ids] [--variants labels] [--p-table "table:shape=..."] \
        [--out result.json]
    python tools/synth_validate.py summary result.json
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import wave
from pathlib import Path
from typing import Any, Sequence

import emu_vs_hw as ev
import numpy as np

ROOT = Path(__file__).resolve().parents[1]
SPEC = ROOT / "docs" / "research" / "data" / "synth_validation.json"
SEGMENTS = ROOT / "docs" / "research" / "data" / "emu_vs_hw_segments.json"
WORSE_DB = 3.0  # D22 bar: no clip may get worse by more than this in presence or RMS
PRESENCE = "presence_1500_3000"
BRILLIANCE = "brilliance_3000_8000"


def load_spec(path: Path = SPEC) -> dict[str, Any]:
    spec = json.loads(path.read_text(encoding="utf-8"))
    labels = [v["label"] for v in spec["variants"]]
    if labels[0] != "legacy" or len(set(labels)) != len(labels):
        raise ValueError("variants must be unique and start with 'legacy'")
    for clip in spec["clips"]:
        for key in ("id", "source", "role", "wave"):
            if key not in clip:
                raise ValueError(f"clip {clip.get('id', '?')} lacks '{key}'")
        if clip["role"] not in ("fit", "holdout"):
            raise ValueError(f"clip {clip['id']}: role must be fit or holdout")
    return spec


def variant_flags(var: dict[str, Any], p_table: str | None) -> list[str]:
    """CLI flags of a variant; ``--p-table`` replaces the plain ``table`` P selection."""
    flags = list(var.get("flags", []))
    if p_table and "table" in flags:
        flags[flags.index("table")] = p_table
    return flags


def variant_ag(var: dict[str, Any], p_table: str | None) -> dict[str, str]:
    """``ag_synth`` keys of a variant (the ``p`` key follows ``--p-table`` like the flags)."""
    keys = dict(var.get("ag", {}))
    if p_table and keys.get("p") == "table":
        keys["p"] = p_table
    return keys


def cut_wav(src: Path, start_s: float, end_s: float, dst: Path) -> Path:
    """Writes the ``start_s..end_s`` slice of a 16-bit WAV (cached)."""
    if dst.exists():
        return dst
    with wave.open(str(src)) as w:
        sr, ch = w.getframerate(), w.getnchannels()
        w.setpos(int(start_s * sr))
        raw = w.readframes(int((end_s - start_s) * sr))
    dst.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(dst), "wb") as o:
        o.setnchannels(ch)
        o.setsampwidth(2)
        o.setframerate(sr)
        o.writeframes(raw)
    return dst


def metrics_row(
    hw: Path, emu: Path, onsets: Sequence[float], intervals: Sequence[tuple[float, float]] = ()
) -> dict[str, Any]:
    m = ev.segment_metrics(hw, emu, onsets, intervals)
    b = m["bands_delta_db"]
    return {
        "score": ev.gap_score(m),
        "presence_db": b[PRESENCE],
        "brilliance_db": b[BRILLIANCE],
        "rms_db": m["rms_delta_db"],
        "centroid_hz": m["centroid_delta_hz"],
        "attack_hw_ms": m["attack_hw_ms"],
        "attack_emu_ms": m["attack_emu_ms"],
        "bands_db": b,
    }


def run_part_a(clip: dict[str, Any], args: argparse.Namespace, variants: list[dict]) -> list[dict]:
    segs = {s["id"]: s for s in ev.load_spec(SEGMENTS)["segments"]}
    seg = segs[clip["segment"]]
    work = Path(args.workdir)
    hw, events = ev.prepare_segment(seg, args.video, work, Path(__file__).resolve().parent)
    onsets = ev.note_onsets(events, True)
    intervals = ev.note_intervals(events, True)
    rows = []
    for var in variants:
        emu = work / seg["id"] / f"emu_v_{var['label']}.wav"
        flags = [*variant_flags(var, args.p_table), "--confirmed-only"]
        ev.run(ev.render_cmd(args.ns4, seg["entry"], args.backup, events, emu, 100, flags))
        rows.append({"variant": var["label"], **metrics_row(hw, emu, onsets, intervals)})
    return rows


def run_ag(clip: dict[str, Any], args: argparse.Namespace, variants: list[dict]) -> list[dict]:
    work = Path(args.workdir) / clip["id"]
    src = Path(args.ag_dir) / f"{clip['clip']}.wav"
    if not src.exists():
        raise FileNotFoundError(f"{src}: extract with ffmpeg -i clip.webm -vn -ac 2 -ar 48000")
    t0, t1 = clip["window_s"]
    hw = cut_wav(src, t0, t1, work / "hw.wav")
    rows = []
    for var in variants:
        keys = {**clip["ag"], **variant_ag(var, args.p_table)}
        emu = work / f"emu_{var['label']}.wav"
        cmd = [str(Path(args.ag_synth).resolve()), str(emu)] + [f"{k}={v}" for k, v in keys.items()]
        subprocess.run(cmd, check=True, capture_output=True)
        rows.append({"variant": var["label"], **metrics_row(hw, emu, [])})
    return rows


def run_all(args: argparse.Namespace) -> dict[str, Any]:
    spec = load_spec()
    want = set(args.variants.split(",")) if args.variants else None
    variants = [v for v in spec["variants"] if want is None or v["label"] in want | {"legacy"}]
    out = []
    for clip in spec["clips"]:
        if args.only and clip["id"] not in args.only:
            continue
        fn = run_part_a if clip["source"] == "partA" else run_ag
        rows = fn(clip, args, variants)
        out.append({k: clip[k] for k in ("id", "source", "role", "wave")} | {"variants": rows})
    return {"clips": out}


def verdict(result: dict[str, Any], variant: str) -> dict[str, Any]:
    """D22 bar of ``variant`` against ``legacy`` over all clips of the result."""
    gains, worse = [], []
    for c in result["clips"]:
        rows = {r["variant"]: r for r in c["variants"]}
        if variant not in rows:
            continue
        leg, new = rows["legacy"], rows[variant]
        gains.append(leg["score"] - new["score"])
        # lesson clips fix no absolute level (two Pure Saw notes differ by 13 dB), so the RMS
        # bar applies to Part A clips only
        keys = ("presence_db", "rms_db") if c["source"] == "partA" else ("presence_db",)
        for key in keys:
            if abs(new[key]) - abs(leg[key]) > WORSE_DB:
                worse.append(f"{c['id']}:{key} {leg[key]:+.1f} -> {new[key]:+.1f}")
    n_better = sum(g > 0 for g in gains)
    return {
        "clips": len(gains),
        "mean_gain": round(float(np.mean(gains)), 2) if gains else None,
        "better": n_better,
        "worse_than_bar": worse,
        "passes": bool(gains)
        and float(np.mean(gains)) > 0
        and n_better * 2 > len(gains)
        and not worse,
    }


def table(result: dict[str, Any]) -> str:
    head = (
        "| Clip | Role | Variant | score | presence | brilliance | RMS | centroid Hz "
        "| attack hw/emu |"
    )
    lines = [head, "|" + "---|" * (head.count("|") - 1)]
    for c in result["clips"]:
        for r in c["variants"]:
            lines.append(
                f"| {c['id']} | {c['role']} | {r['variant']} | {r['score']:.2f} | "
                f"{r['presence_db']:+.1f} | {r['brilliance_db']:+.1f} | {r['rms_db']:+.1f} | "
                f"{r['centroid_hz']:+.0f} | {r['attack_hw_ms']}/{r['attack_emu_ms']} |"
            )
    return "\n".join(lines)


def summary(result: dict[str, Any]) -> str:
    labels = sorted({r["variant"] for c in result["clips"] for r in c["variants"]} - {"legacy"})
    lines = ["| Variant | clips | mean score gain | clips better | clips over the bar | D22 |"]
    lines += ["|---|---|---|---|---|---|"]
    for lab in labels:
        for scope, sel in (("all", None), ("holdout", "holdout")):
            sub = {"clips": [c for c in result["clips"] if sel is None or c["role"] == sel]}
            v = verdict(sub, lab)
            lines.append(
                f"| {lab} ({scope}) | {v['clips']} | {v['mean_gain']} | {v['better']} | "
                f"{len(v['worse_than_bar'])} {'; '.join(v['worse_than_bar'])} | "
                f"{'pass' if v['passes'] else 'fail'} |"
            )
    return "\n".join(lines)


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = p.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run", help="render and score the clips")
    r.add_argument("--video", required=True, help="Part A demo .webm")
    r.add_argument("--backup", required=True, help="factory .ns4b")
    r.add_argument("--workdir", required=True)
    r.add_argument("--ag-dir", default=".", help="directory of <clip id>.wav files")
    r.add_argument("--ns4", default="target/release/ns4")
    r.add_argument("--ag-synth", default="target/release/examples/ag_synth")
    r.add_argument("--only", nargs="*")
    r.add_argument("--variants", help="comma separated variant labels (legacy always runs)")
    r.add_argument(
        "--p-table", help='replaces "table" in a variant, e.g. "table:shape=.6/.35/2/.5/1.75"'
    )
    r.add_argument("--out")
    s = sub.add_parser("summary", help="summarise a result JSON")
    s.add_argument("result")
    return p


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    if args.cmd == "summary":
        result = json.loads(Path(args.result).read_text(encoding="utf-8"))
    else:
        result = run_all(args)
        if args.out:
            Path(args.out).write_text(json.dumps(result, indent=1) + "\n", encoding="utf-8")
        print(table(result))
    print()
    print(summary(result))
    return 0


if __name__ == "__main__":
    sys.exit(main())
