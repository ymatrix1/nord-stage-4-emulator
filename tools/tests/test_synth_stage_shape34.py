"""Tests for the SHAPE 3 and SHAPE 4 stage models (`0x120F55`, `0x12145E`) and their goldens."""

from __future__ import annotations

import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import synth_stage_golden9 as golden9  # noqa: E402
import synth_stage_shape3 as shape3  # noqa: E402
import synth_stage_shape4 as shape4  # noqa: E402
import synth_wave_model as m  # noqa: E402
import synth_wave_oracle as o  # noqa: E402


def lane(spread: float = 0.5, pitch: float = 0.01, **kw) -> m.Lane:
    one, zero = m.f2u(1.0), m.f2u(0.0)
    base = {
        "pitch": m.f2u(pitch), "phase1": 0x12345678, "phase2": 0x9ABCDEF0, "shape_a": zero,
        "shape_b": zero, "ratio1": one, "ratio2": one, "weights": (zero, one, zero, one),
        "out_gain": one, "spread": m.f2u(spread),
    }  # fmt: skip
    base.update(kw)
    return m.Lane(**base)


def test_cos_helper_is_minus_cosine_of_half_pi_x() -> None:
    assert abs(float(shape3.cos_helper(0)) + 1.0) < 1e-3
    assert abs(float(shape3.cos_helper(1 << 30))) < 1e-6
    q = int(0.4 * 2**30)
    assert abs(float(shape3.cos_helper(q)) + math.cos(math.pi / 2 * 0.4)) < 1e-3
    assert shape3.cos_helper(q) == shape3.cos_helper(-q)


def test_shape3_width_law_clamps_between_two_and_the_shape_limit() -> None:
    ln = lane(spread=0.5, pitch=0.01)
    inc = m.fix(m.mul(m.ld(ln.pitch), m.ld(ln.ratio1)), 31)
    _, _, _, w, c, iw_fix, _, _, _ = shape3.shape3_setup(ln)
    # K0 / a = 0.1815 / 0.01 = 18.15 is limited by 14 s^2 + 2 = 5.5
    assert abs(float(w) - 5.5) < 1e-6 and inc > 0
    assert abs(float(c) - 2 / (math.pi * 5.5)) < 1e-6
    assert iw_fix == m.fix(m.ld(m.st(m.recip_refined(w, m.ONE))), 31)
    high = shape3.shape3_setup(lane(spread=0.5, pitch=0.2))[3]
    assert float(high) == 2.0  # K0 / a = 0.9 is raised to 2


def test_shape4_setup_matches_the_documented_laws() -> None:
    inc1, _, a, g, inv_a, inv_d, offset, c0 = shape4.shape4_setup(lane(spread=0.5, pitch=0.02))
    assert inc1 == m.fix(m.f2u and m.mul(m.ld(m.f2u(0.02)), m.ONE), 31)
    assert abs(float(inv_a) - 0.5 / float(a)) / float(inv_a) < 1e-5
    assert abs(float(inv_d) - 2 / 1.125) < 1e-6  # D = 1 + 0.25 - 0.125
    assert offset == 2 * m.fix(m.ld(m.f2u(0.5)), 30)
    assert abs(float(c0) - (2 / 3) * (float(a) ** 2 - 1)) < 1e-6
    assert g > 0


def test_stages_return_two_lanes_of_four_words_and_are_deterministic() -> None:
    for stage in (shape3.stage_shape3, shape4.stage_shape4):
        lanes = [lane(), lane(pitch=0.013, phase1=7)]
        out, p1, p2 = stage(lanes)
        assert len(out) == 2 and all(len(r) == 4 for r in out)
        assert len(p1) == 2 and len(p2) == 2
        assert out == stage(lanes)[0]
        assert all(math.isfinite(float(m.ld(w))) for r in out for w in r)


def test_stages_are_registered_in_the_oracle() -> None:
    assert o.MODELS[o.STAGE_SHAPE3] is shape3.stage_shape3
    assert o.MODELS[o.STAGE_SHAPE4] is shape4.stage_shape4
    assert o.STAGE_SHAPE3 == 0x120F55 and o.STAGE_SHAPE4 == 0x12145E
    for stage in (o.STAGE_SHAPE3, o.STAGE_SHAPE4):
        assert stage in o.S5_STAGES and stage in o.PITCH_STAGES


def test_golden_rows_have_the_documented_shape() -> None:
    rows = golden9.shape_rows(shape4.stage_shape4)
    assert len(rows) == 160
    assert all(math.isfinite(float(m.ld(r[3]))) for r in rows)
    text = golden9.rust()
    for name in ("SH3_PHASE", "SH3_OUT", "SH4_DT", "SH4_SHAPE"):
        assert f"const {name}: [u32; 160]" in text
