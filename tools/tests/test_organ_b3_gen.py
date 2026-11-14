"""Unit tests for tools/organ_b3_gen.py (synthetic wheels; no firmware)."""

from __future__ import annotations

import numpy as np
import organ_b3_gen as g


def sine_table(points: int = 129) -> np.ndarray:
    return np.sin(2 * np.pi * np.arange(points) / (points - 1))


def test_phase_inc_is_a_gear_ladder_within_a_cent_of_12tet() -> None:
    cents = g.phase_inc_cents()
    assert cents[0] == 0.0
    assert max(abs(c) for c in cents) < 1.0
    assert all(b > a for a, b in zip(g.PHASE_INC, g.PHASE_INC[1:]))


def test_base_step_is_the_ssfr_rounding_of_inc_times_the_fine_word() -> None:
    # 0x3F000000 / 2**31 = 63 / 128; `floor` differs from `round` on some pitch classes
    assert g.base_step(0) == (0x31AD21 * 63 + 64) >> 7
    assert g.base_step(0, "floor") == (0x31AD21 * 63) >> 7
    assert any(g.base_step(k) != g.base_step(k, "floor") for k in range(12))


def test_note_step_doubles_per_octave_and_matches_oracle_frequency() -> None:
    assert g.note_step(60) == g.base_step(0) << 5
    assert g.note_step(72) == 2 * g.note_step(60)
    # VERIFIED against the oracle fit of ring word DM 0xB3318 (note 60): 0.011938497 cycles/call
    assert abs(g.note_freq(60) - 0.011938497) < 5e-10
    # note 67 (pitch class 7), oracle fit 0.017884120
    assert abs(g.note_freq(67) - 0.017884120) < 5e-9


def test_drawbar_partials_and_ring_index() -> None:
    assert g.partial_notes(60) == [48, 67, 60, 72, 79, 84, 88, 91, 96]
    assert g.ring_index(60) == 48
    assert g.RING_BASE + g.ring_index(60) == 0xB3318


def test_wheel_interpolates_and_wraps() -> None:
    t = sine_table()
    p = np.array([0.0, 0.25, 0.5, 1.0, 1.25, -0.75])
    assert np.allclose(g.wheel(t, p), np.sin(2 * np.pi * p), atol=2e-4)


def test_fit_gain_phase_recovers_gain_and_phase() -> None:
    t = sine_table()
    f = 0.0119385
    series = 0.4 * g.model_series(t, f, 0.3, 1500)
    fit = g.fit_gain_phase(series, t, f)
    assert abs(fit.gain - 0.4) < 1e-3
    assert abs(fit.phase0 - 0.3) < 1e-3
    assert fit.rel_residual < 1e-4


def test_fft_and_refine_freq_find_the_frequency() -> None:
    t = sine_table()
    f = 0.0178841
    series = g.model_series(t, f, 0.7, 2000)
    assert abs(g.fft_freq(series) - f) < 2e-4
    assert abs(g.refine_freq(series, t, g.fft_freq(series)) - f) < 1e-8


def test_scan_columns_skips_silent_words_and_reports_notes() -> None:
    t = sine_table()
    ring = np.zeros((1200, 0x70), dtype=np.float32)
    ring[:, 48] = 0.3 * g.model_series(t, g.note_freq(60), 0.1, 1200)
    rows = g.scan_columns(ring, t)
    assert [r["note"] for r in rows] == [60]
    assert rows[0]["address"] == 0xB3318
    assert abs(rows[0]["gain"] - 0.3) < 1e-3
    assert abs(rows[0]["freq"] / g.note_freq(60) - 1) < 1e-6


def test_harmonics_of_a_sine_plus_second_harmonic() -> None:
    p = np.arange(129) / 128
    t = 0.98 * np.sin(2 * np.pi * p) - 0.02 * np.cos(4 * np.pi * p)
    amp, ph = g.harmonics(t, 3)
    assert np.allclose(amp, [0.98, 0.02, 0.0], atol=1e-9)
    assert abs(ph[0] + np.pi / 2) < 1e-9 and abs(abs(ph[1]) - np.pi) < 1e-9
