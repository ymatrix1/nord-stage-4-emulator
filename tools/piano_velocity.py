#!/usr/bin/env python3
"""Per-note replay velocity of the Velvet Grand hardware clip, from the audio.

`render-patch` replays every detected note at one fixed velocity, but the performer's velocity
changes the level (the firmware law `NpnoLevel::main_voice_q23`) and the timbre (different
velocities select different strokes, so the partial balance changes). This tool inverts both:

1. For every confirmed note onset it measures, over the first `WINDOW_S` seconds, the partial
   levels at the note's harmonics (`k * f0`, k = 1..`MAX_PARTIALS`, below `FMAX_HZ`): total partial
   level `L` (dB) and brightness `B` = upper partials (k >= 4) over lower ones (k <= 3) in dB.
   Measuring at the note's own partials keeps the other sounding notes largely out of the number.
2. It renders every distinct note at a grid of velocities with the emulator
   (`ns4 render-patch`, same program, same engine) and measures the same `L`, `B`.
3. It picks, per note, the velocity whose (`B`, `L - gain`) matches the hardware, where `gain` is
   the unknown recording gain (dB), a single offset estimated from all notes (median residual).
   Brightness does not depend on that gain; level does.

Everything is local: the hardware WAV and the events JSON come from `emu_vs_hw.py`; nothing is
committed. Findings in `docs/research/piano-note-velocity.md`.

    uv run python tools/piano_velocity.py --hw hw.wav --events events.json \
        --ns4 target/release/ns4 --backup "<factory>.ns4b" \
        --entry "Program/Bank A/Velvet Grand.ns4p" --workdir <scratch> --out-events ev_vel.json
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any, Sequence

import numpy as np
import video_ref

# EMPIRICAL (this tool, `docs/research/piano-note-velocity.md`): measurement window and partial
# set. 150 ms spans the attack and the first decay of every note; 10 partials below 8 kHz stay
# inside the YouTube low-pass (the hardware clip is -70 dB re RMS above 8 kHz).
WINDOW_S = 0.15
MAX_PARTIALS = 10
FMAX_HZ = 8000.0
# Relative half width of a partial band, with a floor so that low notes (65 Hz) still cover the
# Hann main lobe of a 150 ms window (about 13 Hz).
BAND_REL = 0.015
BAND_MIN_HZ = 8.0
# A note whose partials were already this close to the post-onset level (dB) before the onset
# is a ringing repeat or a doubled octave: its attack level is not trustworthy.
CONTAMINATION_DB = 6.0
# ASSUMED: scatter (dB) of the brightness and level measurements, used only to weigh the two
# terms of the fit. Brightness is gain-free and sharper than a level that rides on other notes.
SIGMA_B_DB = 3.0
SIGMA_L_DB = 4.0
# Reference velocity grid; the fit interpolates linearly between the points.
VELOCITY_GRID = (16, 28, 40, 52, 64, 76, 88, 100, 110, 118, 124, 127)
REF_SPACING_S = 2.0
REF_HOLD_S = 1.2
EPS = 1e-20


def midi_hz(midi: int) -> float:
    return 440.0 * 2.0 ** ((midi - 69) / 12.0)


def partial_powers(
    mono: np.ndarray, sr: int, t0: float, midi: int, win_s: float = WINDOW_S
) -> np.ndarray | None:
    """Power in the harmonic bands `k * f0` of the window `[t0, t0 + win_s)`, or None.

    Returns one value per partial below `FMAX_HZ` (at most `MAX_PARTIALS`). None when the window
    is shorter than half of `win_s`.
    """
    a = int(round(t0 * sr))
    n = int(round(win_s * sr))
    seg = np.asarray(mono[max(0, a) : a + n], dtype=float)
    if a < 0 or len(seg) < n // 2:
        return None
    nfft = 1 << int(np.ceil(np.log2(4 * len(seg))))
    spec = np.abs(np.fft.rfft(seg * np.hanning(len(seg)), nfft)) ** 2
    freqs = np.fft.rfftfreq(nfft, 1.0 / sr)
    f0 = midi_hz(midi)
    out = []
    for k in range(1, MAX_PARTIALS + 1):
        fk = k * f0
        if fk > FMAX_HZ:
            break
        half = max(BAND_REL * fk, BAND_MIN_HZ)
        sel = (freqs >= fk - half) & (freqs <= fk + half)
        out.append(float(spec[sel].sum()))
    return np.asarray(out) if out else None


def level_db(p: np.ndarray) -> float:
    """Total partial level (dB, arbitrary reference, identical for hardware and emulator)."""
    return float(10.0 * np.log10(p.sum() + EPS))


def brightness_db(p: np.ndarray) -> float | None:
    """Upper (k >= 4) over lower (k <= 3) partial power in dB; None with fewer than 4 partials."""
    if len(p) < 4:
        return None
    return float(10.0 * np.log10((p[3:].sum() + EPS) / (p[:3].sum() + EPS)))


def note_features(
    mono: np.ndarray, sr: int, t0: float, midi: int
) -> dict[str, float | bool | None] | None:
    """Level, brightness and the contamination flag of one note onset."""
    p = partial_powers(mono, sr, t0, midi)
    if p is None:
        return None
    lv = level_db(p)
    pre = partial_powers(mono, sr, t0 - WINDOW_S - 0.05, midi) if t0 > WINDOW_S + 0.05 else None
    pre_lv = level_db(pre) if pre is not None else None
    return {
        "level_db": lv,
        "bright_db": brightness_db(p),
        "contaminated": pre_lv is not None and pre_lv > lv - CONTAMINATION_DB,
    }


def mono_of(path: Path) -> tuple[int, np.ndarray]:
    sr, left, right = video_ref.read_wav_stereo(path)
    return sr, 0.5 * (np.asarray(left) + np.asarray(right))


def distinct_notes(events: Sequence[dict[str, Any]]) -> list[int]:
    return sorted({int(e["midi"]) for e in events})


def reference_events(notes: Sequence[int]) -> list[dict[str, Any]]:
    """One note after the other, `REF_SPACING_S` apart, for the reference renders."""
    return [
        {
            "midi": m,
            "onset_s": 1.0 + i * REF_SPACING_S,
            "offset_s": 1.0 + i * REF_SPACING_S + REF_HOLD_S,
        }
        for i, m in enumerate(notes)
    ]


def run(cmd: Sequence[str]) -> str:
    return subprocess.run(cmd, check=True, capture_output=True, text=True).stdout


def render_reference(
    ns4: str, entry: str, backup: str, notes: Sequence[int], velocity: int, workdir: Path
) -> Path:
    """Renders every note of `notes` once at `velocity` (cached in `workdir`)."""
    wav = workdir / f"ref_v{velocity}.wav"
    if wav.exists():
        return wav
    ev = workdir / "ref_events.json"
    ev.write_text(json.dumps({"events": reference_events(notes)}), encoding="utf-8")
    run(
        [
            str(Path(ns4).resolve()),
            "render-patch",
            entry,
            "--backup",
            backup,
            "--events",
            str(ev),
            "--velocity",
            str(velocity),
            "--wav",
            str(wav),
        ]
    )
    return wav


def reference_table(
    ns4: str, entry: str, backup: str, notes: Sequence[int], workdir: Path
) -> dict[int, dict[str, np.ndarray]]:
    """Per note: arrays over `VELOCITY_GRID` of the emulator's level and brightness (NaN: none)."""
    workdir.mkdir(parents=True, exist_ok=True)
    table = {
        m: {
            "level": np.full(len(VELOCITY_GRID), np.nan),
            "bright": np.full(len(VELOCITY_GRID), np.nan),
        }
        for m in notes
    }
    ref = reference_events(notes)
    for j, v in enumerate(VELOCITY_GRID):
        sr, mono = mono_of(render_reference(ns4, entry, backup, notes, v, workdir))
        for e in ref:
            f = note_features(mono, sr, e["onset_s"], e["midi"])
            if f is None:
                continue
            table[e["midi"]]["level"][j] = f["level_db"]
            b = f["bright_db"]
            table[e["midi"]]["bright"][j] = np.nan if b is None else b
    return table


