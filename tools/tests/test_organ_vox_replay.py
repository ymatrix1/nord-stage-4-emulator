"""Unit tests for tools/organ_vox_replay.py (fake oracle executable; no firmware needed)."""

from __future__ import annotations

import numpy as np
import organ_oracle_audio as oa
import organ_post_oracle as po
import organ_post_ref as pr
import organ_vox_replay as vr


def test_record_addresses_follow_the_image_strides() -> None:
    assert vr.record_addresses(0) == (0xE2D42, 0x95E46, 0x95FC6)
    assert vr.record_addresses(1) == (0xE2D49, 0x95EA6, 0x95FD2)
    assert vr.record_addresses(2) == (0xE2D50, 0x95F06, 0x95FDE)
    assert vr.record_addresses(3) == (0xE2D57, 0x95F66, 0x95FEA)


def test_the_arm_burst_is_the_gain_targets_and_the_vibrato_depth() -> None:
    p = vr.vox_pokes([60], 0xE46400, 0, weights=False)
    assert p == {0x95FC6: oa.ONE, 0x95FC7: 0, pr.DEPTH: 0xE46400}


def test_chord_weights_go_on_note_minus_24_of_the_record_weights() -> None:
    p = vr.vox_pokes([60, 64, 67], 0, 2)
    ones = sorted(a - 0x95F06 for a, v in p.items() if v == oa.f2u(1.0) and 0x95F06 <= a < 0x95F66)
    assert ones == [36, 40, 43]
    assert p[0x95FDE] == oa.ONE and p[0x95FDF] == 0


def test_replay_steps_run_the_post_stage_then_the_output_stage_on_the_record() -> None:
    steps = vr.replay_steps(2)
    assert steps[0] == po.POST_STEP
    assert steps[1] == f"{po.OUT_STEP}:I0={po.FINAL_OUT:#x},I1=0xe2d50"


def test_lane_stats_ignore_the_start_up_frames() -> None:
    x = np.zeros((10, 2))
    x[0] = [5.0, -5.0]
    x[5] = [0.5, 0.25]
    s = vr.lane_stats(x, skip=2)
    assert s["peak"] == 0.5 and s["lane_diff"] == 0.25
    assert vr.lane_stats(x, skip=20) == {"peak": 0.0, "rms": 0.0, "lane_diff": 0.0}


def test_render_returns_frames_by_two_lanes(monkeypatch) -> None:
    seen = {}

    def fake_run_chain(exe, image, pokes, steps, ranges, calls):
        seen.update(exe=exe, steps=steps, ranges=ranges, calls=calls)
        words = np.array([0x3F800000, 0x40000000, 0, 0], dtype="<u4")
        return np.tile(words, (calls, 1))

    monkeypatch.setattr(po, "run_chain", fake_run_chain)
    out = vr.render("exe", "img", {}, 0, 8)
    assert seen["calls"] == 4 and seen["ranges"] == ((po.FINAL_OUT, 4),)
    assert out.shape == (8, 2) and out[0].tolist() == [1.0, 2.0]
