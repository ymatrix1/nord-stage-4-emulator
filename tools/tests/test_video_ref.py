"""Unit tests for tools/video_ref.py."""

from __future__ import annotations

import json
import math
import struct
import wave
from pathlib import Path

import pytest
import video_ref
from PIL import Image, ImageDraw


def _write_pcm16_wav(path: Path, sr: int, left: list[float], right: list[float]) -> None:
    with wave.open(str(path), "wb") as wf:
        wf.setnchannels(2)
        wf.setsampwidth(2)
        wf.setframerate(sr)
        frames = bytearray()
        for lv, rv in zip(left, right):
            li = max(-32768, min(32767, int(round(lv * 32767.0))))
            ri = max(-32768, min(32767, int(round(rv * 32767.0))))
            frames.extend(struct.pack("<hh", li, ri))
        wf.writeframes(bytes(frames))


def _write_extensible_f32_wav(path: Path, sr: int, left: list[float], right: list[float]) -> None:
    data = bytearray()
    for lv, rv in zip(left, right):
        data.extend(struct.pack("<ff", lv, rv))
    fmt_payload = (
        struct.pack(
            "<HHIIHHHHI",
            0xFFFE,  # WAVE_FORMAT_EXTENSIBLE
            2,
            sr,
            sr * 8,
            8,
            32,
            22,
            32,
            3,
        )
        + b"\x03\x00\x00\x00\x00\x00\x10\x00\x80\x00\x00\xaa\x00\x38\x9b\x71"
    )
    riff_size = 4 + (8 + len(fmt_payload)) + (8 + len(data))
    header = (
        b"RIFF"
        + struct.pack("<I", riff_size)
        + b"WAVE"
        + b"fmt "
        + struct.pack("<I", len(fmt_payload))
        + fmt_payload
        + b"data"
        + struct.pack("<I", len(data))
    )
    path.write_bytes(header + bytes(data))


def test_extract_video_id_and_timestamps():
    url = "https://www.youtube.com/watch?v=_zksT69H30c"
    assert video_ref.extract_video_id(url) == "_zksT69H30c"
    assert video_ref.extract_video_id("https://youtu.be/_zksT69H30c?t=445") == "_zksT69H30c"
    assert video_ref.extract_video_id("https://www.youtube.com/shorts/AbCdEf_1234") == "AbCdEf_1234"
    assert video_ref.extract_video_id("_zksT69H30c") == "_zksT69H30c"
    assert video_ref.extract_video_id("part_a") == "_zksT69H30c"
    assert video_ref.extract_video_id("DEMO [_zksT69H30c].webm") == "_zksT69H30c"
    assert video_ref.extract_video_id("not-a-valid-video") is None

    assert video_ref.parse_timestamp("07:25") == 445.0
    assert video_ref.parse_timestamp("1:02:03.5") == 3723.5
    assert video_ref.parse_timestamp(12.5) == 12.5
    assert video_ref.format_timestamp(445) == "07:25"
    assert video_ref.format_timestamp(3723) == "01:02:03"
    assert video_ref.timestamp_slug(445) == "07m25s"
    with pytest.raises(ValueError):
        video_ref.parse_timestamp("-5")
    with pytest.raises(ValueError):
        video_ref.parse_timestamp("1:2:3:4")


def test_find_local_video_and_sources_registry(tmp_path: Path):
    cached = tmp_path / "NEW NORD STAGE 4 [_zksT69H30c].webm"
    cached.write_bytes(b"fake-webm")
    found = video_ref.find_local_video("_zksT69H30c", search_dirs=[tmp_path])
    assert found == cached

    reg_path = tmp_path / "sources.json"
    video_ref.save_source_entry(
        {"id": "TestVid1234", "url": "https://youtu.be/TestVid1234", "title": "Part B"},
        registry_path=reg_path,
    )
    reg = video_ref.load_sources_registry(registry_path=reg_path)
    assert "_zksT69H30c" in reg
    assert reg["TestVid1234"]["title"] == "Part B"


