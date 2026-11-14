import dsm_partials as dp
import numpy as np


def tone(sr, freqs_amps, secs=3.0):
    t = np.arange(int(sr * secs)) / sr
    return sum(a * np.sin(2 * np.pi * f * t) for f, a in freqs_amps)


def test_midi_hz():
    assert abs(dp.midi_hz(69) - 440.0) < 1e-9
    assert abs(dp.midi_hz(60) - 261.6256) < 1e-3


def test_partial_levels_recovers_known_ratios():
    sr = 48000
    f0 = dp.midi_hz(60)
    x = tone(sr, [(f0, 1.0), (2 * f0, 0.5), (4 * f0, 0.25)])
    rel = dp.relative_to(dp.partial_levels(x, sr, 60, 0.2, 2.8))
    assert abs(rel["4'"] - (-6.02)) < 0.3
    assert abs(rel["2'"] - (-12.04)) < 0.3
    assert rel["16'"] < -60


def test_band_db_and_welch_short_input():
    sr = 48000
    x = tone(sr, [(1000.0, 1.0)], secs=0.05)
    f, p = dp.welch_power(x, sr)
    assert len(f) == len(p)
    assert dp.band_db(f, p, 1000.0, 30.0) > dp.band_db(f, p, 3000.0, 30.0) + 30


def test_read_wav_any_float32(tmp_path):
    import struct

    data = np.array([[0.5, -0.5], [0.25, 0.0]], np.float32).tobytes()
    hdr = (
        b"RIFF"
        + struct.pack("<I", 36 + len(data))
        + b"WAVEfmt "
        + struct.pack("<IHHIIHH", 16, 3, 2, 48000, 48000 * 8, 8, 32)
    )
    p = tmp_path / "f.wav"
    p.write_bytes(hdr + b"data" + struct.pack("<I", len(data)) + data)
    sr, x = dp.read_wav_any(p)
    assert sr == 48000 and x.shape == (2, 2) and abs(x[0, 0] - 0.5) < 1e-9
