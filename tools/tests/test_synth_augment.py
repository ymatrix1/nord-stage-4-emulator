"""Tests for tools/synth_augment.py (the codec test needs ffmpeg and is skipped without it)."""

from __future__ import annotations

import sys
from pathlib import Path

import numpy as np
import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import synth_augment as sa  # noqa: E402
import synth_wav  # noqa: E402

RATE = 44100


def tone(seconds=2.0, f=440.0):
    t = np.arange(int(RATE * seconds)) / RATE
    return (0.3 * np.sin(2 * np.pi * f * t)).astype(np.float32)


def test_plan_is_seeded_and_in_range():
    assert sa.plan_augment(5) == sa.plan_augment(5)
    plans = [sa.plan_augment(s) for s in range(300)]
    assert all(-2.0 <= p["tilt_db_oct"] <= 2.0 and -24.0 <= p["peak_dbfs"] <= -1.0 for p in plans)
    assert 0.35 < sum(p["codec"] is not None for p in plans) / 300 < 0.65
    assert all(30.0 <= p["noise_snr_db"] <= 60.0 for p in plans if p["noise_snr_db"] is not None)
    off = sa.plan_augment(5, enabled=False)
    assert off["codec"] is None and off["noise_snr_db"] is None and off["tilt_db_oct"] == 0.0


def test_tilt_changes_balance_and_keeps_rms():
    rng = np.random.default_rng(0)
    x = rng.standard_normal(RATE).astype(np.float32) * 0.1
    y = sa.apply_tilt(x, RATE, 2.0)
    assert synth_wav.rms(y) == pytest.approx(synth_wav.rms(x), rel=1e-3)
    sx, sy = np.abs(np.fft.rfft(x)), np.abs(np.fft.rfft(y))
    hi, lo = slice(8000, 10000), slice(200, 400)  # bins are 1 Hz wide
    assert sy[hi].sum() / sy[lo].sum() > 2.5 * sx[hi].sum() / sx[lo].sum()
    assert sa.apply_tilt(x, RATE, 0.0) is x


def test_noise_level_matches_snr():
    x = tone()
    y = sa.add_noise(x, 40.0, seed=1)
    noise_rms = synth_wav.rms(y - x)
    assert 20 * np.log10(synth_wav.rms(x) / noise_rms) == pytest.approx(40.0, abs=0.5)
    assert np.array_equal(y, sa.add_noise(x, 40.0, seed=1))


def test_apply_plan_sets_peak_without_codec():
    plan = {"tilt_db_oct": 1.0, "codec": None, "noise_snr_db": 50.0, "peak_dbfs": -12.0}
    y = sa.apply_plan(tone(), RATE, plan, seed=3)
    assert float(np.abs(y).max()) == pytest.approx(10 ** (-12 / 20), rel=1e-3)
    assert sa.apply_plan(np.zeros(100, np.float32), RATE, plan, seed=3).max() == 0.0


def test_codec_runner_is_called_with_encoder_settings(tmp_path):
    calls = []

    def fake(cmd):
        calls.append(cmd)
        if cmd[-1].endswith("out.wav"):
            synth_wav.write_wav_int16(Path(cmd[-1]), synth_wav.to_mono_int16(tone(1.9)), RATE)

    codec = {"name": "aac", "ext": "m4a", "kbps": 96}
    y = sa.codec_round_trip(tone(2.0), RATE, codec, fake)
    assert len(y) == 2 * RATE and len(calls) == 2  # short decode padded back to the input length
    assert "aac" in calls[0] and "96k" in calls[0]


@pytest.mark.skipif(not sa.ffmpeg_available(), reason="ffmpeg not installed")
@pytest.mark.parametrize("name,ext,kbps", [("aac", "m4a", 96), ("libmp3lame", "mp3", 128)])
def test_real_codec_keeps_length_and_alignment(name, ext, kbps):
    x = tone(2.0, 1000.0) * np.hanning(2 * RATE).astype(np.float32)
    y = sa.codec_round_trip(x, RATE, {"name": name, "ext": ext, "kbps": kbps})
    assert len(y) == len(x)
    lag = np.argmax(np.correlate(y[: RATE // 2], x[: RATE // 2], "full")) - (RATE // 2 - 1)
    assert abs(int(lag)) < 64  # decoder delay is compensated (under 1.5 ms)
    assert synth_wav.rms(y) == pytest.approx(synth_wav.rms(x), rel=0.15)
