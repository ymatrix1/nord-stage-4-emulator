#!/usr/bin/env python3
"""Probe the firmware image for embedded 16-bit audio and export it for listening.

Findings so far (``docs/research/synth-0x2f-ns4decode-crosscheck.md``): ``bank0_CODE.bin`` holds
about 1.1 MB of little-endian int16 audio from ``0x300000``; adjacent samples correlate at
0.85..0.99 and the data is four interleaved channels with slowly evolving periodic waveforms.

    python tools/fw_audio_scan.py scan   target/fw_unpacked/bank0_CODE.bin
    python tools/fw_audio_scan.py export target/fw_unpacked/bank0_CODE.bin out_dir \\
        --offset 0x300000 --length 0x120000 --channels 4 --rate 48000

Exported WAVs are Nord content: keep them in a git-ignored folder, never commit them.
"""

from __future__ import annotations

import argparse
import struct
import sys
import wave
from pathlib import Path

WINDOW = 4096  # bytes per scan window


def int16_samples(image: bytes, offset: int, length: int) -> list[int]:
    """Little-endian signed 16-bit samples of `image[offset:offset+length]`."""
    if offset < 0 or offset + length > len(image):
        raise ValueError("range outside the image")
    count = length // 2
    return list(struct.unpack_from(f"<{count}h", image, offset))


def adjacent_correlation(samples: list[int]) -> float:
    """Pearson correlation of neighbouring samples (about 0 for noise/code, near 1 for audio)."""
    if len(samples) < 3:
        raise ValueError("need at least 3 samples")
    a, b = samples[:-1], samples[1:]
    ma, mb = sum(a) / len(a), sum(b) / len(b)
    cov = sum((x - ma) * (y - mb) for x, y in zip(a, b, strict=True))
    va = sum((x - ma) ** 2 for x in a)
    vb = sum((y - mb) ** 2 for y in b)
    if va == 0.0 or vb == 0.0:
        return 0.0
    return cov / (va * vb) ** 0.5


def scan_audio_windows(image: bytes, threshold: float = 0.6) -> list[tuple[int, float]]:
    """`(offset, correlation)` of every 4 KB window that looks like audio."""
    out = []
    for off in range(0, len(image) - WINDOW + 1, WINDOW):
        corr = adjacent_correlation(int16_samples(image, off, WINDOW))
        if corr >= threshold and any(abs(s) > 64 for s in int16_samples(image, off, WINDOW)):
            out.append((off, corr))
    return out


def merge_regions(windows: list[tuple[int, float]], gap: int = 4 * WINDOW) -> list[tuple[int, int]]:
    """Merge nearby audio windows into `(start, end)` byte regions."""
    regions: list[list[int]] = []
    for off, _ in windows:
        if regions and off - regions[-1][1] <= gap:
            regions[-1][1] = off + WINDOW
        else:
            regions.append([off, off + WINDOW])
    return [(a, b) for a, b in regions]


def deinterleave(samples: list[int], channels: int) -> list[list[int]]:
    if channels < 1:
        raise ValueError("channels must be >= 1")
    usable = len(samples) - len(samples) % channels
    return [samples[c:usable:channels] for c in range(channels)]


def write_wav(path: Path, samples: list[int], rate: int) -> None:
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(rate)
        w.writeframes(struct.pack(f"<{len(samples)}h", *samples))


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("scan")
    s.add_argument("image", type=Path)
    s.add_argument("--threshold", type=float, default=0.6)
    e = sub.add_parser("export")
    e.add_argument("image", type=Path)
    e.add_argument("outdir", type=Path)
    e.add_argument("--offset", type=lambda v: int(v, 0), required=True)
    e.add_argument("--length", type=lambda v: int(v, 0), required=True)
    e.add_argument("--channels", type=int, default=1)
    e.add_argument("--rate", type=int, default=48_000)
    args = p.parse_args(argv)
    image = args.image.read_bytes()
    if args.cmd == "scan":
        for a, b in merge_regions(scan_audio_windows(image, args.threshold)):
            print(f"audio-like int16 region 0x{a:x}..0x{b:x} ({(b - a) / 1024:.0f} KiB)")
        return 0
    samples = int16_samples(image, args.offset, args.length)
    args.outdir.mkdir(parents=True, exist_ok=True)
    for ch, data in enumerate(deinterleave(samples, args.channels)):
        out = args.outdir / f"ch{ch}_{args.offset:#x}.wav"
        write_wav(out, data, args.rate)
        print(f"wrote {out} ({len(data)} samples)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