def interp_curve(values: np.ndarray) -> np.ndarray:
    """Linear interpolation of `values` (NaN-tolerant) to velocities 1..127 (index v - 1)."""
    grid = np.asarray(VELOCITY_GRID, dtype=float)
    ok = ~np.isnan(values)
    if ok.sum() < 2:
        return np.full(127, np.nan)
    return np.interp(np.arange(1, 128), grid[ok], values[ok])


def fit_velocity(
    hw: dict[str, Any], curves: dict[str, np.ndarray], gain_db: float
) -> tuple[int, float]:
    """Velocity (1..127) and cost minimising the brightness and gain-corrected level errors."""
    cost = np.zeros(127)
    cost += ((hw["level_db"] - gain_db - curves["level"]) / SIGMA_L_DB) ** 2
    if hw["bright_db"] is not None and not np.isnan(curves["bright"]).all():
        cost += ((hw["bright_db"] - curves["bright"]) / SIGMA_B_DB) ** 2
    cost = np.where(np.isnan(cost), np.inf, cost)
    i = int(np.argmin(cost))
    return i + 1, float(cost[i])


def estimate(
    hw_feats: list[tuple[int, dict[str, Any]]],
    table: dict[int, dict[str, np.ndarray]],
    gains_db: np.ndarray | None = None,
) -> tuple[list[int], float]:
    """Per-note velocities and the recording gain (dB, hardware minus emulator scale).

    The gain is the grid value (`gains_db`, default -30..+30 dB in 0.25 dB steps) that minimises
    the summed fit cost of the clean (not contaminated) notes; every note then takes the velocity
    that fits best at that gain. Level alone cannot separate a velocity shift from a gain shift
    (about 0.4 dB per step); the brightness term is what anchors the gain.
    """
    curves = {m: {k: interp_curve(v) for k, v in t.items()} for m, t in table.items()}
    clean = [(m, f) for m, f in hw_feats if not f["contaminated"]] or list(hw_feats)
    grid = np.arange(-30.0, 30.01, 0.25) if gains_db is None else gains_db
    totals = [sum(fit_velocity(f, curves[m], g)[1] for m, f in clean) for g in grid]
    gain = float(grid[int(np.argmin(totals))])
    return [fit_velocity(f, curves[m], gain)[0] for m, f in hw_feats], gain


