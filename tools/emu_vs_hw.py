#!/usr/bin/env python3
"""Emulator-vs-hardware audio comparison driver (Part A demo video).

For every segment in ``docs/research/data/emu_vs_hw_segments.json`` it extracts the hardware
WAV slice and the played notes (``video_ref.py``), renders the same notes with
``ns4 render-patch`` once per variant (default engine and each opt-in firmware path), and
writes the metric rows (6-band spectrum, centroid, stereo width, RMS level, attack time) as JSON
plus a gap ranking. Audio, frames and video stay in ``--workdir`` (never committed).

Hardware audio is lossy YouTube audio of a live performance: the numbers are shape/level
evidence only (``docs/research/emulator-vs-hardware-audio.md``).
"""

from __future__ import annotations

import argparse
import json
import statistics
import subprocess
import sys
from pathlib import Path
from typing import Any, Sequence

import numpy as np
import video_ref

DEFAULT_SPEC = Path("docs/research/data/emu_vs_hw_segments.json")
BAND_NAMES = tuple(name for name, _lo, _hi in video_ref.SPECTRAL_BANDS)
# The 8..20 kHz band is excluded from ranking: YouTube audio is low-passed (hardware clip is
# about -70 dB re RMS there), so any emulator content reads as a huge false gap.
SCORE_BANDS = tuple(b for b in BAND_NAMES if not b.startswith("air"))
ATTACK_MIN_PEAK = 1e-3  # linear, about -60 dBFS: ignore onsets with no energy
ENV_SMOOTH_S = 0.002
ATTACK_WINDOW_S = (0.02, 0.25)  # search from onset - 20 ms to onset + 250 ms


def envelope(mono: np.ndarray, sr: int, smooth_s: float = ENV_SMOOTH_S) -> np.ndarray:
    """Absolute-value envelope smoothed with a moving average of ``smooth_s`` seconds."""
    n = max(1, int(round(smooth_s * sr)))
    return np.convolve(np.abs(mono), np.ones(n) / n, mode="same")


def attack_ms(mono: np.ndarray, sr: int, onsets_s: Sequence[float]) -> float | None:
    """Median 10 % to 90 % rise time (ms) of the envelope after each onset, or None.

    Onsets whose window peak is below ``ATTACK_MIN_PEAK`` are skipped. EMPIRICAL heuristic:
    overlapping notes make single values noisy, so only the median over many onsets is used.
    """
    env = envelope(mono, sr)
    pre, post = ATTACK_WINDOW_S
    out: list[float] = []
    for t0 in onsets_s:
        a = max(0, int(round((t0 - pre) * sr)))
        b = min(len(env), int(round((t0 + post) * sr)))
        if b - a < 8:
            continue
        w = env[a:b]
        peak = float(w.max())
        if peak < ATTACK_MIN_PEAK:
            continue
        i10 = int(np.argmax(w >= 0.1 * peak))
        i90 = int(np.argmax(w >= 0.9 * peak))
        out.append(max(0, i90 - i10) * 1000.0 / sr)
    return round(statistics.median(out), 1) if out else None


def read_mono(path: Path) -> tuple[int, np.ndarray]:
    sr, left, right = video_ref.read_wav_stereo(path)
    return sr, 0.5 * (np.asarray(left) + np.asarray(right))


def segment_metrics(hw_wav: Path, emu_wav: Path, onsets_s: Sequence[float]) -> dict[str, Any]:
    """Spectrum / level / width deltas (emulator minus hardware) plus attack times."""
    rep = video_ref.compare_wavs(hw_wav, emu_wav)
    sr_h, hw = read_mono(hw_wav)
    sr_e, emu = read_mono(emu_wav)
    hw_att = attack_ms(hw, sr_h, onsets_s)
    emu_att = attack_ms(emu, sr_e, onsets_s)
    d = rep["delta_emu_minus_hw"]
    return {
        "bands_delta_db": {k: d["bands_rel_db"][k] for k in BAND_NAMES},
        "centroid_delta_hz": d["spectral_centroid_hz"],
        "rms_delta_db": d["rms_dbfs"],
        "width_hw": rep["hardware"]["stereo_width_ratio"],
        "width_emu": rep["emulator"]["stereo_width_ratio"],
        "centroid_hw_hz": rep["hardware"]["spectral_centroid_hz"],
        "rms_hw_dbfs": rep["hardware"]["rms_dbfs"],
        "attack_hw_ms": hw_att,
        "attack_emu_ms": emu_att,
    }


