#!/usr/bin/env python3
"""Score Transkun (base or an adapter) on the hardware demo clips and write a replay stream.

    W=C:/tmp/ns4-muscriptor
    C:/tmp/ns4-ft-venv/Scripts/python.exe tools/synth_hw_eval.py \
        --clip synth_horizon $W/work/horizon.wav $W/vr/horizon.json 290 \
        --clip synth_leaky $W/work/leaky.wav $W/vr/leaky.json 1275 \
        --clip pianopad_grand $W/work/grandpad.wav $W/vr/grandpad.json 1171 \
        --adapter C:/tmp/ns4-ft-runs/lora1/step_3000 \
        --out lora_hw.json --stream-out lora_stream.json

Per clip: recall and precision lower bound against the video-confirmed notes at +-80 ms and
+-150 ms (``transcribe_agree.match_notes``, the numbers of ``data/muscriptor_agreement.json``).
``--stream-out`` writes one ``{"events", "pedal"}`` document in media time (clip times plus the
clip's ``at_s``) that ``tools/emu_vs_hw.py --events-from`` replays through the emulator, so the
band deltas against the hardware audio can be compared with the base model and with a uniform
velocity of 100. The clips' audio and video events are local files, never committed.
"""

from __future__ import annotations

import argparse
import json
import sys
from collections.abc import Sequence
from pathlib import Path
from typing import Any

import synth_eval
import transcribe_agree

TOLERANCES = (0.08, 0.15)
PEDAL_PITCH = -64
RECALL_GOAL = 0.40  # plan target T5: recall at +-150 ms above this on at least two of three clips


def split_notes(notes: Sequence[dict]) -> tuple[list[dict], list[dict]]:
    """Model notes to ``--events`` rows (clip time) and pedal rows (``t``, ``value``)."""
    events, pedal = [], []
    for n in sorted(notes, key=lambda n: (n["start"], n["pitch"])):
        if n["pitch"] == PEDAL_PITCH:
            pedal += [{"t": n["start"], "value": 127}, {"t": n["end"], "value": 0}]
        elif n["pitch"] > 0:
            events.append(
                {
                    "midi": n["pitch"],
                    "onset_s": round(n["start"], 4),
                    "offset_s": round(max(n["end"], n["start"] + 0.02), 4),
                    "velocity": int(round(n["velocity"])),
                }
            )
    return events, pedal


def score_clip(video_events: Path, events: Sequence[dict]) -> dict[str, Any]:
    """Agreement of clip-time ``events`` with the confirmed notes of a ``video_ref`` file."""
    vid = json.loads(video_events.read_text(encoding="utf-8"))["events"]
    confirmed = [e for e in vid if e.get("confirmed")]
    out: dict[str, Any] = {"video_confirmed": len(confirmed), "estimated": len(events)}
    for tol in TOLERANCES:
        res = transcribe_agree.match_notes(confirmed, events, tol)
        out[f"tol_{round(tol * 1000)}ms"] = {
            k: res[k] for k in ("matched_exact", "recall", "recall_pitch_class")
        } | {"precision_lower_bound": res["precision_lower_bound"]}
    return out


def merge_stream(parts: Sequence[tuple[float, list[dict], list[dict]]]) -> dict[str, Any]:
    """One media-time document from ``(at_s, events, pedal)`` per clip."""
    events, pedal = [], []
    for at_s, ev, ped in parts:
        events += [
            {**e, "onset_s": e["onset_s"] + at_s, "offset_s": e["offset_s"] + at_s} for e in ev
        ]
        pedal += [{**p, "t": p["t"] + at_s} for p in ped]
    events.sort(key=lambda e: (e["onset_s"], e["midi"]))
    pedal.sort(key=lambda p: p["t"])
    return {"source": "synth_hw_eval", "events": events, "pedal": pedal}


def passes_recall_goal(result: dict[str, Any]) -> int:
    """Number of clips whose +-150 ms recall is above the goal."""
    return sum(1 for c in result.values() if (c["tol_150ms"]["recall"] or 0.0) > RECALL_GOAL)


def main(argv: Sequence[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument(
        "--clip",
        nargs=4,
        action="append",
        required=True,
        metavar=("NAME", "WAV", "VIDEO_EVENTS", "AT_S"),
    )
    p.add_argument("--adapter", help="Adapter folder; the base model when omitted.")
    p.add_argument("--scale", type=float, default=1.0, help="Adapter strength, 0..1.")
    p.add_argument("--device", default="cuda")
    p.add_argument("--out")
    p.add_argument("--stream-out")
    args = p.parse_args(argv)
    model = synth_eval.load_model(args.device, args.adapter, args.scale)
    result: dict[str, Any] = {}
    parts = []
    for name, wav, video, at_s in args.clip:
        notes = synth_eval.transcribe_file(model, Path(wav), args.device, keep_pedal=True)
        events, pedal = split_notes(notes)
        result[name] = score_clip(Path(video), events)
        parts.append((float(at_s), events, pedal))
    summary = {"clips": result, "clips_above_goal": passes_recall_goal(result)}
    text = json.dumps(summary, indent=1)
    if args.out:
        Path(args.out).write_text(text, encoding="utf-8")
    if args.stream_out:
        doc = merge_stream(parts)
        Path(args.stream_out).write_text(json.dumps(doc), encoding="utf-8")
    print(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