def test_detect_red_panel_box_and_roi_crops(tmp_path: Path):
    im = Image.new("RGB", (640, 360), color=(22, 20, 20))
    draw = ImageDraw.Draw(im)
    # Warm salmon shirt upper center (must be ignored by red-excess detector)
    draw.rectangle((260, 40, 380, 170), fill=(195, 130, 110))
    # Nord Red top chassis box: x in [40, 600), y in [190, 280)
    draw.rectangle((40, 190, 600, 280), fill=(185, 22, 24))
    # Dark OLED window inside panel
    draw.rectangle((290, 215, 340, 245), fill=(10, 12, 16))
    # White keybed directly below
    draw.rectangle((44, 282, 596, 355), fill=(240, 240, 232))

    x0, y0, x1, y1 = video_ref.detect_red_panel_box(im)
    assert 30 <= x0 <= 55
    assert 180 <= y0 <= 200
    assert 585 <= x1 <= 610
    assert 270 <= y1 <= 290

    frame_path = tmp_path / "frame.png"
    im.save(frame_path)
    crops = video_ref.extract_roi_crops(
        frame_path,
        outdir=tmp_path / "crops",
        prefix="07m25s",
        rois=("panel", "oled_zoom", "piano", "fx", "reverb", "keybed"),
    )
    assert set(crops.keys()) == {"panel", "oled_zoom", "piano", "fx", "reverb", "keybed"}
    for roi_path in crops.values():
        assert roi_path.is_file()
        assert roi_path.stat().st_size > 100


def test_compose_storyboard_grid(tmp_path: Path):
    tiles = []
    for idx, color in enumerate([(180, 20, 20), (20, 160, 40), (30, 50, 190)]):
        p = tmp_path / f"tile_{idx}.png"
        Image.new("RGB", (160, 90), color=color).save(p)
        tiles.append((float(idx * 30), p))
    sb = video_ref.compose_storyboard_grid(tiles, tmp_path / "sb.png", cols=2, tile_width=180)
    assert sb.is_file()
    out = Image.open(sb)
    assert out.width > 360
    assert out.height > 200


def test_analyze_and_compare_pcm_and_extensible_float_wav(tmp_path: Path):
    sr = 48000
    n = 48000
    hw_l = [0.5 * math.sin(2.0 * math.pi * 440.0 * i / sr) for i in range(n)]
    hw_r = list(hw_l)
    hw_wav = tmp_path / "hw_440.wav"
    _write_pcm16_wav(hw_wav, sr, hw_l, hw_r)

    emu_l = [0.5 * math.sin(2.0 * math.pi * 4000.0 * i / sr) for i in range(n)]
    emu_r = [-0.5 * math.sin(2.0 * math.pi * 4000.0 * i / sr) for i in range(n)]
    emu_wav = tmp_path / "emu_4k_wide.wav"
    _write_extensible_f32_wav(emu_wav, sr, emu_l, emu_r)

    hw_t = video_ref.analyze_wav(hw_wav)
    assert abs(hw_t.rms_dbfs - (-9.03)) < 0.25
    assert hw_t.stereo_width_ratio < 0.01
    assert 410.0 <= hw_t.spectral_centroid_hz <= 470.0
    assert hw_t.bands_rel_db["mid_400_1500"] > -1.0
    assert hw_t.bands_rel_db["brilliance_3000_8000"] < -35.0

    # For anti-phase stereo sine, mono sum cancels so measure directly on float stereo read
    sr2, rl, rr = video_ref.read_wav_stereo(emu_wav)
    assert sr2 == sr
    assert abs(rl[10] + rr[10]) < 1e-6

    # Compare non-inverted 4k stereo signal for spectral band verification
    _write_extensible_f32_wav(emu_wav, sr, emu_l, [0.5 * x for x in emu_l])
    report = video_ref.compare_wavs(hw_wav, emu_wav)
    assert report["emulator"]["bands_rel_db"]["brilliance_3000_8000"] > -1.0
    assert report["delta_emu_minus_hw"]["spectral_centroid_hz"] > 3000.0


def test_cli_sources_and_analyze_json(tmp_path: Path, capsys: pytest.CaptureFixture[str]):
    wav_path = tmp_path / "tone.wav"
    n = 16000
    tone = [0.25 * math.sin(2.0 * math.pi * 500.0 * i / 48000) for i in range(n)]
    _write_pcm16_wav(wav_path, 48000, tone, tone)

    assert video_ref.main(["--json", "sources"]) == 0
    src_out = json.loads(capsys.readouterr().out)
    assert "_zksT69H30c" in src_out

    assert video_ref.main(["--json", "analyze", str(wav_path)]) == 0
    ana_out = json.loads(capsys.readouterr().out)
    assert ana_out["sample_rate"] == 48000
    assert 470.0 <= ana_out["spectral_centroid_hz"] <= 530.0
