#!/usr/bin/env python3
"""Cut drawbar and layer-fader caps, and their shadows, from the reference render (D17).

Used by `tools/panel_refbake.py`. The cap is separated from the slate plate by colour: the caps
are neutral or warm (blue <= red), the slate and the silver label strips are bluish, the LED
ladders red. Slot strips are dropped by width. The shadow layer is the darkening of the slate
around the cap, as black with alpha, so it composes on any slate plate.
"""

from __future__ import annotations

import math

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

# EMPIRICAL: cap half-widths in 2x px (= reference px) around the cap centre; the nose sticks
# out further on the left. Columns outside are never cap (keeps the LED ladders out).
HALF_LEFT, HALF_RIGHT = 64, 56
# EMPIRICAL: rows of rounded top corners on the caps (2x px).
CORNER_ROWS = 12
# EMPIRICAL: luminance of clean slate around the drawbars, the zero of the shadow layer.
SLATE_LUM = 62.0
# EMPIRICAL: offset (x, y px) of the cap's shadow under the top-left light, read on the
# reference drawbars.
SHADOW_SHIFT = (5, 10)


def ref_window(ref: Image.Image, centre, size, ref_px_per_unit: float = 129.0) -> np.ndarray:
    """Reference pixels around `centre` covering a 2x sprite of `size` px, as float RGB."""
    k = ref_px_per_unit / 128.0
    w, h = size[0] * k, size[1] * k
    x0, y0 = math.floor(centre[0] - w / 2), math.floor(centre[1] - h / 2)
    sub = ref.convert("RGB").crop((x0, y0, math.ceil(x0 + w + 1), math.ceil(y0 + h + 1)))
    fx, fy = centre[0] - w / 2 - x0, centre[1] - h / 2 - y0  # crop() pads outside the image
    out = sub.resize(size, Image.LANCZOS, box=(fx, fy, fx + w, fy + h))
    return np.asarray(out, dtype=np.float64)


def colour_mask(a: np.ndarray, kind: str) -> np.ndarray:
    """Everything that is not background: slate and silver labels are bluish (b > r), LED
    ladders red, slot interiors near black. Shaded cap faces stay in even when dark."""
    r, g, b = a[..., 0], a[..., 1], a[..., 2]
    # slate is about +21; shaded cap faces stay below +12; in deep shadow the slate keeps a
    # small blue lead while the cap feet are neutral (about 5, 4, 4)
    bluish = (b - r >= 14) | ((b - r >= 4) & (r < 40))
    red = r - g >= 60  # LED cores only; the red glow on a cap edge stays cap
    return ~bluish & ~red & (a.max(axis=2) > 3)


def largest_from_centre(mask: np.ndarray) -> np.ndarray:
    """The connected part of `mask` that contains (or is nearest to) the frame centre."""
    h, w = mask.shape
    img = Image.fromarray((mask * 100).astype(np.uint8)).copy()  # fromarray is read-only
    ys, xs = np.nonzero(mask)
    if not len(ys):
        return mask
    i = np.argmin((xs - w / 2) ** 2 + (ys - h / 2) ** 2)
    ImageDraw.floodfill(img, (int(xs[i]), int(ys[i])), 255)
    return np.asarray(img) == 255


def fill_holes(mask: np.ndarray) -> np.ndarray:
    h, w = mask.shape
    img = Image.fromarray(np.pad(mask, 1).astype(np.uint8) * 100).copy()
    ImageDraw.floodfill(img, (0, 0), 255)  # outside reached from the corner
    return ~(np.asarray(img)[1:-1, 1:-1] == 255)


