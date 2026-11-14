"""Tests for the MISC and SUB OSC stage models (`0x122030`, `0x122151`, `0x122417`, `0x12179A`)."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import synth_stage_golden as g  # noqa: E402
import synth_stage_golden8 as golden8  # noqa: E402
import synth_stage_misc as misc  # noqa: E402
import synth_stage_noise as noise  # noqa: E402
import synth_stage_sub as sub  # noqa: E402
import synth_stage_sync as sync  # noqa: E402
import synth_wave_model as m  # noqa: E402
import synth_wave_oracle as o  # noqa: E402


def lane(**kw) -> m.Lane:
    return golden8.blank(**kw)


def test_lcg_matches_the_noise_stage_and_floats_are_exact() -> None:
    s, v = misc.lcg(3)
    assert s == noise.noise_states(3)[0]
    assert v == m.flt(s, -31)


def test_pink_rows_update_by_the_counter_mask_rule() -> None:
    # counter 1 -> 2: only the fastest row (mask 1, index 9) sees `c & mask == 0`
    ln = lane(state=3, rows=(0x3F800000,) * misc.ROWS, counter=1)
    _, _, counter, rows = misc.pink_lane(ln)
    assert counter == 5  # four steps
    assert rows[0] == 0x3F800000  # the slowest row (mask 0x3ff) is not refreshed
    assert rows[9] != 0x3F800000  # the fastest row is
    # a counter of 0x3ff + 1 = 0x400 refreshes row 0 (mask 0x3ff) in the first step
    _, _, _, rows2 = misc.pink_lane(lane(state=3, rows=(0x3F800000,) * misc.ROWS, counter=0x3FF))
    assert rows2[0] != 0x3F800000


def test_lowpass_state_is_one_pole_and_scales_by_64() -> None:
    ln = lane(state=7, lp=0)
    words, state, z = misc.lowpass_lane(ln)
    s, v = misc.lcg(7)
    z1 = m.add(m.mul(m.sub(v, m.X(0)), m.ld(misc.LP_COEF)), m.X(0))
    assert words[0] == m.st(m.mul(z1, m.ld(misc.LP_GAIN)))
    assert state != 7 and z != 0 and s != state


def test_stage_models_return_two_lanes_of_four_words() -> None:
    lanes = [lane(state=3, rows=(0,) * misc.ROWS), lane(state=5, rows=(0,) * misc.ROWS, lp=0)]
    for lanes_, stage in (
        (lanes, misc.stage_misc0),
        (lanes, misc.stage_misc1),
    ):
        out, states, _ = stage(lanes_)
        assert len(out) == 2 and all(len(r) == 4 for r in out) and len(states) == 2
    assert noise.MULT == 0x0BB38435


def test_fm_modulator_increment_is_clipped_at_half_a_cycle() -> None:
    words = sync.table()
    low = lane(pitch=m.f2u(0.006), spread=m.f2u(0.5))
    high = lane(pitch=m.f2u(0.9), spread=m.f2u(1.0))
    assert 0 < misc.mod_inc(low, words) < 1 << 30
    assert misc.mod_inc(high, words) == 1 << 30
    out, ph1, ph2 = misc.fm_lane(low, words)
    assert len(out) == 4 and ph1 != low.phase1 and ph2 != low.phase2


def test_sub_osc_adds_a_saw_and_a_half_rate_square() -> None:
    ln = lane(pitch=m.f2u(0.01), phase1=0x10000000, spread=m.f2u(0.0))
    out, p1, p2 = sub.stage_sub_osc([ln, ln])
    # s5 = 0: no pulse, the level 1 - 0.3 * 0 = 1: the saw helper alone at phase 2 u
    inc = m.fix(m.mul(m.ld(ln.pitch), m.ld(ln.ratio1)), 31)
    saw, _ = m.saw_helper(m.u32(2 * ln.phase1), inc, amp=m.ONE)
    assert out[0] == [m.st(w) for w in saw]
    half = sub.sync1.ssfr(inc, 0x40000000)
    assert p1[0] == m.u32(ln.phase1 + 4 * half) and p2[0] == ln.phase2 + 4 * half & 0xFFFFFFFF


def test_oracle_registers_the_new_stages() -> None:
    for stage, model in (
        (o.STAGE_MISC0, misc.stage_misc0),
        (o.STAGE_MISC1, misc.stage_misc1),
        (o.STAGE_MISC2, misc.stage_misc2),
        (o.STAGE_SUB_OSC, sub.stage_sub_osc),
    ):
        assert o.MODELS[stage] is model
    assert o.STAGE_MISC0 in o.HP_STAGES and o.STAGE_MISC2 in o.HP_STAGES
    assert o.STAGE_SUB_OSC in o.S5_STAGES and o.STAGE_SUB_OSC in o.PITCH_STAGES


def test_random_misc_draws_rows_counter_and_filter_state_only_for_misc() -> None:
    lanes = o.random_lanes(__import__("random").Random(1))
    o.random_misc(1, lanes, o.STAGE_MISC0)
    assert len(lanes[0].rows) == misc.ROWS and lanes[0].lp == 0
    lanes = o.random_lanes(__import__("random").Random(1))
    o.random_misc(1, lanes, o.STAGE_MISC1)
    assert lanes[0].rows == () and lanes[0].lp != 0
    lanes = o.random_lanes(__import__("random").Random(1))
    o.random_misc(1, lanes, o.STAGE_SUB_OSC)
    assert lanes[0].rows == () and lanes[0].lp == 0


def test_pokes_place_rows_counter_and_filter_words() -> None:
    lanes = o.random_lanes(__import__("random").Random(2))
    o.random_misc(2, lanes, o.STAGE_MISC0)
    pokes = o.pokes(lanes)
    assert pokes[o.PINK_PTR - 32 + 2 * 3] == lanes[0].rows[3]
    assert pokes[o.PINK_PTR - 32 + 2 * 3 + 1] == lanes[1].rows[3]
    assert pokes[o.RECORD + 13] == lanes[0].counter
    lanes = o.random_lanes(__import__("random").Random(2))
    o.random_misc(2, lanes, o.STAGE_MISC1)
    assert o.pokes(lanes)[o.RECORD - 3] == lanes[0].lp


def test_golden8_rust_file_is_what_the_generator_prints() -> None:
    path = (
        Path(__file__).resolve().parents[2] / "crates/nord-core/src/synth/tests_fw_stage_golden8.rs"
    )
    assert path.read_text() == golden8.rust(), "regenerate with synth_stage_golden8.py"
    assert len(golden8.pink_sequence()) == 4 * golden8.BLOCKS
    assert len(golden8.lowpass_sequence()) == 4 * golden8.BLOCKS
    assert g.INCREMENTS


def test_golden8_rows_are_finite_and_cover_the_grids() -> None:
    fm, sb = golden8.fm_rows(), golden8.sub_rows()
    assert len(fm) == len(sb) == len(g.INCREMENTS) * len(golden8.SHAPES) * (g.STARTS // 4) * 4
    assert all(m.ld(r[-1]) == m.ld(r[-1]) for r in fm + sb)