def gap_score(m: dict[str, Any]) -> float:
    """Single number for ranking: mean |band delta| (dB) over ``SCORE_BANDS`` (no air band).

    Level, width and attack are reported but kept out of the score because the hardware
    level depends on the performance velocity and on YouTube loudness processing.
    """
    vals = [abs(m["bands_delta_db"][k]) for k in SCORE_BANDS]
    return round(sum(vals) / len(vals), 2)


def worst_band(m: dict[str, Any]) -> tuple[str, float]:
    k = max(SCORE_BANDS, key=lambda b: abs(m["bands_delta_db"][b]))
    return k, m["bands_delta_db"][k]


def load_spec(path: Path) -> dict[str, Any]:
    spec = json.loads(path.read_text(encoding="utf-8"))
    for seg in spec["segments"]:
        for key in ("id", "entry", "engine", "at_s", "duration_s", "variants"):
            if key not in seg:
                raise ValueError(f"segment {seg.get('id', '?')} lacks '{key}'")
        if not seg["variants"] or seg["variants"][0]["label"] != "default":
            raise ValueError(f"segment {seg['id']}: first variant must be 'default'")
    return spec


def render_cmd(
    ns4: str, entry: str, backup: str, events: Path, wav: Path, velocity: int, flags: Sequence[str]
) -> list[str]:
    return [
        str(Path(ns4).resolve()),  # a relative path with '/' fails in CreateProcess on Windows
        "render-patch",
        entry,
        "--backup",
        backup,
        "--events",
        str(events),
        "--velocity",
        str(velocity),
        "--wav",
        str(wav),
        *flags,
    ]


def variant_flags(var: dict[str, Any], confirmed_only: bool) -> list[str]:
    """Render flags of a variant, plus ``--confirmed-only`` when requested."""
    return [*var.get("flags", []), *(["--confirmed-only"] if confirmed_only else [])]


def apply_edits(doc: Any, edits: dict[str, Any]) -> None:
    """Sets dotted-path values (`organ.fx.reverb_enabled`, `organ.drawbars_b[3]`) in a JSON."""
    for path, value in edits.items():
        node = doc
        parts = path.replace("]", "").replace("[", ".").split(".")
        for key in parts[:-1]:
            node = node[int(key)] if isinstance(node, list) else node[key]
        last = parts[-1]
        if isinstance(node, list):
            node[int(last)] = value
        else:
            if last not in node:
                raise KeyError(f"program has no field '{path}'")
            node[last] = value


def edited_program(
    ns4: str, entry: str, backup: str, work: Path, label: str, edits: dict[str, Any]
) -> Path:
    """Extracts `entry`, applies `edits` to its JSON form and returns the rebuilt `.ns4p`."""
    exe = str(Path(ns4).resolve())
    work.mkdir(parents=True, exist_ok=True)
    src, js, out = work / "orig.ns4p", work / f"{label}.json", work / f"{label}.ns4p"
    run([exe, "backup", backup, "--extract", entry, str(src)])
    run([exe, "convert", str(src), "--to-json", str(js)])
    doc = json.loads(js.read_text(encoding="utf-8"))
    apply_edits(doc, edits)
    js.write_text(json.dumps(doc), encoding="utf-8")
    run([exe, "convert", str(js), "--to-ns4p", str(out)])
    return out


def run(cmd: Sequence[str]) -> str:
    return subprocess.run(cmd, check=True, capture_output=True, text=True).stdout


def prepare_segment(seg: dict[str, Any], video: str, work: Path, tools: Path) -> tuple[Path, Path]:
    """Extracts the hardware WAV and the played-note events once (cached in ``work``)."""
    sdir = work / seg["id"]
    sdir.mkdir(parents=True, exist_ok=True)
    wav = sdir / "hw.wav"
    events = sdir / "events.json"
    vr = [sys.executable, str(tools / "video_ref.py")]
    if not wav.exists():
        res = json.loads(
            run(
                [
                    *vr,
                    "--json",
                    "extract",
                    video,
                    "--at",
                    str(seg["at_s"]),
                    "--duration",
                    str(seg["duration_s"]),
                    "--rois",
                    "oled_zoom",
                    "--outdir",
                    str(sdir / "x"),
                ]
            )
        )
        Path(res["segments"][0]["wav"]).replace(wav)
    if not events.exists():
        run(
            [
                *vr,
                "notes",
                video,
                "--at",
                str(seg["at_s"]),
                "--duration",
                str(seg["duration_s"]),
                "--events-out",
                str(events),
            ]
        )
    return wav, events


