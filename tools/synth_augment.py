#!/usr/bin/env python3
"""Audio augmentation for the synth fine-tuning dataset (``tools/synth_dataset.py``).

The hardware clips are YouTube video audio: lossy-coded, of unknown gain and EQ, with a noise
floor. The emulator renders are clean. Each item draws one plan (seeded) and applies, in order:
spectral tilt, codec round trip (ffmpeg), noise floor, final peak level.

Provenance (``.agents/rules/provenance-in-code.md``): every range below is ASSUMED (plausible
video-audio conditions, not measured from the clips). What would confirm them: compare the
long-term spectrum and noise floor of the hardware clips with the augmented renders.

- tilt: -2 to +2 dB per octave around 1 kHz, 60 % of items (clamped to 100 Hz to 10 kHz)
- codec: AAC 64 to 192 kbit/s, MP3 96 to 128, Opus 48 to 96, 50 % of items
- noise: pink noise at 30 to 60 dB below the signal RMS, 50 % of items
- level: final peak between -24 and -1 dBFS, always (Transkun normalises gain per segment)
"""

from __future__ import annotations

import random
import shutil
import subprocess
import tempfile
from collections.abc import Callable
from pathlib import Path
from typing import Any

import numpy as np
import synth_wav

CODECS = (
    ("aac", "m4a", (64, 96, 128, 192)),
    ("libmp3lame", "mp3", (96, 128)),
    ("libopus", "opus", (48, 64, 96)),
)
Runner = Callable[[list[str]], None]


def ffmpeg_available() -> bool:
    return shutil.which("ffmpeg") is not None


def run_ffmpeg(cmd: list[str]) -> None:
    subprocess.run(cmd, check=True, capture_output=True)


def plan_augment(seed: int, enabled: bool = True) -> dict[str, Any]:
    """Seeded augmentation plan; ``enabled=False`` keeps only a neutral -1 dBFS level."""
    rng = random.Random(seed ^ 0xA06)
    plan: dict[str, Any] = {"tilt_db_oct": 0.0, "codec": None, "noise_snr_db": None}
    if not enabled:
        return {**plan, "peak_dbfs": -1.0}
    if rng.random() < 0.6:
        plan["tilt_db_oct"] = round(rng.uniform(-2.0, 2.0), 2)
    if rng.random() < 0.5:
        name, ext, rates = rng.choice(CODECS)
        plan["codec"] = {"name": name, "ext": ext, "kbps": rng.choice(rates)}
    if rng.random() < 0.5:
        plan["noise_snr_db"] = round(rng.uniform(30.0, 60.0), 1)
    plan["peak_dbfs"] = round(rng.uniform(-24.0, -1.0), 1)
    return plan


def apply_tilt(x: np.ndarray, rate: int, tilt_db_oct: float) -> np.ndarray:
    """Spectral tilt in dB per octave around 1 kHz, RMS preserved."""
    if tilt_db_oct == 0.0 or x.size == 0:
        return x
    freqs = np.fft.rfftfreq(len(x), 1.0 / rate)
    f = np.clip(freqs, 100.0, 10000.0)
    gain = 10.0 ** (tilt_db_oct * np.log2(f / 1000.0) / 20.0)
    y = np.fft.irfft(np.fft.rfft(x) * gain, len(x))
    scale = synth_wav.rms(x) / max(synth_wav.rms(y), 1e-12)
    return (y * scale).astype(np.float32)


def pink_noise(n: int, rng: np.random.Generator) -> np.ndarray:
    """Unit-RMS pink (1/f power) noise by spectral shaping of white noise."""
    spec = np.fft.rfft(rng.standard_normal(n))
    f = np.maximum(np.arange(len(spec)), 1)
    y = np.fft.irfft(spec / np.sqrt(f), n)
    return (y / max(synth_wav.rms(y), 1e-12)).astype(np.float32)


def add_noise(x: np.ndarray, snr_db: float, seed: int) -> np.ndarray:
    noise = pink_noise(len(x), np.random.default_rng(seed))
    return (x + noise * synth_wav.rms(x) * 10.0 ** (-snr_db / 20.0)).astype(np.float32)


def codec_round_trip(
    x: np.ndarray, rate: int, codec: dict[str, Any], run: Runner = run_ffmpeg
) -> np.ndarray:
    """Encode and decode through ffmpeg; length is forced back to the input length."""
    with tempfile.TemporaryDirectory(prefix="ns4aug_") as tmp:
        src, enc, dst = (Path(tmp) / n for n in ("in.wav", f"enc.{codec['ext']}", "out.wav"))
        synth_wav.write_wav_int16(src, synth_wav.to_mono_int16(x), rate)
        base = ["ffmpeg", "-v", "error", "-y"]
        run([*base, "-i", str(src), "-c:a", codec["name"], "-b:a", f"{codec['kbps']}k", str(enc)])
        run([*base, "-i", str(enc), "-ar", str(rate), "-ac", "1", "-c:a", "pcm_s16le", str(dst)])
        y, _ = synth_wav.read_wav(dst)
    y = y[:, 0]
    return np.pad(y, (0, max(0, len(x) - len(y))))[: len(x)].astype(np.float32)


def apply_plan(
    x: np.ndarray, rate: int, plan: dict[str, Any], seed: int, run: Runner = run_ffmpeg
) -> np.ndarray:
    """Runs the plan on mono float audio and sets the final peak level."""
    y = apply_tilt(x, rate, plan["tilt_db_oct"])
    if plan["codec"] is not None:
        y = codec_round_trip(y, rate, plan["codec"], run)
    if plan["noise_snr_db"] is not None:
        y = add_noise(y, plan["noise_snr_db"], seed)
    peak = float(np.max(np.abs(y))) if y.size else 0.0
    if peak > 0.0:
        y = y * np.float32(10.0 ** (plan["peak_dbfs"] / 20.0) / peak)
    return y
