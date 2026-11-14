"""Unit tests for tools/organ_shaper.py (synthetic tables; no firmware needed)."""

from __future__ import annotations

import numpy as np
import organ_shaper as sh
import pytest


def test_the_table_runs_from_minus_one_to_one_and_is_nearly_odd() -> None:
    t = sh.blep_table(*sh.CLEAN)
    assert t.shape == (sh.POINTS,)
    assert abs(t[0] + 1) < 1e-3 and abs(t[-1] - 1) < 1e-3
    odd_error = np.abs(t + t[::-1]).max()
    assert odd_error < 0.02  # the half-sample shift leaves a small even part
    assert abs(t[512]) < 0.01


def test_the_band_limited_step_overshoots_by_about_nine_percent_at_a_third_of_the_range() -> None:
    t = sh.blep_table(*sh.FITTED)
    peak = int(np.argmax(t))
    assert abs(peak - 512 - 171) <= 2  # first extremum at 512 / 3 steps
    assert 1.07 < t[peak] < 1.11
    assert 1.0 <= t[512 + 128] < 1.06


def test_error_stats_and_the_fit_recover_the_parameters() -> None:
    target = sh.blep_table(171.0, -0.44)
    assert sh.error_stats(target, target) == (0.0, 0.0)
    a, shift, rms, peak = sh.fit(target)
    assert abs(a - 171.0) <= 0.1 and abs(shift + 0.44) <= 0.02
    assert rms < 2e-4 and peak < 6e-4


def test_clean_and_fitted_forms_are_close_to_each_other() -> None:
    rms, peak = sh.error_stats(sh.blep_table(*sh.CLEAN), sh.blep_table(*sh.FITTED))
    assert rms < 5e-4 and peak < 1e-3


def test_blackman_window_is_zero_outside_and_one_at_the_centre() -> None:
    w = sh.blackman(np.array([-1.5, -1.0, 0.0, 1.0, 2.0]))
    assert w[0] == 0 and w[4] == 0
    assert w[2] == pytest.approx(1.0)
    assert w[1] == pytest.approx(0.0, abs=1e-12)


def test_parse_dump_reads_floats_and_checks_the_count() -> None:
    words = np.linspace(-1, 1, sh.POINTS).astype("<f4").view("<u4")
    text = "calls 1, steps per call 0\nDM(0xe2312..): " + " ".join(f"{w:x}" for w in words) + "\n"
    out = sh.parse_dump(text)
    assert out.shape == (sh.POINTS,) and out[0] == -1.0 and out[-1] == 1.0
    with pytest.raises(ValueError):
        sh.parse_dump(text, count=10)


def test_read_table_builds_the_dump_command(monkeypatch) -> None:
    words = np.zeros(sh.POINTS, dtype="<u4")
    seen: list[list[str]] = []

    class Done:
        stdout = "DM(0xe2312..): " + " ".join(f"{w:x}" for w in words)

    def fake_run(cmd, check, capture_output, text):
        seen.append(cmd)
        return Done()

    monkeypatch.setattr(sh.subprocess, "run", fake_run)
    table = sh.read_table("exe", "img")
    assert table.shape == (sh.POINTS,)
    assert seen[0][seen[0].index("--dump") + 1] == f"{sh.TABLE_BASE - 512:#x}:1025"
