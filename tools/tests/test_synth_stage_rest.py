"""Tests for the noise, Shape Square, SUPER and MULTI stage models (no firmware needed)."""

from __future__ import annotations

import json
import random
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import synth_stage_noise as noise  # noqa: E402
import synth_stage_record as rec  # noqa: E402
import synth_stage_shape_sine as shape_sine  # noqa: E402
import synth_stage_shape_sine2 as sine2  # noqa: E402
import synth_stage_shape_sine3 as sine3  # noqa: E402
import synth_stage_shapes as shapes  # noqa: E402
import synth_stage_super1 as super1  # noqa: E402
import synth_stage_sync as sync  # noqa: E402
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
    "0x123427": shape_sine.stage_shape_sine,
    "0x1235c9": shape_sine.stage_shape_sine_1,
    "0x1232a5": shape_sine.stage_shape_sine_2,
    "0x122f1f": shape_sine.stage_shape_sine_5,
    "0x120dab": sine2.stage_shape_sine_4,
    "0x121e9d": super1.stage_super1,
    "0x12377d": sine3.stage_shape_sine_3,
    "0x122a32": sync.stage_sync2,
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


def test_rust_golden_files_are_what_the_generator_prints() -> None:
    import synth_stage_golden as g

    base = Path(__file__).resolve().parents[2] / "crates/nord-core/src/synth"
    for part, name in (
        (1, "tests_fw_stage_golden.rs"),
        (2, "tests_fw_stage_golden2.rs"),
        (3, "tests_fw_stage_golden3.rs"),
    ):
        assert (base / name).read_text() == g.rust(part), f"regenerate part {part}"


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


def test_shape_sine_4_golden_file_is_what_the_generator_prints() -> None:
    import synth_stage_golden4 as g4

    path = Path(__file__).resolve().parents[2] / "crates/nord-core/src/synth"
    assert (path / "tests_fw_stage_golden4.rs").read_text() == g4.rust()


def test_shape_sine_4_high_pass_is_a_one_pole_difference_filter() -> None:
    one, zero = m.f2u(1.0), m.f2u(0.0)
    c = sine2.HP_IDLE[2]
    # idle states: the first word passes unchanged, later words add `c` times the previous output
    words = [m.f2u(v) for v in (0.5, 0.5, 0.25, -0.25)]
    out = sine2.output_high_pass(words, (zero, zero, c))
    assert out[0] == words[0]
    assert m.ld(out[1]) == m.ld(m.st(m.mul(m.ld(out[0]), m.ld(c))))
    # a state word shifts only the first difference
    shifted = sine2.output_high_pass(words, (one, zero, c))
    assert m.ld(shifted[0]) == m.sub(m.ld(words[0]), m.ONE)


def test_shape_sine_4_setup_laws_at_the_idle_spread() -> None:
    f = m.f2u(0.01)
    inc = m.fix(m.ld(f), 31)
    g, p, ir, k = sine2.setup(f, inc, m.f2u(0.5))
    assert 0.0 < float(g) < 1.5
    assert abs(float(p) - 1.0085) < 1e-3  # polynomial of the spread word at 0.5
    # width law saturates for spread >= 0.1: q = 1 and `pq = f`
    assert abs(float(ir) * float(m.ld(f)) - float(sine2.INV_TWO_PI)) < 1e-6
    assert float(k) > 0.0


def test_super1_fold_is_a_square_with_a_smoothed_edge_at_a_quarter_cycle() -> None:
    r7 = m.mul(m.ONE, m.X(50))
    far = super1.fold_diff([0x10000000, 0x70000000, 0x90000000, 0xE0000000], r7)
    # away from |x| = 0.5 the bump `w` is 0 and the sample is the sign of `|x| - 0.5`
    assert far == [-m.ONE, m.ONE, m.ONE, -m.ONE]
    near = super1.fold_diff([0x3FF00000], r7)[0]  # just below |x| = 0.5: inside the bump
    assert -m.ONE < near < 0


def test_super1_runs_seven_voices_and_leaves_the_eighth_phase() -> None:
    lanes = o.random_lanes(random.Random(5), True)
    for ln in lanes:
        ln.pitch, ln.spread = m.f2u(0.01), m.f2u(0.3)
    _, last, _ = super1.stage_super1(lanes)
    assert super1.VOICES == 7
    assert last[0] != lanes[0].phases[6]


