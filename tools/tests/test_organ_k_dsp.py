"""Tests for tools/organ_k_dsp.py (pure measurement helpers of the organ K controls)."""

from __future__ import annotations

import math
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import organ_k_dsp as d  # noqa: E402

SR = 8000


def tone(freq: float, amp: float = 0.1, secs: float = 2.0) -> np.ndarray:
    t = np.arange(int(SR * secs)) / SR
    return amp * np.sin(2 * np.pi * freq * t)


def test_band_filter_keeps_inband_and_drops_outband() -> None:
    x = tone(500) + tone(50) + tone(3000)
    y = d.band_filter(x, SR, [(160.0, 1300.0)])
    assert abs(d.rms_db(y) - d.rms_db(tone(500))) < 0.1


def test_band_filter_cut_removes_hole() -> None:
    x = tone(500) + tone(2000)
    y = d.band_filter(x, SR, [(160.0, 3000.0)], [(1500.0, 3000.0)])
    assert abs(d.rms_db(y) - d.rms_db(tone(500))) < 0.1


def test_band_filter_stereo_axis() -> None:
    x = np.stack([tone(500), tone(50)])
    y = d.band_filter(x, SR, [(160.0, 1300.0)])
    assert d.rms_db(y[1]) < -100 and d.rms_db(y[0]) > -30


def test_rms_db_known_value_and_stereo_power() -> None:
    assert abs(d.rms_db(tone(500, 1.0)) - (-3.01)) < 0.01
    st = np.stack([tone(500, 1.0), np.zeros(SR * 2)])
    assert abs(d.rms_db(st) - (-6.02)) < 0.01  # mean power over both channels


def test_rms_and_crest_mask() -> None:
    x = np.concatenate([tone(500, 0.5, 1.0), np.zeros(SR)])
    m = np.arange(len(x)) < SR
    assert abs(d.rms_db(x, m) - d.rms_db(tone(500, 0.5, 1.0))) < 1e-6
    assert abs(d.crest_db(x, m) - 3.01) < 0.05
    assert math.isnan(d.rms_db(x, np.zeros(len(x), dtype=bool)))
    assert math.isnan(d.crest_db(x, np.zeros(len(x), dtype=bool)))


def test_k_needed_and_spread() -> None:
    assert abs(d.k_needed(2.8, -6.0206) - 5.6) < 0.01
    assert abs(d.k_needed(2.8, 0.0) - 2.8) < 1e-9
    assert abs(d.spread_db([1.0, 2.0, 4.0]) - 12.04) < 0.01
    assert d.spread_db([1.0]) == 0.0


def test_drive_slope_linear_and_compressed() -> None:
    assert abs(d.drive_slope(2.0, 6.0206, 1.0, 0.0) - 1.0) < 1e-3
    assert abs(d.drive_slope(2.0, 3.0103, 1.0, 0.0) - 0.5) < 1e-3


def test_active_frames_gate_silence() -> None:
    x = np.concatenate([tone(500, 0.1, 1.0), np.zeros(SR), tone(500, 0.1, 1.0)])
    fr = d.active_frames(x, SR)
    m = d.frames_to_samples(fr, len(x), SR)
    assert m[: SR // 2].all() and not m[SR + SR // 4 : SR + SR // 2].any()


def test_note_frames_pure_and_pad() -> None:
    n = SR * 4
    ev = [
        {"midi": 60, "onset_s": 0.0, "offset_s": 1.0},
        {"midi": 40, "onset_s": 0.5, "offset_s": 1.5},
        {"midi": 62, "onset_s": 3.0, "offset_s": 3.1},
    ]
    anyf = d.note_frames(ev, n, SR, lo=48)
    pure = d.note_frames(ev, n, SR, lo=48, pure=True)
    assert anyf[int(0.7 / d.HOP_S)] and not pure[int(0.7 / d.HOP_S)]
    assert pure[int(0.1 / d.HOP_S)]
    assert pure[int(3.2 / d.HOP_S)] and not pure[int(3.6 / d.HOP_S)]  # 0.25 s release pad


def test_note_stats_and_poly_matched() -> None:
    ev = [
        {"midi": 60, "onset_s": 0.0, "offset_s": 1.0, "velocity": 50},
        {"midi": 62, "onset_s": 0.0, "offset_s": 1.0, "velocity": 70},
    ]
    s = d.note_stats(ev, 2.0)
    assert s["mean_poly"] == 1.0 and s["density_per_s"] == 1.0 and s["mean_velocity"] == 60.0
    assert abs(d.poly_matched_k(8.0, 1.0, 4.0) - 4.0) < 1e-9
    assert d.note_stats([], 1.0)["mean_dur_s"] == 0.0
