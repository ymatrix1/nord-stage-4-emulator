import json
import wave
from pathlib import Path

import numpy as np
import organ_gap as og

SR = 48_000


def write_wav(path: Path, x: np.ndarray) -> None:
    pcm = (np.clip(x, -1, 1) * 32767).astype("<i2")
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(pcm.tobytes())


def harmonic_tone(seconds: float = 2.0, f0: float = 220.0, n: int = 12) -> np.ndarray:
    t = np.arange(int(seconds * SR)) / SR
    return sum(np.sin(2 * np.pi * f0 * k * t) / k for k in range(1, n + 1)) * 0.1


def test_peakiness_separates_a_line_spectrum_from_noise():
    rng = np.random.default_rng(5)
    noise = 0.1 * rng.standard_normal(2 * SR)
    tone = og.tonal_peakiness_db(harmonic_tone(), SR, 400, 3000)
    flat = og.tonal_peakiness_db(noise, SR, 400, 3000)
    assert tone is not None and flat is not None
    assert tone > flat + 8.0, (tone, flat)


def test_peakiness_none_for_a_clip_shorter_than_one_frame():
    assert og.tonal_peakiness_db(np.zeros(1000), SR, 400, 3000) is None


def test_band_table_puts_a_1khz_tone_in_its_band():
    t = np.arange(2 * SR) / SR
    rows = og.third_band_table(0.2 * np.sin(2 * np.pi * 1200 * t), SR)
    best = max(rows, key=lambda r: r[2])
    assert (best[0], best[1]) == (1000, 1500)
    assert best[2] > -1.0


def test_register_histogram_counts_octaves_confirmed_and_all(tmp_path):
    ev = [
        {"midi": 40, "confirmed": True},
        {"midi": 45, "confirmed": False},
        {"midi": 62, "confirmed": False},
    ]
    p = tmp_path / "events.json"
    p.write_text(json.dumps({"events": ev}))
    h = og.register_histogram(p)
    assert h["all"] == [(3, 2), (5, 1)]
    assert h["confirmed"] == [(3, 1)]


def test_report_and_main_read_hw_and_emulator_wavs(tmp_path, capsys):
    write_wav(tmp_path / "hw.wav", harmonic_tone())
    write_wav(tmp_path / "emu_a.wav", harmonic_tone(f0=196.0, n=4))
    (tmp_path / "events.json").write_text(json.dumps({"events": [{"midi": 57}]}))
    assert og.main([str(tmp_path), "emu_a.wav"]) == 0
    out = capsys.readouterr().out
    assert "| band Hz |" in out and "peakiness dB" in out and "notes per octave" in out


def test_main_stops_when_a_wav_is_missing(tmp_path, capsys):
    assert og.main([str(tmp_path), "nope.wav"]) == 2
    assert "missing" in capsys.readouterr().err
