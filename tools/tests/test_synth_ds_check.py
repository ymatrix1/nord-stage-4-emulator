"""Tests for tools/synth_ds_check.py."""

from __future__ import annotations

import json
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import synth_ds_check as chk  # noqa: E402
import synth_wav  # noqa: E402

RATE = 44100


def burst_audio(notes, seconds=8.0, shift=0.0):
    t = np.arange(int(RATE * seconds)) / RATE
    x = np.zeros_like(t)
    for n in notes:
        on, off = n["start"] + shift, n["end"] + shift
        env = ((t >= on) & (t < off)) * np.minimum(1.0, (t - on) / 0.01)
        x += 0.3 * env * np.sin(2 * np.pi * chk.midi_hz(n["pitch"]) * t)
    return x.astype(np.float32)


NOTES = [
    {"start": 1.0, "end": 1.8, "pitch": 60, "velocity": 50},
    {"start": 3.0, "end": 3.8, "pitch": 67, "velocity": 60},
    {"start": 5.0, "end": 5.8, "pitch": 72, "velocity": 70},
]


def test_aligned_audio_passes_and_shifted_audio_fails():
    ok = chk.check_item(burst_audio(NOTES), NOTES, RATE)
    assert ok["isolated"] == 3 and ok["rise"] == 1.0 and abs(ok["lag_ms"]) < 60
    bad = chk.check_item(burst_audio(NOTES, shift=0.12), NOTES, RATE)
    assert bad["lag_ms"] > 80  # the energy starts 120 ms after the listed onset


def test_isolated_excludes_neighbours_and_short_notes():
    notes = [
        {"start": 1.0, "end": 2.0, "pitch": 60, "velocity": 50},
        {"start": 1.1, "end": 2.0, "pitch": 64, "velocity": 50},
        {"start": 4.0, "end": 4.1, "pitch": 60, "velocity": 50},
        {"start": 6.0, "end": 7.0, "pitch": 60, "velocity": 50},
    ]
    assert [n["start"] for n in chk.isolated(notes)] == [6.0]


def make_dataset(tmp_path):
    ents = []
    for i, split in enumerate(("train", "val")):
        wav = tmp_path / "audio" / f"i{i}.wav"
        wav.parent.mkdir(exist_ok=True)
        synth_wav.write_wav_int16(wav, synth_wav.to_mono_int16(burst_audio(NOTES)))
        ents.append(
            {
                "id": f"i{i}",
                "program": f"Program/A/P{i}.ns4p",
                "split": split,
                "kind": "melody",
                "vel_mode": "skewed",
                "audio": f"audio/i{i}.wav",
                "duration": 8.0,
                "silent": False,
                "pedal": [{"start": 1.0, "end": 2.0}] if i else [],
                "augment": {"codec": None, "noise_snr_db": 40.0},
                "notes": NOTES,
            }
        )
    doc = {"entries": ents}
    (tmp_path / "dataset.json").write_text(json.dumps(doc), encoding="utf-8")
    return doc


def test_stats_and_main(tmp_path, capsys):
    doc = make_dataset(tmp_path)
    st = chk.dataset_stats(doc)
    assert st["items"] == 2 and st["notes"] == 6 and st["overlap_programs"] == 0
    assert st["pedal_share"] == 0.5 and st["noise_share"] == 1.0 and st["silent"] == 0
    assert sum(st["velocity_hist_0_127_by_16"]) == 6
    out = tmp_path / "stats.json"
    assert chk.main([str(tmp_path), "--sample", "2", "--out", str(out)]) == 0
    res = json.loads(out.read_text(encoding="utf-8"))
    assert res["alignment"]["items_checked"] == 2 and res["alignment"]["median_rise_share"] == 1.0
    assert "stats" in capsys.readouterr().out
