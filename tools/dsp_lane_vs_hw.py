#!/usr/bin/env python3
"""Compare a SHARC-oracle output lane WAV (``sharc-sys --lane --wav``) with a hardware clip.

The hardware slice is cut from the local Part A demo video (``ffmpeg``, never committed) and
both files go through ``video_ref.compare_wavs`` (level, stereo width, spectral centroid, 6-band
spectrum relative to RMS). The ranking number is ``emu_vs_hw.gap_score`` (mean absolute band
delta in dB without the air band). The oracle lane is the dry DSP output before the FX chip, the
hardware clip is the final stereo mix, so the numbers are shape and level evidence only
(``docs/research/dsp-peripherals-audio.md`` section 5). Audio stays in ``--workdir``.
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
from pathlib import Path
from typing import Any

import emu_vs_hw
import video_ref

DEFAULT_SPEC = Path("docs/research/data/emu_vs_hw_segments.json")


def load_segment(spec: Path, seg_id: str) -> dict[str, Any]:
    """The segment ``seg_id`` of the emu-vs-hw spec file (``at_s`` and ``duration_s`` inside)."""
    doc = json.loads(spec.read_text(encoding="utf-8"))
    for seg in doc["segments"]:
        if seg["id"] == seg_id:
            return seg
    raise KeyError(f"no segment {seg_id!r} in {spec}")


def cut_hardware_wav(video: Path, at_s: float, duration_s: float, out: Path) -> Path:
    """48 kHz stereo 16-bit WAV of ``duration_s`` seconds from ``at_s`` of ``video``."""
    ffmpeg = shutil.which("ffmpeg")
    if ffmpeg is None:
        raise RuntimeError("ffmpeg not found on PATH")
    out.parent.mkdir(parents=True, exist_ok=True)
    cmd = [ffmpeg, "-y", "-ss", f"{at_s:.3f}", "-t", f"{duration_s:.3f}", "-i", str(video)]
    cmd += ["-vn", "-ac", "2", "-ar", "48000", "-c:a", "pcm_s16le", str(out)]
    subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return out


def compare(hw_wav: Path, emu_wav: Path) -> dict[str, Any]:
    """Level, centroid, width and band deltas (oracle minus hardware) plus the gap score."""
    rep = video_ref.compare_wavs(hw_wav, emu_wav)
    delta = rep["delta_emu_minus_hw"]
    bands = {k: delta["bands_rel_db"][k] for k in emu_vs_hw.BAND_NAMES}
    score = emu_vs_hw.gap_score({"bands_delta_db": bands})
    worst = max(emu_vs_hw.SCORE_BANDS, key=lambda k: abs(bands[k]))
    return {
        "hw_rms_dbfs": rep["hardware"]["rms_dbfs"],
        "emu_rms_dbfs": rep["emulator"]["rms_dbfs"],
        "rms_delta_db": delta["rms_dbfs"],
        "hw_centroid_hz": rep["hardware"]["spectral_centroid_hz"],
        "emu_centroid_hz": rep["emulator"]["spectral_centroid_hz"],
        "width_hw": rep["hardware"]["stereo_width_ratio"],
        "width_emu": rep["emulator"]["stereo_width_ratio"],
        "bands_delta_db": bands,
        "gap_score_db": score,
        "worst_band": [worst, bands[worst]],
    }


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--video", required=True, help="Local Part A video (.webm).")
    p.add_argument("--workdir", required=True, help="Scratch dir for the hardware WAV.")
    p.add_argument("--segment", default="organ_b3", help="Segment id of the spec file.")
    p.add_argument("--spec", type=Path, default=DEFAULT_SPEC)
    p.add_argument(
        "--slice-s", type=float, default=0.0, help="Use only this many seconds (0 = all)."
    )
    p.add_argument("emu_wav", nargs="+", type=Path, help="Oracle lane WAV files.")
    args = p.parse_args()
    seg = load_segment(args.spec, args.segment)
    dur = args.slice_s or float(seg["duration_s"])
    hw = cut_hardware_wav(
        Path(args.video), float(seg["at_s"]), dur, Path(args.workdir) / f"hw_{args.segment}.wav"
    )
    out = {w.name: compare(hw, w) for w in args.emu_wav}
    print(json.dumps(out, indent=2))


if __name__ == "__main__":
    main()
