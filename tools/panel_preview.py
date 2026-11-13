#!/usr/bin/env python3
"""Compose B7 proof sprites the way the app draws them (plate, shadow, part, overlay layers).

Used by `python tools/panel_assets.py preview` and by `tools/panel_compare.py`. Everything here
reads only the committed sprites in `assets/panel/proof/`.
"""

from __future__ import annotations

from pathlib import Path

import panel_assets as P
from PIL import Image

PREVIEW_FRAMES = (0, 16, 32, 47, 63)
BG = (24, 24, 26, 255)


def _open(proof: Path, name: str, scale: int) -> Image.Image:
    return Image.open(proof / f"{name}_{scale}x.png").convert("RGBA")


def tiled(tile: Image.Image, w: int, h: int) -> Image.Image:
    """Repeat a seamless tile to cover w x h."""
    out = Image.new("RGBA", (w, h))
    for y in range(0, h, tile.height):
        for x in range(0, w, tile.width):
            out.paste(tile, (x, y))
    return out


def centre_crop(img: Image.Image, w: int, h: int) -> Image.Image:
    cx, cy = img.width // 2, img.height // 2
    return img.crop((cx - w // 2, cy - h // 2, cx - w // 2 + w, cy - h // 2 + h))


def knob_frame(proof: Path, frame: int, scale: int = 2) -> Image.Image:
    e = P.KNOB_FRAME_PX * scale
    strip = _open(proof, "knob_filmstrip", scale)
    col, row = frame % P.KNOB_GRID, frame // P.KNOB_GRID
    return strip.crop((col * e, row * e, (col + 1) * e, (row + 1) * e))


def composed_knob(proof: Path, frame: int, scale: int = 2, layer: str = "knob_ring"):
    """Red plate, the ring or scale layer, then the knob frame (shadow baked in)."""
    e = P.KNOB_FRAME_PX * scale
    bg = tiled(_open(proof, "plate_red", scale), e, e)
    bg.alpha_composite(_open(proof, layer, scale))
    bg.alpha_composite(knob_frame(proof, frame, scale))
    return bg


def composed_cap(proof: Path, name: str, scale: int = 2) -> Image.Image:
    """Slate plate with slot, then the shared shadow layer, then the cap sprite.
    `name` is `drawbar_cap_black`, `drawbar_cap_white` or `layer_cap_grey`."""
    w, h = (v * scale for v in P.SPRITES[name])
    bg = centre_crop(_open(proof, "plate_slate_slot", scale), w, h)
    shadow = "layer_shadow" if name.startswith("layer") else "drawbar_shadow"
    for n in (shadow, name):
        bg.alpha_composite(_open(proof, n, scale))
    return bg


def composed_button(proof: Path, color: str, scale: int = 2, plate: str = "plate_red"):
    w, h = (v * scale for v in P.SPRITES[f"button_{color}"])
    bg = tiled(_open(proof, plate, scale), w, h)
    bg.alpha_composite(_open(proof, f"button_{color}", scale))
    return bg


def stack(rows: list[list[Image.Image]], gap: int = 8) -> Image.Image:
    """Lay out rows of images left to right, rows top to bottom, on a dark sheet."""
    width = max(sum(i.width for i in r) + gap * (len(r) - 1) for r in rows)
    height = sum(max(i.height for i in r) for r in rows) + gap * (len(rows) - 1)
    sheet = Image.new("RGBA", (width, height), BG)
    y = 0
    for r in rows:
        x = 0
        for img in r:
            sheet.paste(img, (x, y))
            x += img.width + gap
        y += max(i.height for i in r) + gap
    return sheet


def compose_preview(proof: Path = P.PROOF_DIR, scale: int = 2) -> Image.Image:
    """Proof sheet: knob frames with ring and one with the DRIVE scale; drawbar, layer and
    button sprites; the three seamless tiles (1x) repeated 2 x 2 to show they wrap."""
    knobs = [composed_knob(proof, f, scale) for f in PREVIEW_FRAMES]
    knobs.append(composed_knob(proof, 9, scale, layer="knob_scale"))
    caps = [composed_cap(proof, f"drawbar_cap_{c}", scale) for c in P.DRAWBAR_COLORS]
    caps.append(composed_cap(proof, "layer_cap_grey", scale))
    btns = [composed_button(proof, c, scale) for c in ("grey", "black")]
    btns += [composed_button(proof, c, scale, "plate_slate") for c in ("grey", "black")]
    col = stack([[b] for b in btns], 4)
    tiles = []
    for t in P.TILES:  # 1x tiles repeated 2 x 2: enough to show the wrap, keeps the sheet small
        tile = _open(proof, t, 1)
        tiles.append(tiled(tile, tile.width * 2, tile.height * 2))
    return stack([knobs, caps + [col], tiles])
