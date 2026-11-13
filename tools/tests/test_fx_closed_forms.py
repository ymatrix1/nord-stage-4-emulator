"""Tests for tools/fx_closed_forms.py: the documented equations against the bit-exact models."""

import random

import fx_closed_forms as cf
import fx_models
import fx_oracle as fo
import pytest

#: relative error ceiling per routine over the seeds below (float64 equations against 40-bit
#: registers and truncating stores; measured worst cases 9.8e-5, 2.8e-5, 6.2e-5 and 1.9e-3 where a
#: table index sits next to a rounding boundary)
TOLERANCE = {"resonant": 3e-4, "resonant4": 1e-4, "allpass": 2e-4, "lfo": 5e-3}
CLOSED = {
    "resonant": cf.resonant2,
    "resonant4": cf.resonant4,
    "allpass": cf.allpass,
    "lfo": cf.lfo,
}


@pytest.mark.parametrize("name", sorted(CLOSED))
def test_closed_form_matches_the_model(name):
    rt = fx_models.routines()[name]
    rng = random.Random(3)
    checked = 0
    for _ in range(120):
        case = rt.scenario(rng)
        mem, written = fo.model_run(rt, case)
        want = CLOSED[name]({**rt.static, **case})
        assert set(want) >= written, f"{name}: unmodelled writes {sorted(written - set(want))}"
        for addr, value in want.items():
            if isinstance(value, int):  # integer words (the phase sum) must be exact
                assert mem[addr] == value
                continue
            got = cf.f32(mem[addr])
            scale = max(abs(got), abs(value), 1e-3)
            assert abs(got - value) / scale < TOLERANCE[name], (name, hex(addr), got, value)
            checked += 1
    assert checked > 1000


def test_helpers_clip_smooth_lerp_and_f32():
    assert cf.clip(3.0, -2.0) == 2.0 and cf.clip(-3.0, 2.0) == -2.0 and cf.clip(0.5, 2.0) == 0.5
    assert cf.smooth(1.0, 0.0, 0.25) == 0.75 and cf.smooth(2.0, 5.0, 0.0) == 2.0
    assert cf.f32(0x3F800000) == 1.0 and cf.f32(0x00400000) == 0.0
    assert cf.K == cf.f32(0x3F7DA3C2)
    table = {10: 0x3F800000, 11: 0x40000000}  # 1.0, 2.0
    assert cf.lerp(table, 10, 0, 0.25) == 1.25


def test_tdf2_step_is_a_transposed_direct_form_two_biquad():
    st = [0.0, 0.0]
    b, a = (1.0, 0.5, 0.25), (0.1, 0.05)
    ys = [cf.lfo_tdf2(x, st, b, a) for x in (1.0, 0.0, 0.0, 0.0)]
    assert ys[0] == 1.0
    # impulse response of y = x + z1: second sample is b1 - a1 b0
    assert abs(ys[1] - (0.5 - 0.1)) < 1e-12
    assert abs(ys[2] - (0.25 - 0.05 - 0.1 * ys[1])) < 1e-12
