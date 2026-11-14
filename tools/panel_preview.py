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


def strip_frame(proof: Path, frame: int, scale: int = 2, strip: str = "knob_filmstrip"):
    e = P.KNOB_FRAME_PX * scale
    sheet = _open(proof, strip, scale)
    col, row = frame % P.KNOB_GRID, frame // P.KNOB_GRID
    return sheet.crop((col * e, row * e, (col + 1) * e, (row + 1) * e))


def knob_frame(proof: Path, frame: int, scale: int = 2) -> Image.Image:
    return strip_frame(proof, frame, scale)


def composed_knob(proof: Path, frame: int, scale: int = 2, layer: str = "knob_ring"):
    """Generic knob: red plate, the ring or scale layer, then the Blender frame (with shadow)."""
    e = P.KNOB_FRAME_PX * scale
    bg = tiled(_open(proof, "plate_red", scale), e, e)
    bg.alpha_composite(_open(proof, layer, scale))
    bg.alpha_composite(knob_frame(proof, frame, scale))
    return bg


REF_KNOB_LAYER = {"master": "knob_ring", "drive": "knob_scale"}


def composed_ref_knob(proof: Path, name: str, frame: int, scale: int = 2) -> Image.Image:
    """Reference knob (D28): red plate, ring or scale, Blender shadow, the cap body cut from
    the reference, then the Blender pointer frame on top."""
    e = P.KNOB_FRAME_PX * scale
    bg = tiled(_open(proof, "plate_red", scale), e, e)
    for n in (REF_KNOB_LAYER[name], f"knob_shadow_{name}", f"knob_body_{name}"):
        bg.alpha_composite(_open(proof, n, scale))
    bg.alpha_composite(strip_frame(proof, frame, scale, f"pointer_{name}"))
    return bg


def composed_cap(proof: Path, name: str, scale: int = 2) -> Image.Image:
    """Slate plate with slot, then the cap's shadow layer, then the cap sprite.
    `name` is `drawbar_cap_black`, `drawbar_cap_white` or `layer_cap_grey`."""
    w, h = (v * scale for v in P.CAP_FRAMES[name])
    bg = centre_crop(_open(proof, "plate_slate_slot", scale), w, h)
    for n in (name.replace("_cap_", "_shadow_"), name):
        bg.alpha_composite(_open(proof, n, scale))
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
    """Proof sheet: reference knobs at three angles each and the generic knob; drawbar and
    layer caps; every button; triangle selectors; the seamless tiles (1x, 2 x 2)."""
    knobs = [composed_ref_knob(proof, n, f, scale) for n in P.REF_KNOBS for f in (0, 32, 63)]
    knobs.append(composed_knob(proof, 32, scale))
    caps = [composed_cap(proof, f"drawbar_cap_{c}", scale) for c in P.DRAWBAR_COLORS]
    caps.append(composed_cap(proof, "layer_cap_grey", scale))
    names = P.ref_sprites()
    buttons = [_open(proof, n, scale) for n in names if n.startswith("button_")]
    rows = [knobs, caps]
    for i in range(0, len(buttons), 5):
        rows.append(buttons[i : i + 5])
    tris = [_open(proof, n, scale) for n in names if n.startswith("tri_") and "_lit_" not in n]
    tris += [_open(proof, n, scale) for n in names if n.startswith("tri_lit_")]
    tiles = []
    for t in P.TILES:  # 1x tiles repeated 2 x 2: enough to show the wrap, keeps the sheet small
        tile = _open(proof, t, 1)
        tiles.append(tiled(tile, tile.width * 2, tile.height * 2))
    return stack(rows + [tris, tiles])