def note_onsets(events: Path, confirmed_only: bool = False) -> list[float]:
    """Onset times of the detected notes (video-confirmed ones only when requested)."""
    ev = json.loads(events.read_text(encoding="utf-8"))["events"]
    return [e["onset_s"] for e in ev if e.get("confirmed", True) or not confirmed_only]


def run_all(args: argparse.Namespace) -> dict[str, Any]:
    spec = load_spec(Path(args.spec))
    work = Path(args.workdir)
    tools = Path(__file__).resolve().parent
    results = []
    for seg in spec["segments"]:
        if args.only and seg["id"] not in args.only:
            continue
        hw_wav, events = prepare_segment(seg, args.video, work, tools)
        onsets = note_onsets(events, args.confirmed_only)
        suffix = "_conf" if args.confirmed_only else ""
        rows = []
        for var in seg["variants"]:
            emu = work / seg["id"] / f"emu_{var['label']}{suffix}.wav"
            flags = variant_flags(var, args.confirmed_only)
            if var.get("edits"):
                prog = edited_program(
                    args.ns4,
                    seg["entry"],
                    args.backup,
                    work / seg["id"],
                    var["label"],
                    var["edits"],
                )
                flags = [*flags, "--program-file", str(prog)]
            run(
                render_cmd(
                    args.ns4,
                    seg["entry"],
                    args.backup,
                    events,
                    emu,
                    seg.get("velocity", 100),
                    flags,
                )
            )
            m = segment_metrics(hw_wav, emu, onsets)
            m.update(label=var["label"], score=gap_score(m))
            rows.append(m)
        results.append(
            {
                **{k: seg[k] for k in ("id", "entry", "engine", "at_s", "duration_s")},
                "notes": len(onsets),
                "variants": rows,
            }
        )
    return {"video": spec.get("video"), "segments": results}


def markdown_table(result: dict[str, Any]) -> str:
    head = (
        "| Segment | Variant | " + " | ".join(BAND_NAMES) + " | centroid Hz | RMS dB | "
        "width hw/emu | attack hw/emu ms | score |"
    )
    lines = [head, "|" + "---|" * (head.count("|") - 1)]
    for seg in result["segments"]:
        for v in seg["variants"]:
            b = v["bands_delta_db"]
            lines.append(
                f"| {seg['id']} | {v['label']} | "
                + " | ".join(f"{b[k]:+.1f}" for k in BAND_NAMES)
                + f" | {v['centroid_delta_hz']:+.0f} | {v['rms_delta_db']:+.1f} | "
                f"{v['width_hw']:.2f}/{v['width_emu']:.2f} | "
                f"{v['attack_hw_ms']}/{v['attack_emu_ms']} | {v['score']:.2f} |"
            )
    return "\n".join(lines)


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--video", required=True, help="Local Part A video (.webm).")
    p.add_argument("--backup", required=True, help="Factory .ns4b backup.")
    p.add_argument("--ns4", default="target/release/ns4", help="Release ns4 binary.")
    p.add_argument("--spec", default=str(DEFAULT_SPEC))
    p.add_argument("--workdir", required=True, help="Scratch dir for WAV/events (not committed).")
    p.add_argument("--out", help="Write the result JSON here.")
    p.add_argument(
        "--confirmed-only", action="store_true", help="Replay video-confirmed notes only."
    )
    p.add_argument("--only", nargs="*", help="Segment ids to run.")
    return p


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    for label, path in (("video", args.video), ("backup", args.backup)):
        if not Path(path).exists():
            print(f"missing {label}: {path} (nothing downloaded; stop)", file=sys.stderr)
            return 2
    result = run_all(args)
    if args.out:
        Path(args.out).write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(markdown_table(result))
    return 0


if __name__ == "__main__":
    sys.exit(main())
