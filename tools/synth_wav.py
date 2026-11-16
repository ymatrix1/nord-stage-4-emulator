#!/usr/bin/env python3
"""WAV helpers for the synth dataset: ``ns4 render-patch`` output to Transkun's input format.

``render-patch`` writes stereo 32-bit float WAV. Transkun reads integer PCM
(``transkun.Data.readAudioSlice`` divides by the dtype maximum), so the dataset stores mono
16-bit 44.1 kHz. Standard library plus numpy only.
"""

from __future__ import annotations

import struct
import wave
from pathlib import Path

import numpy as np

PCM, FLOAT = 1, 3
SAMPLE_RATE = 44100


def read_wav(path: Path) -> tuple[np.ndarray, int]:
    """Returns ``(float32 array [n, channels] in -1..1, sample rate)``; PCM16/32 or float32."""
    raw = Path(path).read_bytes()
    if raw[:4] != b"RIFF" or raw[8:12] != b"WAVE":
        raise ValueError(f"{path}: not a RIFF/WAVE file")
    pos, fmt, data = 12, None, None
    while pos + 8 <= len(raw):
        cid, size = raw[pos : pos + 4], struct.unpack_from("<I", raw, pos + 4)[0]
        body = raw[pos + 8 : pos + 8 + size]
        if cid == b"fmt ":
            fmt = struct.unpack_from("<HHIIHH", body)
        elif cid == b"data":
            data = body
        pos += 8 + size + (size & 1)
    if fmt is None or data is None:
        raise ValueError(f"{path}: missing fmt or data chunk")
    tag, ch, rate, _, _, bits = fmt
    if tag == FLOAT and bits == 32:
        arr = np.frombuffer(data, dtype="<f4").astype(np.float32)
    elif tag == PCM and bits == 16:
        arr = np.frombuffer(data, dtype="<i2").astype(np.float32) / 32768.0
    elif tag == PCM and bits == 32:
        arr = np.frombuffer(data, dtype="<i4").astype(np.float32) / 2147483648.0
    else:
        raise ValueError(f"{path}: unsupported format tag={tag} bits={bits}")
    return arr[: len(arr) // ch * ch].reshape(-1, ch), rate


def resample_fft(x: np.ndarray, rate_in: int, rate_out: int) -> np.ndarray:
    """Band-limited resample of a mono signal by spectrum truncation or zero padding.

    ``render-patch`` runs at 48 kHz (VERIFIED: a 60 s render file of 23,035,052 bytes is
    2,879,381 stereo float32 frames) and Transkun expects 44.1 kHz. The whole signal is
    transformed with 1 s of zero padding each side so the circular edge wrap stays outside the
    kept samples. No scipy dependency.
    """
    if rate_in == rate_out:
        return x
    pad = rate_in
    padded = np.concatenate([np.zeros(pad), x.astype(np.float64), np.zeros(pad)])
    n_out = round(len(padded) * rate_out / rate_in)
    spec = np.fft.rfft(padded)
    out_bins = n_out // 2 + 1
    spec = spec[:out_bins] if out_bins <= len(spec) else np.pad(spec, (0, out_bins - len(spec)))
    y = np.fft.irfft(spec, n_out) * (n_out / len(padded))
    lo = round(pad * rate_out / rate_in)
    return y[lo : lo + round(len(x) * rate_out / rate_in)].astype(np.float32)


def to_mono_int16(x: np.ndarray) -> np.ndarray:
    """Channel mean, clipped to full scale, rounded to int16."""
    mono = x.mean(axis=1) if x.ndim == 2 else x
    return np.round(np.clip(mono, -1.0, 1.0) * 32767.0).astype("<i2")


def write_wav_int16(path: Path, samples: np.ndarray, rate: int = SAMPLE_RATE) -> None:
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(rate)
        w.writeframes(samples.astype("<i2").tobytes())


def rms(x: np.ndarray) -> float:
    return float(np.sqrt(np.mean(np.square(x, dtype=np.float64)))) if x.size else 0.0
