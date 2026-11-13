import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import synth_saw_model as m  # noqa: E402


def test_constants_are_the_image_values():
    assert abs(m.A - 0.34560624) < 1e-8
    assert abs(m.B - 0.11520208) < 1e-8
    assert abs(m.C - 4.3401995) < 1e-6
    assert abs(m.B * m.C * 2.0 - 1.0) < 1e-6  # the fade is unity below its knee


def test_fade_knee_and_zero():
    assert abs(m.fade(0.05) - 1.0) < 1e-6 and m.fade(0.05) == m.fade(0.1)
    assert abs(m.fade((m.A - m.B) / 2.0) - 1.0) < 1e-6  # knee at 0.1152 cycles per sample
    assert abs(m.fade(m.A / 2.0) - 0.5) < 1e-6  # half way down the linear fade
    assert m.fade((m.A + m.B) / 2.0) < 1e-6
    assert m.fade(0.4) == 0.0


def test_blep_step_values_measured_in_the_oracle():
    assert m.blep_step(0.0) == -1.0
    assert m.blep_step(1.0) == -0.25
    assert m.blep_step(-1.0) == 0.25
    assert m.blep_step(2.0) == 0.0
    assert m.blep_step(-2.0) == 0.0
    assert abs(m.blep_step(0.5) + 0.5625) < 1e-12


def test_saw_is_midpoint_at_the_wrap_and_linear_elsewhere():
    inc = 1.0 / 256.0
    assert m.saw_sample(0.0, inc) == 0.0  # y = 1 - 0 - 1
    assert abs(m.saw_sample(0.25, inc) - 0.5) < 1e-6
    assert abs(m.saw_sample(0.75, inc) + 0.5) < 1e-6
    assert abs(m.saw_sample(inc, inc) - (1.0 - 2.0 * inc - 0.25)) < 1e-6


def test_block_matches_oracle_grid_rows():
    # oracle (sharc-run, inc = 2^24 = 1/256 cycle): phase 0 gives 0.74219 0.98437 0.97656 0.96875
    y = m.saw_block(0.0, 1.0 / 256.0)
    assert [round(v, 5) for v in y] == [0.74219, 0.98437, 0.97656, 0.96875]
    # inc = 1/128: the first sample is the -0.25 step of the wrap
    y = m.saw_block(0.0, 1.0 / 128.0)
    assert [round(v, 5) for v in y] == [0.73437, 0.96875, 0.95312, 0.9375]


def test_amplitude_scales_linearly():
    assert abs(m.saw_sample(0.3, 0.01, 0.5) - 0.5 * m.saw_sample(0.3, 0.01)) < 1e-9
