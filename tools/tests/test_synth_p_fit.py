"""Tests of `tools/synth_p_fit.py`: harmonic levels, residuals, scores and the stage model level."""

from __future__ import annotations

import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import synth_p_fit as p  # noqa: E402

SR = 48_000
F0 = 392.0


def tone(kind: str, n: int = 24_000) -> np.ndarray:
    t = np.arange(n) / SR
    k = np.arange(1, 25)
    if kind == "saw":
        return sum(np.sin(2 * np.pi * j * F0 * t) / j for j in k) * 0.3
    return sum(np.sin(2 * np.pi * j * F0 * t) / j for j in k if j % 2) * 0.3


def test_ideal_series() -> None:
    assert np.allclose(p.ideal_saw_db(3), [0.0, -6.0206, -9.5424], atol=1e-3)
    sq = p.ideal_square_db(4)
    assert sq[0] == 0.0 and np.isinf(sq[1]) and abs(sq[2] + 9.5424) < 1e-3


def test_harmonic_levels_recover_a_saw_and_a_square() -> None:
    saw = p.relative_db(p.harmonic_levels(tone("saw"), SR, F0, 12))
    assert p.residual_db(saw, p.ideal_saw_db(12)) < 0.3
    sq = p.relative_db(p.harmonic_levels(tone("square"), SR, F0, 12))
    assert sq[1] < -60.0 and abs(sq[2] + 9.54) < 0.3
    # absolute level: the sine of amplitude 0.3 reads -10.46 dBFS
    assert abs(p.harmonic_levels(tone("saw"), SR, F0, 2)[0] + 10.46) < 0.2


def test_residual_ignores_harmonics_below_the_floor() -> None:
    ideal = p.ideal_square_db(6)
    measured = ideal.copy()
    measured[1] = -80.0  # even harmonic: ideal is -inf, must not count
    assert p.residual_db(measured, ideal) == 0.0
    measured[2] += 3.0
    assert p.residual_db(measured, ideal) > 1.0


def test_score_and_gain() -> None:
    m = {"low": -1.0, "mid": 2.0, "pres": -9.0, "bril": 4.0, "rms": 3.6}
    assert p.spectral_score(m) == 4.0
    assert p.fit_gain_db(3.6) == -3.6


def test_stage_model_pulse_is_six_db_over_saw_under_one_block() -> None:
    saw = p.stage_h1_db("saw", F0, ncalls=60)
    pulse = p.stage_h1_db("pulse", F0, ncalls=60)
    assert abs(saw) < 0.5
    assert abs(pulse - saw - 6.0) < 0.5


def test_cli_stage_prints(capsys) -> None:  # type: ignore[no-untyped-def]
    assert p.main(["stage", "saw", "392"]) == 0
    assert "H1" in capsys.readouterr().out
