from __future__ import annotations

import math
import random
import struct
import wave
from pathlib import Path

import fw_audio_scan as fa
import pytest


def sine_bytes(n: int, amp: int = 8000) -> bytes:
    return struct.pack(f"<{n}h", *[int(amp * math.sin(2 * math.pi * i / 97.0)) for i in range(n)])


def noise_bytes(n: int) -> bytes:
    rng = random.Random(1)
    return bytes(rng.randrange(256) for _ in range(n))


def test_adjacent_correlation_separates_audio_from_noise_and_handles_edges():
    audio = fa.int16_samples(sine_bytes(2048), 0, 4096)
    noise = fa.int16_samples(noise_bytes(4096), 0, 4096)
    assert fa.adjacent_correlation(audio) > 0.95
    assert abs(fa.adjacent_correlation(noise)) < 0.2
    assert fa.adjacent_correlation([5, 5, 5, 5]) == 0.0
    with pytest.raises(ValueError):
        fa.adjacent_correlation([1, 2])
    with pytest.raises(ValueError):
        fa.int16_samples(b"\0" * 10, 4, 100)


def test_scan_finds_and_merges_the_audio_region():
    image = noise_bytes(8192) + sine_bytes(4096) + noise_bytes(8192)  # audio at 8192..16384
    windows = fa.scan_audio_windows(image)
    assert [off for off, _ in windows] == [8192, 12288]
    assert fa.merge_regions(windows) == [(8192, 16384)]
    assert fa.merge_regions([]) == []


def test_deinterleave_splits_channels_and_drops_the_partial_frame():
    assert fa.deinterleave([1, 2, 3, 4, 5, 6, 7], 3) == [[1, 4], [2, 5], [3, 6]]
    with pytest.raises(ValueError):
        fa.deinterleave([1], 0)


def test_write_wav_and_cli_roundtrip(tmp_path: Path, capsys):
    img = tmp_path / "fw.bin"
    img.write_bytes(noise_bytes(4096) + sine_bytes(4096))
    assert fa.main(["scan", str(img)]) == 0
    assert "0x1000..0x3000" in capsys.readouterr().out
    out = tmp_path / "out"
    assert (
        fa.main(
            [
                "export",
                str(img),
                str(out),
                "--offset",
                "0x1000",
                "--length",
                "0x2000",
                "--channels",
                "2",
            ]
        )
        == 0
    )
    files = sorted(out.glob("ch*.wav"))
    assert len(files) == 2
    with wave.open(str(files[0])) as w:
        assert w.getnchannels() == 1 and w.getframerate() == 48_000 and w.getnframes() == 2048