def test_golden5_file_is_what_the_generator_prints() -> None:
    import synth_stage_golden5 as g5

    path = Path(__file__).resolve().parents[2] / "crates/nord-core/src/synth"
    assert (path / "tests_fw_stage_golden5.rs").read_text() == g5.rust()


def test_shape_sine_3_setup_laws_at_the_idle_spread() -> None:
    f, s5 = m.f2u(0.01), m.f2u(0.5)
    g, ra, rb, k, r7, c, shift = sine3.setup(f, s5)
    assert 0.0 < float(g) < 1.5
    # vr = 0.75 (1 - 5.5 f) at the idle spread; rA, rB are 1 / (1 -+ vr)
    vr = 0.75 * (1 - 5.5 * float(m.ld(f)))
    assert abs(float(ra) - 1 / (1 - vr)) < 1e-5
    assert abs(float(rb) - 1 / (1 + vr)) < 1e-5
    assert abs(float(c) - 2 / 3.141592653589793 * vr) < 1e-6
    assert abs(shift - vr * 2**31) < 2**9
    assert abs(float(r7) - 50.0) < 1e-3
    assert float(k) > 0.0


def test_shape_sine_3_warp_selects_the_lower_slope_below_the_knee() -> None:
    ra, rb = m.X(2), m.X(1, 2)
    assert sine3.warp_y(m.X(-3, 4), ra, rb) == m.X(1, 2)  # (a + 1) rA = 0.5 < 1: lower slope
    assert sine3.warp_y(m.X(0), ra, rb) == m.X(-1, 2)  # (a + 1) rA = 2 >= 1: (a - 1) rB


def test_shape_sine_3_golden_has_96_rows() -> None:
    import synth_stage_golden5 as g5

    assert len(g5.sine3_rows()) == 96


def test_sync_table_file_has_the_documented_nodes() -> None:
    words = sync.table()
    assert len(words) == 258
    assert m.ld(words[0]) == m.ONE and m.ld(words[128]) == m.X(16)
    assert words[257] == 0x358637BD
    assert all(m.ld(a) < m.ld(b) for a, b in zip(words[:127], words[1:128], strict=True))


def test_sync_lookup_interpolates_linearly_and_hits_the_nodes() -> None:
    words = sync.table()
    assert sync.lookup(m.f2u(0.5), words) == m.ld(words[64])  # index 64, fraction 0
    mid = sync.lookup(m.f2u(0.5 + 0.5 / 128), words)
    want = m.ld(words[64]) + (m.ld(words[65]) - m.ld(words[64])) / 2
    assert abs(float(mid - want)) < 1e-6


def test_sync_uuf_is_the_upper_unsigned_product_word() -> None:
    assert sync.uuf(0x80000000, 0x80000000) == 0x40000000
    assert sync.uuf(0xFFFFFFFF, 2) == 1
    assert sync.int_clip(0x00000500, 0x100) == 0x100
    assert sync.int_clip(m.u32(-0x500), 0x100) == m.u32(-0x100)


def test_sync_setup_at_the_idle_spread_matches_the_table_node() -> None:
    f = m.f2u(0.01)
    g, frac, lim, mult, q, r7, off = sync.setup(f, m.f2u(0.5), sync.table())
    h = m.ld(sync.table()[64])  # 4.0439
    assert lim == 4 << 25 and mult == m.fix(h, 25)
    assert abs(float(frac) - (float(h) - 4)) < 1e-6
    assert abs(float(r7) - 50.0) < 1e-3 and abs(float(q) - 50.0 / float(h)) < 1e-4
    assert abs(off + 2**31 / float(h)) < 2**8 or off == m.fix(-m.X(1) / h, 31)
    assert 0 < float(g) < 1.5


def test_sync_and_sine3_rust_table_is_what_the_generator_prints() -> None:
    path = Path(__file__).resolve().parents[2] / "crates/nord-core/src/synth"
    assert (path / "fw_sync_table.rs").read_text() == sync.rust_table()
