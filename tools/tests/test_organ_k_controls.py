"""Tests for tools/organ_k_controls.py using synthetic stereo WAVs."""

from __future__ import annotations

import json
import struct
import sys
import wave
from pathlib import Path
from typing import Any

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import organ_k_controls as c  # noqa: E402

SR = 16000


def write_wav(path: Path, x: np.ndarray) -> None:
    pcm = (np.clip(x, -1, 1) * 32767).astype("<i2")
    inter = np.stack([pcm, pcm], axis=1).reshape(-1)
    with wave.open(str(path), "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(struct.pack(f"<{len(inter)}h", *inter))


def tone(freq: float, amp: float, secs: float = 4.0) -> np.ndarray:
    t = np.arange(int(SR * secs)) / SR
    return amp * np.sin(2 * np.pi * freq * t)


def make_workdir(tmp: Path) -> tuple[Path, dict[str, Any]]:
    sdir = tmp / "organ_x"
    sdir.mkdir()
    write_wav(sdir / "hw.wav", tone(500, 0.1) + tone(60, 0.1))
    write_wav(sdir / "emu_x-k28.wav", tone(500, 0.1))
    write_wav(sdir / "emu_x-chain_conf.wav", tone(500, 0.05))
    ev = {"events": [{"midi": 60, "onset_s": 0.0, "offset_s": 3.5, "velocity": 64}]}
    (sdir / "events_transcribed.json").write_text(json.dumps(ev), encoding="utf-8")
    spec = {
        "segments": [
            {
                "id": "organ_x",
                "variants": [
                    {"label": "default"},
                    {"label": "x-k28", "k": 2.8},
                    {"label": "x-chain", "k": 1.4},
                ],
            }
        ],
        "slope_pairs": [{"hi": "x-k28", "lo": "x-chain"}],
        "spread_set": ["x-k28"],
    }
    return tmp, spec


def test_find_emu_wav_both_names(tmp_path: Path) -> None:
    root, _ = make_workdir(tmp_path)
    assert c.find_emu_wav(root / "organ_x", "x-k28").name == "emu_x-k28.wav"
    assert c.find_emu_wav(root / "organ_x", "x-chain").name == "emu_x-chain_conf.wav"
    assert c.find_emu_wav(root / "organ_x", "nope") is None


def test_band_control_ignores_sub_and_gives_k(tmp_path: Path) -> None:
    root, spec = make_workdir(tmp_path)
    res = c.run(root, spec, "events_transcribed.json")
    ctl = res["rows"]["x-k28"]["controls"]
    # hardware holds an extra 60 Hz tone: broadband reads low, the 160..1300 band is equal
    assert ctl["broadband"]["delta_db"] < -2.5
    assert abs(ctl["band"]["delta_db"]) < 0.1
    assert abs(ctl["band"]["k_needed"] - 2.8) < 0.05
    assert ctl["active"]["kept"] > 0.8


def test_slopes_linear_in_band(tmp_path: Path) -> None:
    root, spec = make_workdir(tmp_path)
    res = c.run(root, spec, "events_transcribed.json")
    band = next(s for s in res["slopes"] if s["control"] == "band")
    assert abs(band["slope"] - 1.0) < 0.02
    assert abs(band["k_needed_gap_db"]) < 0.1


def test_spread_and_poly_matched(tmp_path: Path) -> None:
    root, spec = make_workdir(tmp_path)
    res = c.run(root, spec, "events_transcribed.json")
    sp = c.spread_table(res, spec)
    assert {s["control"] for s in sp} >= {"broadband", "band", "matched_48up"}
    res_v = json.loads(json.dumps(res))
    res_v["note_stats"]["organ_x"]["mean_poly"] = 4 * res["note_stats"]["organ_x"]["mean_poly"]
    pm = c.poly_matched_table(res, res_v, spec, "band")
    k_band = res["rows"]["x-k28"]["controls"]["band"]["k_needed"]
    assert abs(pm["k_needed"]["x-k28"] - k_band / 2) < 0.02


def test_confirmed_only_filters_events(tmp_path: Path) -> None:
    root, spec = make_workdir(tmp_path)
    p = root / "organ_x" / "events_transcribed.json"
    ev = [
        {"midi": 60, "onset_s": 0.0, "offset_s": 1.0, "confirmed": True},
        {"midi": 61, "onset_s": 1.0, "offset_s": 2.0, "confirmed": False},
    ]
    p.write_text(json.dumps({"events": ev}), encoding="utf-8")
    res = c.run(root, spec, "events_transcribed.json", confirmed_only=True)
    assert res["note_stats"]["organ_x"]["notes"] == 1


def test_main_writes_json(tmp_path: Path) -> None:
    root, spec = make_workdir(tmp_path)
    sp = tmp_path / "spec.json"
    sp.write_text(json.dumps(spec), encoding="utf-8")
    out = tmp_path / "o.json"
    rc = c.main(["--workdir", str(root), "--spec", str(sp), "--out", str(out)])
    assert rc == 0 and "spread" in json.loads(out.read_text(encoding="utf-8"))
