"""Tests for tools/synth_wav.py."""

from __future__ import annotations

import struct
import sys
from pathlib import Path

import numpy as np
import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import synth_wav as sw  # noqa: E402


def write_float_wav(path: Path, data: np.ndarray, rate: int = 44100) -> None:
    """Stereo/mono float32 WAV like ``ns4 render-patch`` writes."""
    ch = data.shape[1]
    body = data.astype("<f4").tobytes()
    fmt = struct.pack("<HHIIHH", 3, ch, rate, rate * ch * 4, ch * 4, 32)
    riff = b"WAVE" + b"fmt " + struct.pack("<I", 16) + fmt + b"data" + struct.pack("<I", len(body))
    path.write_bytes(b"RIFF" + struct.pack("<I", len(riff) + len(body)) + riff + body)


def test_float_stereo_round_trip_to_mono_int16(tmp_path):
    left = np.linspace(-0.5, 0.5, 1000, dtype=np.float32)
    x = np.stack([left, left * 0.0], axis=1)
    src = tmp_path / "in.wav"
    write_float_wav(src, x)
    arr, rate = sw.read_wav(src)
    assert rate == 44100 and arr.shape == (1000, 2)
    mono = sw.to_mono_int16(arr)
    assert mono.dtype == np.dtype("<i2")
    assert np.allclose(mono / 32767.0, left / 2, atol=2e-4)  # channel mean
    dst = tmp_path / "out.wav"
    sw.write_wav_int16(dst, mono)
    back, rate2 = sw.read_wav(dst)
    assert rate2 == 44100 and back.shape == (1000, 1)
    assert np.allclose(back[:, 0], mono / 32768.0, atol=1e-4)


def test_clipping_to_full_scale():
    mono = sw.to_mono_int16(np.array([[2.0, 2.0], [-3.0, -3.0]], dtype=np.float32))
    assert list(mono) == [32767, -32767]


def test_rms_and_empty():
    assert sw.rms(np.zeros(0)) == 0.0
    assert sw.rms(np.full(100, 0.5, dtype=np.float32)) == pytest.approx(0.5)


def test_bad_files_rejected(tmp_path):
    bad = tmp_path / "bad.wav"
    bad.write_bytes(b"not a wav at all")
    with pytest.raises(ValueError):
        sw.read_wav(bad)


def test_resample_fft_keeps_a_tone_and_length():
    t = np.arange(48000) / 48000
    x = np.sin(2 * np.pi * 1000 * t).astype(np.float32)
    y = sw.resample_fft(x, 48000, 44100)
    assert len(y) == 44100
    ref = np.sin(2 * np.pi * 1000 * np.arange(44100) / 44100)
    assert np.max(np.abs(y[200:-200] - ref[200:-200])) < 2e-3
    assert sw.resample_fft(x, 48000, 48000) is x
    up = sw.resample_fft(y, 44100, 48000)
    assert len(up) == 48000
