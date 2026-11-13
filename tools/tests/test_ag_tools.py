"""Tests for the lesson-video tools: ag_frames, ag_audio, ag_pitch."""

from __future__ import annotations

import wave

import ag_audio
import ag_frames
import ag_pitch
import numpy as np
import pytest

SR = 48_000


def _write_wav(path, x: np.ndarray) -> None:
    pcm = (np.clip(x, -1, 1) * 32767).astype(np.int16)
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(pcm.tobytes())


def test_parse_box_and_rejects_bad_input():
    assert ag_frames.parse_box("10,20,30,40") == (10, 20, 30, 40)
    with pytest.raises(ValueError):
        ag_frames.parse_box("1,2,3")
    with pytest.raises(ValueError):
        ag_frames.parse_box("1,2,0,4")


def test_settled_indices_pick_the_end_of_each_change():
    change = np.array([0, 0, 9, 9, 0, 0, 0, 7, 0, 0, 0], np.float32)
    assert ag_frames.settled_indices(change, thr=2.0, hold=2) == [0, 5, 9]


def test_frame_change_and_sheet_shape():
    a = np.zeros((4, 6, 8, 3), np.uint8)
    a[2:] = 200
    d = ag_frames.frame_change(a)
    assert d[0] == 0 and d[2] == pytest.approx(200) and d[1] == 0
    sheet = ag_frames.make_sheet(list(a), ["a", "b", "c", "d"], cols=2, scale=2.0)
    assert sheet.size == (2 * 16, 2 * (12 + 14))
    with pytest.raises(ValueError):
        ag_frames.make_sheet([], [])


def test_load_wav_roundtrip(tmp_path):
    x = 0.5 * np.sin(2 * np.pi * 440 * np.arange(SR // 10) / SR)
    _write_wav(tmp_path / "a.wav", x)
    sr, y = ag_audio.load_wav(tmp_path / "a.wav")
    assert sr == SR and y.shape == (SR // 10, 1)
    assert np.abs(y[:, 0] - x).max() < 1e-3


def test_peak_and_centroid_tracks_find_a_sine():
    t = np.arange(SR // 2) / SR
    x = np.sin(2 * np.pi * 1234.0 * t).astype(np.float32)
    mag = ag_audio.stft_mag(x, 4096, 960)
    pk = ag_audio.peak_track(mag, SR, 4096, 200, 5000)
    assert np.abs(pk - 1234.0).max() < 2.0
    cen = ag_audio.centroid_track(mag, SR, 4096, 200, 5000)
    assert np.abs(cen - 1234.0).max() < 30.0
    db = ag_audio.band_db(mag, SR, 4096, 1000, 1500)
    assert db.min() > ag_audio.band_db(mag, SR, 4096, 3000, 3500).max() + 40


def test_spectrogram_png_is_written(tmp_path):
    x = np.random.default_rng(1).standard_normal(SR // 4).astype(np.float32)
    mag = ag_audio.stft_mag(x, 1024, 480)
    out = tmp_path / "s.png"
    ag_audio.spectrogram_png(mag, SR, 1024, 480, out, fmax=4000.0, height=64)
    assert out.stat().st_size > 0


def _vibrato_note(rate: float, depth_cents: float, delay: float = 0.0, seconds: float = 2.0):
    t = np.arange(int(seconds * SR)) / SR
    amp = np.clip((t - delay) / 0.4, 0, 1)
    f = 392.0 * 2 ** (amp * depth_cents * np.sin(2 * np.pi * rate * t) / 1200)
    return t, np.sin(2 * np.pi * np.cumsum(f) / SR).astype(np.float32)


def test_vibrato_stats_recover_rate_depth_and_onset():
    _, x = _vibrato_note(5.0, 40.0, delay=0.8)
    t, c = ag_pitch.partial_cents(x, SR, 392.0, 1, n=4096, span_cents=400)
    s = ag_pitch.vibrato_stats(t, c)
    assert s["rate_hz"] == pytest.approx(5.0, abs=0.15)
    assert s["depth_cents"] == pytest.approx(40.0, rel=0.1)
    assert 0.7 < s["onset_s"] < 1.3


def test_vibrato_stats_of_a_flat_note_is_zero():
    _, x = _vibrato_note(5.0, 0.0)
    t, c = ag_pitch.partial_cents(x, SR, 392.0, 1, n=4096, span_cents=400)
    assert ag_pitch.vibrato_stats(t, c)["depth_cents"] < 3.0


def test_exp_decay_fit_recovers_tau():
    t = np.linspace(0, 0.1, 40)
    a, tau = ag_pitch.exp_decay_fit(t, 1200 * np.exp(-t / 0.022))
    assert a == pytest.approx(1200, rel=1e-3) and tau == pytest.approx(0.022, rel=1e-3)
    assert np.isnan(ag_pitch.exp_decay_fit(t, np.full_like(t, 1.0))[0])


def test_cli_entry_points_run(tmp_path, capsys):
    _, x = _vibrato_note(5.0, 40.0, seconds=1.0)
    _write_wav(tmp_path / "v.wav", x)
    assert (
        ag_pitch.main(
            [
                str(tmp_path / "v.wav"),
                "--start",
                "0.0",
                "--end",
                "1.0",
                "--fref",
                "392",
                "--harmonic",
                "1",
                "--n",
                "4096",
            ]
        )
        == 0
    )
    assert "rate_hz" in capsys.readouterr().out
    png = tmp_path / "s.png"
    assert ag_audio.main([str(tmp_path / "v.wav"), "--png", str(png), "--hop", "960"]) == 0
    assert png.exists()
