"""Tests for tools/synth_hw_eval.py (scoring and stream building; the model run needs torch)."""

from __future__ import annotations

import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import synth_hw_eval as hw  # noqa: E402


def note(start, end, pitch, vel=64):
    return {"start": start, "end": end, "pitch": pitch, "velocity": vel}


def test_split_notes_separates_pedal_and_orders_events():
    notes = [note(2.0, 3.0, 64, 70.4), note(1.0, 1.0, 60), note(0.5, 4.0, -64, 127)]
    events, pedal = hw.split_notes(notes)
    assert [e["midi"] for e in events] == [60, 64]
    assert events[0]["offset_s"] == pytest.approx(1.02)  # zero-length notes get a minimum
    assert events[1]["velocity"] == 70
    assert pedal == [{"t": 0.5, "value": 127}, {"t": 4.0, "value": 0}]


def test_score_clip_counts_only_confirmed_notes(tmp_path):
    vid = {
        "events": [
            {"midi": 60, "onset_s": 1.0, "confirmed": True},
            {"midi": 62, "onset_s": 2.0, "confirmed": True},
            {"midi": 64, "onset_s": 3.0, "confirmed": False},
        ]
    }
    (tmp_path / "v.json").write_text(json.dumps(vid), encoding="utf-8")
    est = [
        {"midi": 60, "onset_s": 1.05},  # within 80 ms
        {"midi": 62, "onset_s": 2.12},  # only within 150 ms
        {"midi": 64, "onset_s": 3.0},  # not confirmed: counts as an extra estimate
    ]
    out = hw.score_clip(tmp_path / "v.json", est)
    assert out["video_confirmed"] == 2 and out["estimated"] == 3
    assert out["tol_80ms"]["recall"] == 0.5 and out["tol_150ms"]["recall"] == 1.0
    assert out["tol_150ms"]["precision_lower_bound"] == pytest.approx(0.667, abs=1e-3)


def test_merge_stream_shifts_to_media_time_and_sorts():
    a = (
        [{"midi": 60, "onset_s": 1.0, "offset_s": 2.0, "velocity": 50}],
        [{"t": 1.0, "value": 127}],
    )
    b = ([{"midi": 40, "onset_s": 0.5, "offset_s": 1.0, "velocity": 90}], [])
    doc = hw.merge_stream([(100.0, *a), (10.0, *b)])
    assert [e["midi"] for e in doc["events"]] == [40, 60]
    assert doc["events"][0]["onset_s"] == 10.5 and doc["events"][1]["offset_s"] == 102.0
    assert doc["pedal"] == [{"t": 101.0, "value": 127}]


def test_recall_goal_needs_strictly_more_than_forty_percent():
    clips = {
        "a": {"tol_150ms": {"recall": 0.41}},
        "b": {"tol_150ms": {"recall": 0.40}},
        "c": {"tol_150ms": {"recall": None}},
    }
    assert hw.passes_recall_goal(clips) == 1
