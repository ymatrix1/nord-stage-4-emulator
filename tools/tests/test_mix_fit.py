import struct
from pathlib import Path

import mix_fit
import numpy as np

SR = 48_000


def write_wav(path: Path, x: np.ndarray) -> None:
    pcm = (np.clip(x, -1, 1) * 32767).astype("<i2").tobytes()
    hdr = b"RIFF" + struct.pack("<I", 36 + len(pcm)) + b"WAVEfmt "
    hdr += struct.pack("<IHHIIHH", 16, 1, 1, SR, SR * 2, 2, 16) + b"data"
    path.write_bytes(hdr + struct.pack("<I", len(pcm)) + pcm)


def tone(hz: float, amp: float, secs: float = 2.0) -> np.ndarray:
    t = np.arange(int(SR * secs)) / SR
    return amp * np.sin(2 * np.pi * hz * t)


def test_band_power_lands_in_the_right_third_octave():
    centres = mix_fit.band_centres(63.0, 21)
    p = mix_fit.third_octave_power(tone(1000.0, 0.5), SR, centres)
    assert centres[int(np.argmax(p))] == pytest_approx_band(centres, 1000.0)
    assert p.max() > 1e3 * np.sort(p)[-3]


def pytest_approx_band(centres: np.ndarray, hz: float) -> float:
    return float(centres[np.argmin(np.abs(np.log2(centres / hz)))])


def test_nnls_recovers_known_mix_and_rejects_negative_parts():
    rng = np.random.default_rng(1)
    parts = rng.uniform(0.5, 2.0, size=(20, 2))
    target = 3.0 * parts[:, 0] + 0.5 * parts[:, 1]
    c = mix_fit.nnls_log(target, parts)
    assert np.allclose(c, [3.0, 0.5], rtol=1e-6)
    only_first = 2.0 * parts[:, 0]
    c = mix_fit.nnls_log(only_first, parts)
    assert np.allclose(c, [2.0, 0.0], atol=1e-6)


def test_fit_report_splits_two_tones(tmp_path: Path):
    low, high = tone(200.0, 0.2), tone(3000.0, 0.1)
    write_wav(tmp_path / "hw.wav", 2.0 * low + high)
    write_wav(tmp_path / "a.wav", low)
    write_wav(tmp_path / "b.wav", high)
    r = mix_fit.fit_report(tmp_path / "hw.wav", [tmp_path / "a.wav", tmp_path / "b.wav"])
    assert np.allclose(r["coef"], [4.0, 1.0], rtol=0.05)
    assert np.all(np.abs(r["fit_error_db"][np.asarray(r["hw_db"]) > 20]) < 0.5)
