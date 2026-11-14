"""Tests for the SYNC 1 and SYNC 7 stage models (`0x1226DD`, `0x123C7C`; no firmware needed)."""

from __future__ import annotations

import random
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import synth_stage_sync as sync  # noqa: E402
import synth_stage_sync1 as sync1  # noqa: E402
import synth_stage_sync7 as sync7  # noqa: E402
import synth_wave_model as m  # noqa: E402
import synth_wave_oracle as o  # noqa: E402


def lane(spread: float = 0.5, pitch: float = 0.006) -> m.Lane:
    one, zero = m.f2u(1.0), m.f2u(0.0)
    return m.Lane(
        pitch=m.f2u(pitch), phase1=0x12345678, phase2=0x9ABCDEF0, shape_a=zero, shape_b=zero,
        ratio1=one, ratio2=one, weights=(zero, one, zero, zero), out_gain=one,
        spread=m.f2u(spread),
    )  # fmt: skip


def test_ssfr_rounds_ties_to_even_and_matches_half_up_elsewhere() -> None:
    # 2 * 2^29 * 2 / 2^32 = 0.25 x: x = 2 is a tie (0.5 -> 0), x = 6 is a tie (1.5 -> 2)
    assert sync1.ssfr(2, 1 << 29) == 0
    assert sync1.ssfr(6, 1 << 29) == 2
    assert sync1.ssfr(7, 1 << 29) == 2  # 1.75 -> 2
    assert sync1.ssfr(m.u32(-6), 1 << 29) == m.u32(-2)  # -1.5 -> -2
    assert sync1.ssfr(0x40000000, 0x40000000) == 0x20000000


def test_sync1_setup_has_the_unfloored_reciprocal_and_the_fade() -> None:
    s = sync1.setup(lane(), sync.table())
    assert s["fh"] == m.ld(m.st(m.mul(s["h"], m.ld(m.f2u(0.006)))))
    assert 0 < s["g"] <= 1.5
    assert s["r"] > 0


def test_sync1_has_a_high_pass_and_depends_on_the_spread() -> None:
    lanes = [lane(), lane()]
    raw, p1, p2 = sync1.stage_sync1(lanes, high_pass=False)
    hp, q1, q2 = sync1.stage_sync1(lanes)
    assert (p1, p2) == (q1, q2) and hp[0][1] != raw[0][1]
    assert sync1.stage_sync1([lane(0.2)] * 2)[0] != sync1.stage_sync1([lane(0.8)] * 2)[0]
    assert o.STAGE_SYNC1 in o.HP_STAGES and o.STAGE_SYNC1 in o.S5_STAGES


def test_sync7_setup_doubles_the_ratio_and_adds_a_quarter_ratio_edge() -> None:
    s = sync7.setup(lane(), sync.table())
    assert s["h"] == m.ld(m.st(m.add(*[sync.lookup(lane().spread, sync.table())] * 2)))
    assert s["h4"] == m.ld(m.st(m.mul(sync7.QUARTER, s["h"])))
    assert s["q3"] >= 2 and s["q"] >= 2 and s["q2"] >= 2
    assert s["cc"] < 1  # 1 - c2 / (2 (1 - c2)) with c2 > 0


def test_sync7_depends_on_the_spread_and_returns_both_phases() -> None:
    a, p1, p2 = sync7.stage_sync7([lane(0.25), lane(0.25)])
    b, q1, q2 = sync7.stage_sync7([lane(0.75), lane(0.75)])
    assert a != b and (p1, p2) == (q1, q2)


def test_pitch_probe_draws_stay_below_nyquist_and_are_independent() -> None:
    for seed in range(1, 30):
        lanes = o.random_lanes(random.Random(seed))
        before = [ln.phase1 for ln in lanes]
        o.random_pitch(seed, lanes, o.STAGE_SYNC2)
        assert [ln.phase1 for ln in lanes] == before
        semis = m.s32(lanes[0].pitch_src) / 0x01000000
        rho = max(abs(float(m.ld(w))) for ln in lanes for w in (ln.ratio1, ln.ratio2))
        assert o.IDLE_PITCH * 2 ** (semis / 12) * rho <= o.MAX_INC * 1.001 or semis <= -23.9
        assert lanes[0].pitch_src == lanes[1].pitch_src
    other = o.random_lanes(random.Random(1))
    o.random_pitch(1, other, o.STAGE_SHAPE_SAW)  # not a probed stage: untouched
    assert all(ln.pitch_src is None for ln in other)
    probed = o.random_lanes(random.Random(1))
    o.random_pitch(1, probed, o.STAGE_SYNC7)
    assert o.pokes(probed)[o.PITCH_SRC] == probed[0].pitch_src


def test_sine_stages_get_a_random_spread() -> None:
    lanes = o.random_lanes(random.Random(3))
    o.random_s5(3, lanes, o.STAGE_SHAPE_SINE3)
    assert all(ln.s5_src is not None for ln in lanes)
