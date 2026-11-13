import os
import sys

import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import synth_ladder_poles as lp  # noqa: E402


def test_prony_recovers_the_poles_of_a_cascade():
    poles = [0.2873, 0.6588, 0.93424, 0.9525]
    h = lp.cascade_impulse(poles, 384)
    got, err = lp.prony_poles(h)
    assert err < 1e-9
    assert np.allclose(np.sort(got.real), poles, atol=2e-4)
    assert np.max(np.abs(got.imag)) < 1e-6


def test_cascade_dc_gain_is_the_product_of_one_pole_gains():
    h = lp.cascade_impulse([0.5, 0.5], 400)
    assert abs(h.sum() - 4.0) < 1e-9  # 1 / (1 - 0.5)^2


def test_recorded_poles_are_stable_and_sorted():
    for (_r, _idx), (poles, dc) in lp.MEASURED.items():
        assert len(poles) == 4
        assert list(poles) == sorted(poles)
        assert all(abs(p) < 1.0 for p in poles)
        assert 1.0 <= dc <= 1.3


def test_two_poles_depend_only_on_r_at_low_resonance():
    a = lp.MEASURED[(0.1, 64)][0]
    b = lp.MEASURED[(0.1, 100)][0]
    assert a[0] == 0.75019 and abs(b[2] - 0.75253) < 1e-3  # the R pole survives index changes
