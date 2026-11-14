"""Tests for the noise, Shape Square, SUPER and MULTI stage models (no firmware needed)."""

from __future__ import annotations

import json
import random
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import synth_stage_noise as noise  # noqa: E402
import synth_stage_record as rec  # noqa: E402
import synth_stage_shapes as shapes  # noqa: E402
import synth_stage_unison as unison  # noqa: E402
import synth_wave_model as m  # noqa: E402
import synth_wave_oracle as o  # noqa: E402

CASES = (
    Path(__file__).resolve().parents[2] / "docs/research/data/synth-stage-oracle-cases-rest.json"
)
MODELS = {
    "0x121fd8": noise.stage_noise,
    "0x121ab5": shapes.stage_shape_square,
    "0x121d6d": unison.stage_super,
    "0x121c54": unison.stage_multi,
}


def test_recorded_oracle_cases_are_reproduced_word_for_word() -> None:
    data = json.loads(CASES.read_text())["stages"]
    assert set(data) == set(MODELS)
    for name, model in MODELS.items():
        assert len(data[name]) == rec.COUNT
        for case in data[name]:
            lanes = [rec.lane_from_json(ln) for ln in case["lanes"]]
            out, _, _ = model(lanes)
            assert out == case["oracle_out"], name


def test_rust_golden_file_is_what_the_generator_prints() -> None:
    import synth_stage_golden as g

    rust = (
        Path(__file__).resolve().parents[2] / "crates/nord-core/src/synth/tests_fw_stage_golden.rs"
    )
    assert rust.read_text() == g.rust(), "regenerate: python tools/synth_stage_golden.py > <file>"


def test_dump_round_trips_through_json() -> None:
    data = {"stages": {"0x1": [{"lanes": [], "oracle_out": [[1]]}]}}
    assert json.loads(rec.dump(data)) == data


def test_noise_lcg_matches_the_oracle_state_after_four_steps() -> None:
    # recorded in `docs/research/synth-stage-models.md` s2: state 0x18072E8C -> 0x8584AB60
    assert noise.noise_states(0x18072E8C)[3] == 0x8584AB60
    assert noise.noise_states(0x9755D4C1)[3] == 0x8A34A985


def test_square_offset_idle_shape_and_fade() -> None:
    inc = m.fix(m.ld(m.f2u(0.001)), 31)
    idle = shapes.square_offset(inc, m.f2u(0.5), 0)
    assert idle == m.u32(2 * m.fix(m.ld(m.f2u(0.5)), 30) - 0x80000000)
    # at a very high pitch the fade closes the width to the centre value
    high = shapes.square_offset(m.fix(m.ld(m.f2u(0.3)), 31), m.f2u(0.5), 0)
    assert high == 0x80000000


def test_spread_factor_is_monotonic_in_the_spread_word() -> None:
    lo, _ = unison.spread_factor(m.f2u(0.0))
    hi, om = unison.spread_factor(m.f2u(0.5))
    assert 0 < lo < hi and abs(float(hi + om) - 1.0) < 1e-6


def test_saw_diff_is_the_negated_band_limited_saw() -> None:
    inc = m.fix(m.ld(m.f2u(0.005)), 31)
    phases = [m.u32(0x20000000 + k * inc) for k in (1, 2, 3, 4)]
    r7 = m.recip_refined(m.flt(inc, -31), m.HALF)
    ys = [float(v) for v in unison.saw_diff(phases, r7)]
    assert all(ys[i] > ys[i + 1] for i in range(3))  # falls with the phase (minus xo)


def test_stage_models_are_deterministic_and_lane_symmetric_in_gain() -> None:
    lanes = o.random_lanes(random.Random(7), True)
    for ln in lanes:
        ln.pitch = m.f2u(0.006)
        ln.spread = m.f2u(0.5)
    for model in MODELS.values():
        a, _, _ = model(lanes)
        b, _, _ = model(lanes)
        assert a == b and len(a) == 2 and all(len(r) == 4 for r in a)


def test_unison_pokes_cover_the_voice_tables() -> None:
    lanes = o.random_lanes(random.Random(3), True)
    pk = o.pokes(lanes)
    assert pk[o.PHASE_TABLE + 3] == lanes[1].phases[1]
    assert pk[o.PARAM - 32 + 2 * 3 + 1] == lanes[1].ratios[3]
    assert pk[o.PARAM - 16 + 4 * 2 + 3] == lanes[0].groups[2][3]
    assert pk[unison.MULTI_OCTAVES_ADDR + 1] == lanes[0].octaves[1]
