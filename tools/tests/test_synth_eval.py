"""Tests for tools/synth_eval.py (scoring only; the model run needs torch and transkun)."""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import synth_eval as ev  # noqa: E402


def note(start, pitch, vel=60):
    return {"start": start, "end": start + 0.5, "pitch": pitch, "velocity": vel}


def test_match_is_one_to_one_same_pitch_within_tolerance():
    truth = [note(1.0, 60), note(2.0, 62)]
    est = [note(1.04, 60), note(1.02, 60), note(2.2, 62), note(2.0, 63)]
    pairs = ev.match(truth, est, 0.05)
    assert pairs == [(0, 1)]  # closest est wins; second est and the others stay unmatched


def test_totals_f1_and_velocity_error():
    t = ev.Totals()
    truth = [note(1.0, 60, 40), note(2.0, 62, 80), note(3.0, 64, 100)]
    est = [note(1.01, 60, 50), note(2.02, 62, 70), note(5.0, 70, 90)]
    t.add(truth, est, 0.05)
    s = t.summary()
    assert s["recall"] == pytest.approx(2 / 3, abs=1e-3)
    assert s["precision"] == pytest.approx(2 / 3, abs=1e-3)
    assert s["f1"] == pytest.approx(2 / 3, abs=1e-3)
    assert s["velocity_mae"] == 10.0
    assert s["velocity_r"] == 1.0


def test_empty_inputs_do_not_divide_by_zero():
    s = ev.Totals().summary()
    assert s["f1"] == 0.0 and s["velocity_mae"] is None


def test_score_groups_by_mode_and_kind():
    entries = [
        {"id": "a", "vel_mode": "loud", "kind": "melody", "notes": [note(1.0, 60)]},
        {"id": "b", "vel_mode": "ramp", "kind": "melody", "notes": [note(1.0, 61)]},
    ]
    est = {"a": [note(1.0, 60)], "b": []}
    res = ev.score(entries, est, 0.05)
    assert res["all"]["recall"] == 0.5
    assert res["vel_mode=loud"]["f1"] == 1.0 and res["vel_mode=ramp"]["f1"] == 0.0
    assert res["kind=melody"]["notes"] == 2


def test_response_groups_split_scores_by_velocity_response(tmp_path):
    def entry(eid, program, vel):
        note = {"start": 1.0, "end": 2.0, "pitch": 60, "velocity": vel}
        return {
            "id": eid,
            "program": program,
            "vel_mode": "skewed",
            "kind": "melody",
            "notes": [note],
        }

    entries = [entry("a", "good", 80), entry("b", "dull", 80)]
    est = {
        "a": [{"start": 1.0, "end": 2.0, "pitch": 60, "velocity": 80}],
        "b": [{"start": 1.0, "end": 2.0, "pitch": 60, "velocity": 20}],
    }
    response = {"good": 0.9, "dull": 0.1}
    got = ev.score(entries, est, 0.05, response)
    assert got["velocity_response=responsive"]["velocity_mae"] == 0.0
    assert got["velocity_response=weak"]["velocity_mae"] == 60.0
    assert "velocity_response=weak" not in ev.score(entries, est, 0.05)
    assert ev.response_group("unknown", response) is None
    assert ev.load_response(tmp_path) is None
    (tmp_path / "velocity_response.json").write_text('{"good": 0.9}', encoding="utf-8")
    assert ev.load_response(tmp_path) == {"good": 0.9}
