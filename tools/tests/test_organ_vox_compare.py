"""Unit tests for tools/organ_vox_compare.py (synthetic partials, no oracle needed)."""

from __future__ import annotations

import numpy as np
import organ_vox_compare as vc
import pytest


def synth(n: int, partials: dict[int, list[float]]) -> np.ndarray:
    """Sum of sinusoids at the oscillator frequencies: `{note: [amp of h = 1, 2, ...]}`."""
    t = np.arange(n)
    x = np.zeros(n)
    for note, amps in partials.items():
        f0 = vc.cycles_per_frame(note - vc.OSC_OFFSET)
        for h, a in enumerate(amps, start=1):
            x += a * np.sin(2 * np.pi * h * f0 * t + 0.3 * h)
    return x


def test_cycles_per_frame_is_the_twelve_tet_ladder() -> None:
    assert vc.cycles_per_frame(12) / vc.cycles_per_frame(0) == pytest.approx(2.0)
    assert vc.cycles_per_frame(0) == pytest.approx(7.5349e-4, rel=1e-4)


def test_partial_amplitude_recovers_known_levels() -> None:
    x = synth(20000, {60: [0.5, 0.05, 0.02]})
    table = vc.partial_table(x, notes=(60,), harmonics=3)
    assert table[0] == pytest.approx([0.5, 0.05, 0.02], rel=0.02)
    db = vc.relative_db(table)
    assert db[0, 0] == 0.0
    assert db[0, 1] == pytest.approx(-20.0, abs=0.3)


def test_compare_reports_zero_deviation_for_identical_signals() -> None:
    x = synth(40000, {60: [0.3, 0.01], 64: [0.3, 0.02], 67: [0.3, 0.01]})
    sig = np.stack([x, x], axis=1)
    res = vc.compare(sig, sig, skip=100, frames=30000)
    assert np.abs(res["dev_db_L"]).max() < 1e-9
    assert np.abs(res["level_db_L"]).max() < 1e-9
    other = np.stack([2 * x, 2 * x], axis=1)
    res = vc.compare(sig, other, skip=100, frames=30000)
    assert res["level_db_L"] == pytest.approx([6.02] * 3, abs=0.05)


def test_sidebands_of_a_phase_modulated_carrier_follow_the_modulation_index() -> None:
    n = 60000
    t = np.arange(n)
    f0 = vc.cycles_per_frame(36)
    beta = 0.4
    x = np.sin(2 * np.pi * f0 * t + beta * np.sin(2 * np.pi * vc.LFO_CYCLES * t))
    sb = vc.sideband_table(x, notes=(60,))
    # J1(beta) / J0(beta) for beta = 0.4 is 0.2
    assert sb[0, 0] == pytest.approx(0.2, rel=0.08)
    assert sb[0, 1] == pytest.approx(0.2, rel=0.08)


def test_load_csv_reads_the_two_lanes(tmp_path) -> None:
    f = tmp_path / "o.csv"
    f.write_text("# mode: vox\n# rate: 43156.4\nindex,L,R\n0,0.5,0.25\n1,-0.5,-0.25\n")
    out = vc.load_csv(str(f))
    assert out.shape == (2, 2) and out[1, 0] == -0.5
