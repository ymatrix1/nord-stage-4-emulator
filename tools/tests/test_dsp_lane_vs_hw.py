import json
import wave

import dsp_lane_vs_hw as d
import numpy as np
import pytest

SR = 48_000


def write_wav(path, samples, sr=SR):
    pcm = (np.clip(samples, -1, 1) * 32767).astype("<i2")
    stereo = np.stack([pcm, pcm], axis=1).reshape(-1)
    with wave.open(str(path), "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(sr)
        w.writeframes(stereo.tobytes())


def tone(freq, amp, seconds=1.0):
    t = np.arange(int(seconds * SR)) / SR
    return amp * np.sin(2 * np.pi * freq * t)


def test_identical_clips_have_zero_gap(tmp_path):
    a = tmp_path / "a.wav"
    write_wav(a, tone(440, 0.3))
    r = d.compare(a, a)
    assert r["rms_delta_db"] == 0 and r["gap_score_db"] == 0
    assert set(r["bands_delta_db"]) >= {"low_120_400", "mid_400_1500"}


def test_a_quieter_lower_tone_shows_level_and_band_deltas(tmp_path):
    hw, emu = tmp_path / "hw.wav", tmp_path / "emu.wav"
    write_wav(hw, tone(1000, 0.4))
    write_wav(emu, tone(200, 0.1))
    r = d.compare(hw, emu)
    assert r["rms_delta_db"] == pytest.approx(-12.0, abs=0.2)
    assert r["emu_centroid_hz"] < r["hw_centroid_hz"]
    assert r["bands_delta_db"]["low_120_400"] > 0 > r["bands_delta_db"]["mid_400_1500"]
    assert r["gap_score_db"] > 10
    assert r["worst_band"][0] in d.emu_vs_hw.SCORE_BANDS


def test_segment_lookup_reads_the_spec(tmp_path):
    spec = tmp_path / "s.json"
    spec.write_text(json.dumps({"segments": [{"id": "x", "at_s": 3, "duration_s": 4}]}))
    assert d.load_segment(spec, "x")["at_s"] == 3
    with pytest.raises(KeyError):
        d.load_segment(spec, "y")
    real = d.load_segment(d.DEFAULT_SPEC, "organ_b3")
    assert real["duration_s"] > 0


def test_cut_needs_ffmpeg(tmp_path, monkeypatch):
    monkeypatch.setattr(d.shutil, "which", lambda _name: None)
    with pytest.raises(RuntimeError):
        d.cut_hardware_wav(tmp_path / "v.webm", 0, 1, tmp_path / "o.wav")
