"""Tests for tools/synth_vel_check.py."""

from __future__ import annotations

import json
import sys
from pathlib import Path

import numpy as np
import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import synth_vel_check as svc  # noqa: E402
import synth_wav  # noqa: E402

RATE = 8000


def phrase(amp_of_vel, n=20):
    """Bursts one second apart; ``amp_of_vel`` maps velocity to burst amplitude."""
    rng = np.random.default_rng(3)
    vels = rng.integers(20, 120, size=n)
    x = np.zeros(RATE * (n + 1), dtype=np.float32)
    t = np.arange(int(0.4 * RATE)) / RATE
    notes = []
    for i, v in enumerate(vels):
        a = int((i + 0.5) * RATE)
        x[a : a + len(t)] = amp_of_vel(v) * np.sin(2 * np.pi * 440 * t)
        notes.append({"start": i + 0.5, "end": i + 0.9, "pitch": 60, "velocity": int(v)})
    return x, notes


def test_spearman_monotonic_and_constant():
    assert svc.spearman([1, 2, 3, 4], [10, 20, 40, 90]) == pytest.approx(1.0)
    assert svc.spearman([1, 2, 3, 4], [4, 3, 2, 1]) == pytest.approx(-1.0)
    assert np.isnan(svc.spearman([1, 2, 3], [5, 5, 5]))


def test_isolated_drops_close_onsets():
    assert svc.isolated(np.array([0.0, 0.1, 2.0, 4.0])) == [2, 3]


def test_item_rho_high_when_loudness_follows_velocity_and_low_when_not():
    x, notes = phrase(lambda v: v / 127 * 0.5)
    assert svc.item_rho(x, RATE, notes) > 0.95
    x, notes = phrase(lambda v: 0.3)
    assert svc.item_rho(x, RATE, notes) is None or abs(svc.item_rho(x, RATE, notes)) < 0.5


def test_item_rho_none_for_few_notes_or_single_velocity():
    x, notes = phrase(lambda v: v / 127 * 0.5)
    assert svc.item_rho(x, RATE, notes[:3]) is None
    flat = [{**n, "velocity": 64} for n in notes]
    assert svc.item_rho(x, RATE, flat) is None


def test_program_rho_averages_items_and_summarize_buckets(tmp_path):
    entries = []
    for name, amp in (("loud", lambda v: v / 127 * 0.5), ("flat", lambda v: 0.3)):
        x, notes = phrase(amp)
        path = tmp_path / "audio" / f"{name}.wav"
        path.parent.mkdir(exist_ok=True)
        synth_wav.write_wav_int16(path, synth_wav.to_mono_int16(x), RATE)
        entries.append({"program": name, "audio": f"audio/{name}.wav", "notes": notes})
    rho = svc.program_rho(tmp_path, entries, workers=1)
    assert rho["loud"] > 0.95 and ("flat" not in rho or rho["flat"] < 0.5)
    assert svc.summarize({"a": 0.9, "b": 0.4, "c": 0.0}) == {
        "programs": 3,
        "weak": 1,
        "middle": 1,
        "strong": 1,
    }


def test_main_writes_the_response_file(tmp_path):
    x, notes = phrase(lambda v: v / 127 * 0.5)
    path = tmp_path / "audio" / "a.wav"
    path.parent.mkdir()
    synth_wav.write_wav_int16(path, synth_wav.to_mono_int16(x), RATE)
    entry = {"program": "p", "audio": "audio/a.wav", "notes": notes, "silent": False}
    (tmp_path / "dataset.json").write_text(json.dumps({"entries": [entry]}), encoding="utf-8")
    assert svc.main([str(tmp_path), "--workers", "1"]) == 0
    assert json.loads((tmp_path / svc.OUT_NAME).read_text(encoding="utf-8"))["p"] > 0.95


def test_ranks_average_ties():
    assert list(svc.ranks([10, 20, 20, 30])) == [0.0, 1.5, 1.5, 3.0]
