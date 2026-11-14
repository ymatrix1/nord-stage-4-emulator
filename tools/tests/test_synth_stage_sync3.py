"""Tests for the SYNC 3..5 stage model (`0x122C5C`) and the `s5` probe (no firmware needed)."""

from __future__ import annotations

import random
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import synth_stage_golden6 as golden6  # noqa: E402
import synth_stage_sync as sync  # noqa: E402
import synth_stage_sync3 as sync3  # noqa: E402
import synth_wave_model as m  # noqa: E402
import synth_wave_oracle as o  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]


def lane(spread: float = 0.5, pulse: int = 0x40000000) -> m.Lane:
    one, zero = m.f2u(1.0), m.f2u(0.0)
    return m.Lane(
        pitch=m.f2u(0.006), phase1=0x12345678, phase2=0x9ABCDEF0, shape_a=zero, shape_b=zero,
        ratio1=one, ratio2=one, weights=(zero, one, zero, zero), out_gain=one,
        spread=m.f2u(spread), pulse=pulse,
    )  # fmt: skip


def test_wave_is_plus_minus_one_away_from_the_width_and_zero_at_it() -> None:
    q = m.X(4)
    assert sync3.wave(m.X(9, 10), m.HALF, q) == 1
    assert sync3.wave(m.X(1, 10), m.HALF, q) == -1
    assert sync3.wave(m.HALF, m.HALF, q) == 0  # d = 0: sign +, u = 1


def test_setup_at_the_idle_pulse_word_has_no_width_offset() -> None:
    s = sync3.setup(lane(), sync.table())
    assert m.st(s["w1"]) == 0 or s["w1"] == 0  # p = 0.5 -> r = 0
    assert s["c1"] == m.HALF and s["ratio"] == 1
    assert s["q"] >= 2 and s["q2"] >= 2
    assert 0 < float(s["g1"]) <= 1.5 and 0 < float(s["g2"]) <= 1.5


def test_stage_without_high_pass_is_the_mix_and_the_high_pass_is_one_pole() -> None:
    lanes = [lane(), lane()]
    raw, p1, p2 = sync3.stage_sync3(lanes, high_pass=False)
    hp, q1, q2 = sync3.stage_sync3(lanes)
    assert (p1, p2) == (q1, q2)
    assert hp[0][0] == raw[0][0]  # first sample: x - 0 + c * 0
    assert hp[0][1] != raw[0][1]


def test_stage_depends_on_the_spread_word() -> None:
    a = sync3.stage_sync3([lane(0.25), lane(0.25)])[0]
    b = sync3.stage_sync3([lane(0.75), lane(0.75)])[0]
    assert a != b


def test_s5_probe_draws_are_independent_of_the_trial_generator() -> None:
    lanes = o.random_lanes(random.Random(5))
    before = [ln.phase1 for ln in lanes]
    o.random_s5(5, lanes, o.STAGE_SYNC3)
    assert [ln.phase1 for ln in lanes] == before
    assert all(ln.s5_src is not None for ln in lanes)
    other = o.random_lanes(random.Random(5))
    o.random_s5(5, other, o.STAGE_SHAPE_SAW)  # not a probed stage: untouched
    assert all(ln.s5_src is None for ln in other)
    assert o.pokes(lanes)[o.S5_SRC] == lanes[0].s5_src


def test_golden6_file_is_what_the_generator_prints() -> None:
    path = ROOT / "crates/nord-core/src/synth/tests_fw_stage_golden6.rs"
    assert path.read_text().replace("\r\n", "\n") == golden6.rust()