def vertical_open(m: np.ndarray, k: int) -> np.ndarray:
    """Morphological opening with a k x 1 vertical line: removes horizontal strokes thinner
    than k px (label frame edges touching the cap) and keeps the tall cap."""
    pad = np.pad(m, ((k // 2, k // 2), (0, 0)))
    er = np.ones_like(m)
    for i in range(k):
        er &= pad[i : i + m.shape[0]]
    pad = np.pad(er, ((k // 2, k // 2), (0, 0)))
    out = np.zeros_like(m)
    for i in range(k):
        out |= pad[i : i + m.shape[0]]
    return out


def clean_edge_glow(a: np.ndarray, m: np.ndarray, radius: float = 4.0) -> np.ndarray:
    """Recolour two kinds of cap pixels from the clean cap interior next to them: red-tinted
    ones (LED glow reflected on the cap; the LED is a separate dynamic layer) and the outer
    2 px rim, which still mixes in the lighter slate and would read as a glowing outline."""
    inner = (
        np.asarray(Image.fromarray(m.astype(np.uint8) * 255).filter(ImageFilter.MinFilter(5))) > 0
    )
    tinted = (m & (a[..., 0] - a[..., 1] > 12)) | (m & ~inner)
    good = (inner & ~tinted).astype(np.float64)
    out = a.copy()
    if not tinted.any() or not good.any():
        return out
    blur = [
        np.asarray(
            Image.fromarray(np.clip(a[..., c] * good, 0, 255).astype(np.uint8)).filter(
                ImageFilter.GaussianBlur(radius)
            ),
            dtype=np.float64,
        )
        for c in range(3)
    ]
    den = (
        np.asarray(
            Image.fromarray((good * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(radius)),
            dtype=np.float64,
        )
        / 255
    )
    spread = np.stack(blur, axis=-1) / np.maximum(den, 1e-3)[..., None]
    out[tinted] = spread[tinted]
    return out


def _spread(a: np.ndarray, src: np.ndarray, radius: float) -> np.ndarray:
    """Average colour of the `src` pixels around every pixel (normalised blur)."""
    w = src.astype(np.float64)
    num = np.stack(
        [
            np.asarray(
                Image.fromarray(np.clip(a[..., c] * w, 0, 255).astype(np.uint8)).filter(
                    ImageFilter.GaussianBlur(radius)
                ),
                dtype=np.float64,
            )
            for c in range(3)
        ],
        axis=-1,
    )
    den = (
        np.asarray(
            Image.fromarray((w * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(radius)),
            dtype=np.float64,
        )
        / 255
    )
    return num / np.maximum(den, 1e-3)[..., None]


def matte(a: np.ndarray, m: np.ndarray, band: int = 2) -> tuple[np.ndarray, np.ndarray]:
    """(alpha 0..1, colour) with a smooth edge: in a band around the mask outline, alpha is the
    position of each pixel between the local cap colour and the local plate colour, and the
    colour is the cap colour, so the sprite composes on any plate without a halo."""
    img = Image.fromarray(m.astype(np.uint8) * 255)
    k = 2 * band + 1
    core = np.asarray(img.filter(ImageFilter.MinFilter(k))) > 0
    outer = ~(np.asarray(img.filter(ImageFilter.MaxFilter(k))) > 0)
    cap_c, bg_c = _spread(a, core, 3.0), _spread(a, outer, 3.0)
    d = cap_c - bg_c
    t = ((a - bg_c) * d).sum(axis=-1) / np.maximum((d * d).sum(axis=-1), 1e-6)
    strong = (d * d).sum(axis=-1) > 20**2  # cap and plate differ enough to unmix
    edge = ~core & ~outer
    alpha = np.where(core, 1.0, 0.0)
    alpha = np.where(edge, np.where(strong, np.clip(t, 0, 1), m.astype(float)), alpha)
    # outside the mask only neutral pixels can be cap edge; bluish ones are slate or a silver
    # label next to the cap, which would otherwise unmix as light bits at the corners
    bluish = a[..., 2] - a[..., 0] >= 10
    alpha = np.where(~m & bluish, 0.0, alpha)
    rgb = np.where(edge[..., None], cap_c, a)
    return alpha, rgb


def cap_mask(a: np.ndarray, kind: str) -> np.ndarray:
    h, w = a.shape[:2]
    m = colour_mask(a, kind)
    cols = np.arange(w)
    m &= ((cols >= w / 2 - HALF_LEFT) & (cols <= w / 2 + HALF_RIGHT))[None, :]
    img = Image.fromarray(m.astype(np.uint8) * 255).filter(ImageFilter.MedianFilter(3))
    img = img.filter(ImageFilter.MinFilter(7)).filter(ImageFilter.MaxFilter(7))  # drop thin lines
    m = np.asarray(img) > 0
    rows = m.sum(axis=1)
    m &= (rows >= 0.45 * rows.max())[:, None]  # slot strips above and below are narrow
    m = vertical_open(fill_holes(largest_from_centre(m)), 11)  # label-frame edges are flat
    m = fill_holes(largest_from_centre(m))
    ys = np.nonzero(m.any(axis=1))[0]
    if len(ys):  # top rows never reach past the body (label frames touch the cap there)
        y0, y1 = ys[0], ys[-1]
        top = slice(y0, y0 + 2 * (y1 - y0) // 5)
        ref_rows = m[y0 + (y1 - y0) // 5 : y0 + 2 * (y1 - y0) // 5]  # below corners and nubs
        band = [np.nonzero(r)[0] for r in ref_rows if r.any()]
        left = np.median([r.min() for r in band])
        right = np.median([r.max() for r in band])
        inside = (cols >= left) & (cols <= right)
        m[top, :] &= inside[None, :]
        # the top block has straight sides: rows below the rounded top corners span the
        # median width exactly (fills LED-ladder notches, trims label and glow nubs)
        straight = slice(y0 + CORNER_ROWS, y0 + 2 * (y1 - y0) // 5)
        m[straight, :] = inside[None, :]
        if kind != "black":  # dark label-frame pixels on the top rows of a light cap
            head = slice(y0, y0 + CORNER_ROWS)
            m[head] &= a[head].max(axis=2) > 90
    return m


def cap_layers(ref: Image.Image, centre, size, kind: str) -> tuple[Image.Image, Image.Image]:
    """(cap sprite with soft alpha, shadow layer) for a 2x frame of `size` around `centre`."""
    a = ref_window(ref, centre, size)
    m = cap_mask(a, kind)
    rgb = clean_edge_glow(a, m)
    alpha, rgb = matte(rgb, m)
    soft = (alpha * 255).astype(np.uint8)
    cap = Image.fromarray(np.dstack([np.clip(rgb, 0, 255), soft]).astype(np.uint8), "RGBA")
    lum = a @ np.array([0.2126, 0.7152, 0.0722])
    smooth = np.asarray(
        Image.fromarray(np.clip(lum, 0, 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(3)),
        dtype=np.float64,
    )  # no grain, no label strokes
    # the light is top-left, so the shadow falls below and to the right of the cap
    shifted = np.roll(np.roll(m, SHADOW_SHIFT[1], axis=0), SHADOW_SHIFT[0], axis=1)
    grown = Image.fromarray((shifted | m).astype(np.uint8) * 255).filter(ImageFilter.MaxFilter(9))
    # the reference pixels hugging the cap are its anti-aliased edge, not shadow
    hug = np.asarray(Image.fromarray(m.astype(np.uint8) * 255).filter(ImageFilter.MaxFilter(9))) > 0
    rows = np.arange(m.shape[0])[:, None]
    top = np.nonzero(m.any(axis=1))[0].min() if m.any() else 0
    cols = np.arange(m.shape[1])[None, :]
    w = m.shape[1]
    inside = (rows > top + 12) & (cols >= w / 2 - HALF_LEFT - 8) & (cols <= w / 2 + HALF_RIGHT + 12)
    bottom = np.nonzero(m.any(axis=1))[0].max() if m.any() else 0
    hug &= rows < bottom - 6  # below the cap the dark contact shadow is real: keep it
    region = (np.asarray(grown) > 0) & inside & ~hug & ~m
    dark = np.clip((SLATE_LUM - smooth) / SLATE_LUM * 1.2, 0, 0.85) * region
    blur = Image.fromarray((dark * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(2.5))
    shadow = np.zeros((*m.shape, 4), np.uint8)
    shadow[..., 3] = np.asarray(blur)
    return cap, Image.fromarray(shadow, "RGBA")
