#!/usr/bin/env python3
"""OLED / panel change sheets for the single-feature lesson videos.

Decodes a cropped region of a video at a low frame rate (ffmpeg raw pipe), finds the
instants where the region changed and then held still (a knob move that settled), and
tiles those frames with timestamps so the displayed parameter values can be read by eye.
Nothing is written next to the video; frames stay in the scratch folder given by --out.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path
from typing import Sequence

import numpy as np
from PIL import Image, ImageDraw

Box = tuple[int, int, int, int]  # x, y, w, h in native video pixels


def parse_box(text: str) -> Box:
    """Parse ``x,y,w,h``."""
    parts = [int(p) for p in text.split(",")]
    if len(parts) != 4 or parts[2] <= 0 or parts[3] <= 0:
        raise ValueError(f"bad box {text!r}, want x,y,w,h")
    return parts[0], parts[1], parts[2], parts[3]


def decode_crop(
    video: Path, box: Box, fps: float, start: float = 0.0, end: float | None = None
) -> np.ndarray:
    """Return ``(n, h, w, 3)`` uint8 frames of ``box`` sampled at ``fps``."""
    x, y, w, h = box
    cmd = ["ffmpeg", "-v", "error", "-ss", str(start)]
    if end is not None:
        cmd += ["-t", str(end - start)]
    cmd += [
        "-i",
        str(video),
        "-an",
        "-vf",
        f"fps={fps},crop={w}:{h}:{x}:{y}",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgb24",
        "-",
    ]
    raw = subprocess.run(cmd, capture_output=True, check=True).stdout
    n = len(raw) // (w * h * 3)
    return np.frombuffer(raw, np.uint8)[: n * w * h * 3].reshape(n, h, w, 3)


def frame_change(frames: np.ndarray) -> np.ndarray:
    """Mean absolute grey difference between consecutive frames (length n, first is 0)."""
    grey = frames.astype(np.float32).mean(axis=3)
    d = np.zeros(len(frames), np.float32)
    if len(frames) > 1:
        d[1:] = np.abs(np.diff(grey, axis=0)).mean(axis=(1, 2))
    return d


def settled_indices(change: np.ndarray, thr: float = 2.0, hold: int = 2) -> list[int]:
    """Frame indices that end a change: ``change > thr`` earlier, then ``hold`` quiet frames.

    The first frame is always included.
    """
    out = [0]
    moving = False
    quiet = 0
    for i in range(1, len(change)):
        if change[i] > thr:
            moving = True
            quiet = 0
        else:
            quiet += 1
            if moving and quiet >= hold:
                out.append(i)
                moving = False
    return out


def make_sheet(
    frames: Sequence[np.ndarray], labels: Sequence[str], cols: int = 3, scale: float = 1.0
) -> Image.Image:
    """Tile frames with a text label above each."""
    if not frames:
        raise ValueError("no frames")
    h, w = frames[0].shape[:2]
    tw, th = int(w * scale), int(h * scale)
    rows = (len(frames) + cols - 1) // cols
    sheet = Image.new("RGB", (cols * tw, rows * (th + 14)), (0, 0, 0))
    draw = ImageDraw.Draw(sheet)
    for i, (fr, lab) in enumerate(zip(frames, labels)):
        cx, cy = (i % cols) * tw, (i // cols) * (th + 14)
        img = Image.fromarray(fr)
        if scale != 1.0:
            img = img.resize((tw, th), Image.LANCZOS)
        sheet.paste(img, (cx, cy + 14))
        draw.text((cx + 2, cy + 1), lab, fill=(255, 255, 0))
    return sheet


def main(argv: Sequence[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("video", type=Path)
    ap.add_argument("--box", required=True, help="x,y,w,h native pixels")
    ap.add_argument("--fps", type=float, default=4.0)
    ap.add_argument("--start", type=float, default=0.0)
    ap.add_argument("--end", type=float)
    ap.add_argument("--thr", type=float, default=2.0)
    ap.add_argument("--hold", type=int, default=2)
    ap.add_argument("--cols", type=int, default=3)
    ap.add_argument("--scale", type=float, default=1.0)
    ap.add_argument("--max-tiles", type=int, default=24)
    ap.add_argument("--every", action="store_true", help="tile every sampled frame")
    ap.add_argument("--out", type=Path, required=True, help="sheet PNG path (scratch)")
    args = ap.parse_args(argv)
    frames = decode_crop(args.video, parse_box(args.box), args.fps, args.start, args.end)
    idx = (
        list(range(len(frames)))
        if args.every
        else settled_indices(frame_change(frames), args.thr, args.hold)
    )
    print(f"{len(frames)} frames, {len(idx)} settled instants", file=sys.stderr)
    idx = idx[: args.max_tiles]
    labels = [f"{args.start + i / args.fps:7.2f}s" for i in idx]
    make_sheet([frames[i] for i in idx], labels, args.cols, args.scale).save(args.out)
    print(" ".join(labels))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