def annotate_events(
    events: list[dict[str, Any]],
    hw_mono: np.ndarray,
    sr: int,
    table: dict[int, dict[str, np.ndarray]],
) -> tuple[list[dict[str, Any]], float, dict[str, Any]]:
    """Copies `events` with an estimated `velocity`; returns them, the gain and a summary."""
    feats = []
    idx = []
    for i, e in enumerate(events):
        f = note_features(hw_mono, sr, float(e["onset_s"]), int(e["midi"]))
        if f is not None and int(e["midi"]) in table:
            feats.append((int(e["midi"]), f))
            idx.append(i)
    vels, gain = estimate(feats, table)
    out = [dict(e) for e in events]
    for i, v in zip(idx, vels, strict=True):
        out[i]["velocity"] = v
    arr = np.asarray(vels)
    summary = {
        "notes": len(events),
        "estimated": len(vels),
        "contaminated": sum(1 for _, f in feats if f["contaminated"]),
        "gain_db": round(gain, 2),
        "velocity_percentiles": {p: int(np.percentile(arr, p)) for p in (5, 25, 50, 75, 95)},
        "velocity_mean": round(float(arr.mean()), 1),
    }
    return out, gain, summary


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--hw", required=True, help="Hardware slice WAV (emu_vs_hw workdir hw.wav).")
    p.add_argument("--events", required=True, help="Detected note events JSON.")
    p.add_argument("--ns4", default="target/release/ns4")
    p.add_argument("--backup", required=True)
    p.add_argument("--entry", default="Program/Bank A/Velvet Grand.ns4p")
    p.add_argument("--workdir", required=True)
    p.add_argument("--out-events", required=True, help="Events JSON with per-note velocities.")
    p.add_argument("--confirmed-only", action="store_true")
    return p


def main(argv: Sequence[str] | None = None) -> int:  # pragma: no cover - thin CLI
    a = build_parser().parse_args(argv)
    doc = json.loads(Path(a.events).read_text(encoding="utf-8"))
    events = doc["events"]
    if a.confirmed_only:
        events = [e for e in events if e.get("confirmed", True)]
    sr, hw = mono_of(Path(a.hw))
    table = reference_table(a.ns4, a.entry, a.backup, distinct_notes(events), Path(a.workdir))
    out, _gain, summary = annotate_events(events, hw, sr, table)
    Path(a.out_events).write_text(json.dumps({"events": out}), encoding="utf-8")
    print(json.dumps(summary, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
