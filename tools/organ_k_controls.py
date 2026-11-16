#!/usr/bin/env python3
"""Per-engine organ level factor ``K`` under four measurement controls (D113, OQ24).

Reads the WAVs that ``tools/emu_vs_hw.py --spec docs/research/data/organ_k_segments.json``
left in a work dir (``<seg>/hw.wav``, ``<seg>/emu_<label>.wav`` or ``..._conf.wav``, the replayed
events) and recomputes the emulator-minus-hardware level under controls that separate engine
effects from measurement effects:

1. ``band``: 160 Hz to 1.3 kHz, which drops the sub range (below note 48) and the horn hole.
2. ``slope``: d(level dB) / d(20 log10 K) and crest delta between the K = 2.8 and chain renders.
3. ``active`` / ``notes``: silent frames gated out, or only frames where a note sounds.
4. ``matched``: the same note-pure frames on the emulator AND the hardware clip.

Audio stays in the work dir (never committed); only the derived numbers go to JSON.
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path
from typing import Any

import numpy as np
import organ_k_dsp as dsp
import video_ref

BAND = ((160.0, 1300.0),)
BAND_WIDE = ((160.0, 8000.0),)
HORN_HOLE = ((1500.0, 3000.0),)
MATCH_LO = 48  # notes below this feed the sub range the zone logic drops (organ-output-level s1)
BASELINE = "broadband"


def read_stereo(path: Path) -> tuple[int, np.ndarray]:
    """``(sample rate, array of shape (2, n))``."""
    sr, left, right = video_ref.read_wav_stereo(path)
    n = min(len(left), len(right))
    return sr, np.stack([np.asarray(left[:n]), np.asarray(right[:n])])


def find_emu_wav(sdir: Path, label: str) -> Path | None:
    """The rendered WAV of a variant (``emu_<label>.wav`` or the ``_conf`` form)."""
    for name in (f"emu_{label}.wav", f"emu_{label}_conf.wav"):
        if (sdir / name).exists():
            return sdir / name
    return None


def control_masks(
    hw: np.ndarray, emu: np.ndarray, sr: int, events: list[dict[str, Any]]
) -> dict[str, tuple[Any, np.ndarray | None]]:
    """``{control: (bands spec, sample mask)}`` for the four controls on one clip pair."""
    n = hw.shape[-1]

    def smp(fr: np.ndarray) -> np.ndarray:
        return dsp.frames_to_samples(fr, n, sr)

    both = dsp.active_frames(hw, sr) & dsp.active_frames(emu, sr)
    any_note = dsp.note_frames(events, n, sr)
    pure48 = dsp.note_frames(events, n, sr, lo=MATCH_LO, pure=True)
    pure4864 = dsp.note_frames(events, n, sr, lo=MATCH_LO, hi=64, pure=True)
    return {
        BASELINE: (None, None),
        "broadband_mono": ("mono", None),
        "band": (BAND, None),
        "band_wide_nohole": ((BAND_WIDE, HORN_HOLE), None),
        "active": (BAND, smp(both)),
        "notes": (BAND, smp(any_note)),
        "matched_48up": (BAND, smp(pure48 & both)),
        "matched_48_64": (BAND, smp(pure4864 & both)),
    }


def apply_band(x: np.ndarray, sr: int, spec: Any) -> np.ndarray:
    if spec is None:
        return x
    if isinstance(spec, str):  # "mono": downmix, kept to show the width effect
        return np.repeat(x.mean(axis=0, keepdims=True), 2, axis=0)
    if isinstance(spec[0], tuple) and isinstance(spec[0][0], tuple):
        return dsp.band_filter(x, sr, spec[0], spec[1])
    return dsp.band_filter(x, sr, spec)


def measure_pair(
    hw: np.ndarray, emu: np.ndarray, sr: int, events: list[dict[str, Any]], k_used: float
) -> dict[str, dict[str, float]]:
    """Per control: emulator and hardware level, delta, ``k_needed``, crest delta, frames kept."""
    n = min(hw.shape[-1], emu.shape[-1])
    hw, emu = hw[..., :n], emu[..., :n]
    out: dict[str, dict[str, float]] = {}
    for name, (spec, mask) in control_masks(hw, emu, sr, events).items():
        h = apply_band(hw, sr, spec)
        e = apply_band(emu, sr, spec)
        e_db, h_db = dsp.rms_db(e, mask), dsp.rms_db(h, mask)
        delta = e_db - h_db
        out[name] = {
            "emu_db": round(e_db, 2),
            "hw_db": round(h_db, 2),
            "delta_db": round(delta, 2),
            "k_needed": round(dsp.k_needed(k_used, delta), 2),
            "crest_delta_db": round(dsp.crest_db(e, mask) - dsp.crest_db(h, mask), 2),
            "kept": round(float(mask.mean()), 3) if mask is not None else 1.0,
        }
    return out


def slopes(rows: dict[str, dict[str, Any]], pairs: list[dict[str, str]]) -> list[dict[str, Any]]:
    """Level slope and crest delta of each ``K = 2.8`` vs chain pair under each control."""
    res = []
    for p in pairs:
        a, b = rows[p["hi"]], rows[p["lo"]]
        for ctl, ra in a["controls"].items():
            rb = b["controls"][ctl]
            res.append(
                {
                    "pair": f"{p['hi']} vs {p['lo']}",
                    "control": ctl,
                    "slope": round(
                        dsp.drive_slope(a["k_used"], ra["emu_db"], b["k_used"], rb["emu_db"]), 3
                    ),
                    "crest_delta_emu_db": round(ra["crest_delta_db"] - rb["crest_delta_db"], 2),
                    "k_needed_gap_db": round(20.0 * math.log10(ra["k_needed"] / rb["k_needed"]), 2),
                }
            )
    return res


def run(
    workdir: Path, spec: dict[str, Any], events_name: str, confirmed_only: bool = False
) -> dict[str, Any]:
    rows: dict[str, dict[str, Any]] = {}
    stats: dict[str, Any] = {}
    for seg in spec["segments"]:
        sdir = workdir / seg["id"]
        sr, hw = read_stereo(sdir / "hw.wav")
        ev = json.loads((sdir / events_name).read_text(encoding="utf-8"))["events"]
        if confirmed_only:
            ev = [e for e in ev if e.get("confirmed", True)]
        stats[seg["id"]] = dsp.note_stats(ev, hw.shape[-1] / sr)
        for var in seg["variants"]:
            wav = find_emu_wav(sdir, var["label"])
            if "k" not in var or wav is None:
                continue
            sr_e, emu = read_stereo(wav)
            if sr_e != sr:
                raise ValueError(f"{wav}: sample rate {sr_e} != hardware {sr}")
            rows[var["label"]] = {
                "segment": seg["id"],
                "k_used": var["k"],
                "controls": measure_pair(hw, emu, sr, ev, var["k"]),
            }
    pairs = spec.get("slope_pairs", [])
    return {"rows": rows, "note_stats": stats, "slopes": slopes(rows, pairs) if pairs else []}


def spread_table(res: dict[str, Any], spec: dict[str, Any]) -> list[dict[str, Any]]:
    """Spread (dB) of ``k_needed`` over the ``spread_set`` variants under each control."""
    out = []
    for ctl in next(iter(res["rows"].values()))["controls"]:
        ks = {lab: res["rows"][lab]["controls"][ctl]["k_needed"] for lab in spec["spread_set"]}
        out.append(
            {"control": ctl, "k_needed": ks, "spread_db": round(dsp.spread_db(ks.values()), 2)}
        )
    return out


def poly_matched_table(
    res_t: dict[str, Any], res_v: dict[str, Any], spec: dict[str, Any], control: str
) -> dict[str, Any]:
    """K needed on the Transkun stream rescaled to the video-confirmed concurrency."""
    rows: dict[str, float] = {}
    for lab in spec["spread_set"]:
        seg = res_t["rows"][lab]["segment"]
        k_t = res_t["rows"][lab]["controls"][control]["k_needed"]
        rows[lab] = round(
            dsp.poly_matched_k(
                k_t, res_t["note_stats"][seg]["mean_poly"], res_v["note_stats"][seg]["mean_poly"]
            ),
            2,
        )
    return {
        "control": control,
        "k_needed": rows,
        "spread_db": round(dsp.spread_db(rows.values()), 2),
    }


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    p.add_argument("--workdir", required=True)
    p.add_argument("--spec", default="docs/research/data/organ_k_segments.json")
    p.add_argument("--events-name", default="events_transcribed.json")
    p.add_argument(
        "--confirmed-only", action="store_true", help="Keep only video-confirmed events."
    )
    p.add_argument("--vs-video", help="JSON of a video-confirmed run: adds the poly-matched K.")
    p.add_argument("--out")
    args = p.parse_args(argv)
    spec = json.loads(Path(args.spec).read_text(encoding="utf-8"))
    res = run(Path(args.workdir), spec, args.events_name, args.confirmed_only)
    if spec.get("spread_set"):
        res["spread"] = spread_table(res, spec)
    if args.vs_video:
        res_v = json.loads(Path(args.vs_video).read_text(encoding="utf-8"))
        res["poly_matched"] = [poly_matched_table(res, res_v, spec, c) for c in ("band", "active")]
    text = json.dumps(res, indent=1)
    if args.out:
        Path(args.out).write_text(text + "\n", encoding="utf-8")
    print(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
