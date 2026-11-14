"""Unit tests for tools/organ_weights_replay.py (script composition and amplitude measurement)."""

from __future__ import annotations

import numpy as np
import organ_oracle_audio as oa
import organ_weights_replay as rp


def test_parse_script_shifts_calls_and_skips_comments() -> None:
    text = "# header\n0 0x957de 0x3f800000  # wB[0]\n\n3 1000 7\nbad line\n"
    assert rp.parse_script(text, shift=10) == [(10, 0x957DE, 0x3F800000), (13, 1000, 7)]
    assert rp.parse_script("") == []


def test_gain_pokes_use_the_set_0_addresses_and_unit_scale() -> None:
    pokes = rp.gain_pokes((0x600000, 0x600000, 0x300000, 0x300000), call=5)
    assert pokes == [
        (5, oa.GAIN_X, oa.f2u(0.75)),
        (5, oa.GAIN_Y, oa.f2u(0.75)),
        (5, oa.CROSS_YX, oa.f2u(0.375)),
        (5, oa.CROSS_XY, oa.f2u(0.375)),
    ]


SCRIPTS = {
    "model3": "0 0x959af 0x3f7ffffe\n",
    "keys_a": "0 0x957de 0x3c7abe00\n",
    "keys_b": "0 0x957de 0x3b7abe00\n",
    "vibrato_on": "0 0x955b6 0x635000\n",
}


def test_compose_places_the_four_phases_and_the_level_poke() -> None:
    pokes = rp.compose(SCRIPTS)
    p = rp.PHASE_CALLS
    assert (0, *rp.LEVEL_POKE) in pokes
    assert (p, 0x957DE, 0x3B7ABE00) in pokes
    assert (2 * p, 0x955B6, 0x635000) in pokes
    assert (2 * p, oa.GAIN_X, oa.f2u(0.75)) in pokes
    assert (3 * p, 0x959AF, 0x3F7FFFFE) in pokes


def test_wheel_weights_follow_the_last_poke_before_the_phase() -> None:
    pokes = rp.compose(SCRIPTS)
    w0 = rp.wheel_weights(pokes, 0)
    w1 = rp.wheel_weights(pokes, 1)
    assert w0[0] == np.frombuffer(np.uint32(0x3C7ABE00).tobytes(), dtype="<f4")[0]
    assert w1[0] < w0[0]  # the phase 1 burst lowers it
    assert not w0[1:].any()


def test_measure_recovers_the_amplitude_of_a_synthetic_wheel() -> None:
    incs = oa.increments(detuned=False)
    cycles = oa.wheel_cycles(10, incs)
    n = 2 * rp.PHASE_CALLS
    t = np.arange(n)
    sig = 0.25 * np.sin(2 * np.pi * cycles * t)
    out = np.stack([sig, sig], axis=1)
    weights = np.zeros(oa.WHEELS)
    weights[10] = 0.5
    rows = rp.measure(out, weights, incs, phase=0)
    assert len(rows) == 1
    wheel, wt, ax, ay = rows[0]
    assert wheel == 10 and wt == 0.5
    assert abs(ax - 0.25) < 0.01 and abs(ay - 0.25) < 0.01


def test_phase_csv_has_the_ratio_column() -> None:
    text = rp.phase_csv({0: [(36, 0.5, 0.25, 0.0)]})
    assert text.splitlines() == [
        "phase,wheel,wB,amp_x,amp_y,amp_x_per_wB",
        "0,36,0.5,0.25,0,0.5",
    ]
