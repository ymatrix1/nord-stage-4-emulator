"""Tests for tools/synth_phrases.py."""

from __future__ import annotations

import random
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import synth_phrases as sp  # noqa: E402


@pytest.mark.parametrize("kind", sp.KINDS)
def test_phrase_obeys_transkun_note_rules(kind):
    _, notes = sp.make_phrase(7, kind=kind, duration_s=30.0)
    assert len(notes) > 10
    last_end: dict[int, float] = {}
    for n in sorted(notes, key=lambda n: n["onset_s"]):
        assert 0.0 <= n["onset_s"] < n["offset_s"] <= 30.0
        assert 1 <= n["velocity"] <= 127
        assert 21 <= n["midi"] <= 108
        # same-pitch notes never overlap and keep the minimum gap
        assert n["onset_s"] >= last_end.get(n["midi"], 0.0) + sp.MIN_GAP_S - 1e-3
        last_end[n["midi"]] = n["offset_s"]


def test_phrase_is_deterministic_and_seed_dependent():
    assert sp.make_phrase(3, duration_s=20.0) == sp.make_phrase(3, duration_s=20.0)
    assert sp.make_phrase(3, duration_s=20.0) != sp.make_phrase(4, duration_s=20.0)


def test_phrase_sorted_and_wide_range():
    _, notes = sp.make_phrase(5, kind="melody", duration_s=40.0, wide=True)
    keys = [(n["onset_s"], n["midi"]) for n in notes]
    assert keys == sorted(keys)
    assert all(sp.WIDE_LOW <= n["midi"] <= sp.WIDE_HIGH for n in notes)


def test_unknown_kind_rejected():
    with pytest.raises(ValueError):
        sp.make_phrase(1, kind="nope")


def test_velocity_distribution_is_soft_skewed():
    rng = random.Random(0)
    vals = sorted(sp.draw_velocity(rng) for _ in range(4000))
    assert vals[0] >= 1 and vals[-1] <= 127
    assert 35 <= vals[len(vals) // 2] <= 55  # median about 45


def test_track_rejects_overlap_and_tiny_notes():
    t = sp._Track(10.0)
    assert t.add(60, 1.0, 1.0, 50)
    assert not t.add(60, 1.5, 1.0, 50)  # overlaps the first note
    assert not t.add(61, 9.99, 1.0, 50)  # clipped to under MIN_NOTE_S
    assert t.add(60, 2.1, 0.5, 50)


@pytest.mark.parametrize("mode", sp.VEL_MODES)
def test_velocity_modes_stay_in_range_and_differ(mode):
    _, notes = sp.make_phrase(11, kind="melody", duration_s=40.0, vel_mode=mode)
    vels = [n["velocity"] for n in notes]
    assert 1 <= min(vels) and max(vels) <= 127
    if mode == "loud":
        assert min(vels) >= 40
    if mode == "uniform":
        assert max(vels) - min(vels) > 80
    with pytest.raises(ValueError):
        sp.make_phrase(1, vel_mode="nope")


def test_pedal_presses_alternate_and_stay_inside_phrase():
    ped = sp.make_pedal(4, 60.0)
    assert [p["value"] for p in ped] == [127, 0] * (len(ped) // 2) and len(ped) >= 4
    ts = [p["t"] for p in ped]
    assert ts == sorted(ts) and ts[-1] < 60.0
    assert sp.make_pedal(4, 60.0) == ped
