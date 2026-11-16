"""Tests for tools/synth_train_data.py."""

from __future__ import annotations

import json
import sys
from pathlib import Path

import numpy as np
import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import synth_train_data as std  # noqa: E402
import synth_wav  # noqa: E402

RATE = 44100


def entry(duration=40.0, notes=None, pedal=None, eid="a", split="train", silent=False):
    return {
        "id": eid,
        "split": split,
        "silent": silent,
        "duration": duration,
        "audio": f"audio/{eid}.wav",
        "notes": notes or [],
        "pedal": pedal or [],
    }


def test_segment_notes_clip_shift_and_flags():
    e = entry(
        notes=[
            {"start": 1.0, "end": 3.0, "pitch": 60, "velocity": 50},  # before the segment
            {"start": 9.0, "end": 12.0, "pitch": 62, "velocity": 60},  # crosses the start
            {"start": 12.0, "end": 14.0, "pitch": 64, "velocity": 70},  # inside
            {"start": 19.0, "end": 22.0, "pitch": 65, "velocity": 80},  # crosses the end
            {"start": 30.0, "end": 31.0, "pitch": 67, "velocity": 90},  # after
        ]
    )
    out = std.segment_notes(e, 10.0, 20.0)
    assert [n["pitch"] for n in out] == [62, 64, 65]
    cross_in, inside, cross_out = out
    assert cross_in["start"] == 0.0 and cross_in["end"] == 2.0
    assert not cross_in["has_onset"] and cross_in["has_offset"]
    assert inside["start"] == 2.0 and inside["has_onset"] and inside["has_offset"]
    assert cross_out["end"] == 10.0 and cross_out["has_onset"] and not cross_out["has_offset"]


def test_pedal_becomes_negative_pitch_notes_sorted():
    e = entry(
        notes=[{"start": 5.0, "end": 6.0, "pitch": 60, "velocity": 50}],
        pedal=[{"start": 4.0, "end": 8.0}],
    )
    out = std.segment_notes(e, 0.0, 16.0)
    assert [(n["pitch"], n["velocity"]) for n in out] == [(-64, 127), (60, 50)]


def write_wav(path: Path, seconds: float) -> np.ndarray:
    x = (np.arange(int(RATE * seconds)) % 1000 - 500).astype(np.float32) / 1000.0
    path.parent.mkdir(parents=True, exist_ok=True)
    synth_wav.write_wav_int16(path, synth_wav.to_mono_int16(x))
    return x


def test_read_segment_exact_and_zero_padded(tmp_path):
    x = write_wav(tmp_path / "a.wav", 3.0)
    seg = std.read_segment(tmp_path / "a.wav", 1.0, 2.0)
    assert len(seg) == RATE and np.allclose(seg, x[RATE : 2 * RATE], atol=1e-3)
    pad = std.read_segment(tmp_path / "a.wav", 2.5, 4.0)
    assert len(pad) == int(1.5 * RATE)
    assert np.allclose(pad[: RATE // 2], x[-RATE // 2 :], atol=1e-3)
    assert not pad[RATE // 2 :].any()
    before = std.read_segment(tmp_path / "a.wav", -0.5, 0.5)
    assert len(before) == RATE and not before[: RATE // 2].any()


def test_load_entries_filters_split_and_silent(tmp_path):
    doc = {
        "entries": [
            entry(eid="t1"),
            entry(eid="v1", split="val"),
            entry(eid="t2", silent=True),
        ]
    }
    (tmp_path / "dataset.json").write_text(json.dumps(doc), encoding="utf-8")
    assert [e["id"] for e in std.load_entries(tmp_path, "train")] == ["t1"]
    assert [e["id"] for e in std.load_entries(tmp_path, "val")] == ["v1"]


def test_sampler_batches_are_seeded_and_shaped(tmp_path):
    ents = []
    for i in range(3):
        write_wav(tmp_path / "audio" / f"e{i}.wav", 20.0)
        ents.append(
            entry(20.0, [{"start": 2.0, "end": 3.0, "pitch": 60 + i, "velocity": 50}], eid=f"e{i}")
        )
    a = std.SegmentSampler(tmp_path, ents, seg_s=8.0, seed=3)
    b = std.SegmentSampler(tmp_path, ents, seg_s=8.0, seed=3)
    audio, notes = a.batch(2)
    assert audio.shape == (2, 8 * RATE, 1) and len(notes) == 2
    audio2, notes2 = b.batch(2)
    assert np.array_equal(audio, audio2) and notes == notes2
    fixed = a.fixed(2)
    assert len(fixed) == 2 and len(fixed[0][0]) == 8 * RATE
    assert [n for _, n in fixed] == [n for _, n in a.fixed(2)]


def test_short_files_are_padded_to_a_full_segment(tmp_path):
    write_wav(tmp_path / "audio" / "s.wav", 3.0)
    s = std.SegmentSampler(tmp_path, [entry(3.0, eid="s")], seg_s=5.0, seed=1)
    audio, _ = s.sample()
    assert len(audio) == 5 * RATE
    with pytest.raises(FileNotFoundError):
        std.SegmentSampler(tmp_path, [entry(9.0, eid="missing")], seg_s=5.0).sample()
