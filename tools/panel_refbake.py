#!/usr/bin/env python3
"""Cut reference-derived panel sprites from the owner's Organ section render (decision D28).

    python tools/panel_refbake.py --ref <NS4-Panel-organ.png>   # writes assets/panel/proof/*.png

D28 narrows D16 for the panel proof: buttons, triangle selectors, knob bodies, the DRIVE scale
and the plate textures are taken from the reference pixels, because the goal is the closest
possible match. Crop boxes and centres are data in `assets/panel/ref_crops.json`. The reference
file itself stays in the git-ignored `.reference/`; only the derived sprites are committed, with
hashes in the manifest (`tools/panel_assets.py pack` runs after this).
"""

from __future__ import annotations

import json
import math
import sys
from pathlib import Path

import numpy as np
import panel_assets as P
import panel_refcaps as RC
from PIL import Image, ImageDraw, ImageFilter

CROPS = P.ROOT / "assets" / "panel" / "ref_crops.json"
TILE_2X = 256  # tile edge at 2x, 1x is half
KNOB_2X = P.KNOB_FRAME_PX * 2


def cap_shadow_name(cap: str) -> str:
    """drawbar_cap_black -> drawbar_shadow_black: every cap has its own shadow layer."""
    return cap.replace("_cap_", "_shadow_")


def load_crops(path: Path = CROPS) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def at_scale(img: Image.Image, scale: int, ref_px_per_unit: float = 129.0) -> Image.Image:
    """Resample a reference-pixel image to our 1x / 2x grid (64 / 128 px per unit)."""
    k = P.PX_PER_UNIT * scale / ref_px_per_unit
    size = (max(1, round(img.width * k)), max(1, round(img.height * k)))
    return img.resize(size, Image.LANCZOS)


def crop(ref: Image.Image, box) -> Image.Image:
    return ref.convert("RGBA").crop(tuple(int(v) for v in box))


def centre_box(centre, edge: float) -> tuple[float, float, float, float]:
    cx, cy = centre
    return (cx - edge / 2, cy - edge / 2, cx + edge / 2, cy + edge / 2)


# ---------------------------------------------------------------- plate textures


def clean_mask(a: np.ndarray, target, tol: float, struct_tol: float = 22.0) -> np.ndarray:
    """Pixels near `target` with no large-scale structure (text, edges, LEDs)."""
    lum = a.mean(axis=2)
    img = Image.fromarray(np.clip(lum, 0, 255).astype(np.uint8))
    blur = np.asarray(img.filter(ImageFilter.GaussianBlur(4)), dtype=np.float64)
    near = np.abs(a - np.asarray(target, dtype=np.float64)).max(axis=2) < tol
    return near & (np.abs(lum - blur) < struct_tol)


def clean_windows(mask: np.ndarray, patch: int, step: int = 3) -> list[tuple[int, int]]:
    """Top-left corners of patch x patch windows that are fully clean (integral image)."""
    ii = np.pad(mask.astype(np.int32).cumsum(0).cumsum(1), ((1, 0), (1, 0)))
    full = patch * patch
    out = []
    for y in range(0, mask.shape[0] - patch, step):
        for x in range(0, mask.shape[1] - patch, step):
            s = ii[y + patch, x + patch] - ii[y, x + patch] - ii[y + patch, x] + ii[y, x]
            if s == full:
                out.append((x, y))
    return out


def quilt(
    a: np.ndarray,
    windows,
    size: int,
    patch: int,
    seed: int = 0,
    mean=None,
    overlap: int = 4,
    tries: int = 12,
) -> np.ndarray:
    """Image quilting on a torus: cells of (patch - overlap) px, each filled with the clean
    reference patch (of `tries` random ones) that best agrees with its already placed
    neighbours in the overlap, blended only across that narrow overlap. Indices wrap, so the
    tile is seamless by construction and the grain keeps its full contrast. Each patch is
    shifted to `mean` (default: the median patch colour) to drop lighting blotches."""
    if not windows:
        raise ValueError("no clean window for the tile")
    step = patch - overlap
    if size % step:
        raise ValueError(f"tile {size} is not a multiple of the cell {step}")
    rng = np.random.default_rng(seed)
    if mean is None:
        mean = np.median([a[y : y + patch, x : x + patch].mean(axis=(0, 1)) for x, y in windows], 0)
    # best-fit choice favours flat patches; keep only patches with typical grain contrast
    grain = np.array([a[y : y + patch, x : x + patch].std() for x, y in windows])
    lo, hi = np.percentile(grain, [35, 90])
    windows = [w for w, g in zip(windows, grain) if lo <= g <= hi] or windows
    ramp = np.ones(patch)
    ramp[:overlap] = (np.arange(overlap) + 0.5) / overlap
    ramp[-overlap:] = ramp[:overlap][::-1]
    w2 = np.outer(ramp, ramp)[..., None]
    acc = np.zeros((size, size, 3))
    wsum = np.zeros((size, size, 1))
    idx = np.arange(patch)
    for y in range(0, size, step):
        for x in range(0, size, step):
            rows, cols = (y + idx) % size, (x + idx) % size
            have = wsum[np.ix_(rows, cols)][..., 0] > 0
            cur = acc[np.ix_(rows, cols)] / np.maximum(wsum[np.ix_(rows, cols)], 1e-9)
            best, best_err = None, None
            for k in rng.integers(len(windows), size=tries):
                sx, sy = windows[k]
                p = a[sy : sy + patch, sx : sx + patch]
                p = p - p.mean(axis=(0, 1)) + mean  # keep the grain, drop lighting blotches
                err = ((p - cur)[have] ** 2).sum() if have.any() else 0.0
                if best_err is None or err < best_err:
                    best, best_err = p, err
            acc[np.ix_(rows, cols)] += best * w2
            wsum[np.ix_(rows, cols)] += w2
    return acc / np.maximum(wsum, 1e-9)


def plate_tile(ref: Image.Image, target, tol: float, seed: int = 0) -> Image.Image:
    """Seamless tile of the plate grain, quilted from clean reference patches (no blending of
    the whole tile, so the grain keeps the reference contrast)."""
    a = np.asarray(ref.convert("RGB"), dtype=np.float64)
    patch = 20
    windows = clean_windows(clean_mask(a, target, tol), patch)
    tile = quilt(a, windows, TILE_2X, patch, seed, np.asarray(target, dtype=np.float64))
    return Image.fromarray(np.clip(np.rint(tile), 0, 255).astype(np.uint8), "RGB").convert("RGBA")


# ---------------------------------------------------------------- triangle selectors


def cell_box(box, grid, col: int, row: int) -> tuple[int, int, int, int]:
    x0, y0, x1, y1 = box
    cw, ch = (x1 - x0) / grid[0], (y1 - y0) / grid[1]
    return (
        round(x0 + col * cw),
        round(y0 + row * ch),
        round(x0 + (col + 1) * cw),
        round(y0 + (row + 1) * ch),
    )


def triangle_layers(ref: Image.Image, spec: dict) -> tuple[Image.Image, Image.Image]:
    """(unlit matrix base, lit overlay for one cell). The lit cell of the reference is replaced
    by an unlit cell of the same orientation; the overlay keeps only the red light."""
    box, grid = spec["box"], spec["grid"]
    base = crop(ref, box)
    lit = cell_box(box, grid, *spec["lit"])
    src = cell_box(box, grid, *spec["unlit_src"])
    w, h = min(lit[2] - lit[0], src[2] - src[0]), min(lit[3] - lit[1], src[3] - src[1])
    patch = crop(ref, (src[0], src[1], src[0] + w, src[1] + h))
    cell = np.asarray(crop(ref, (lit[0], lit[1], lit[0] + w, lit[1] + h)), dtype=np.float64)
    redness = cell[..., 0] - (cell[..., 1] + cell[..., 2]) / 2
    glow = Image.fromarray(((redness > 12) * 255).astype(np.uint8)).filter(ImageFilter.MaxFilter(3))
    base.paste(patch, (lit[0] - box[0], lit[1] - box[1]), glow)  # only the lit pixels change
    alpha = np.clip((redness - 25) / 120, 0, 1) * 255
    out = cell.copy()
    out[..., 3] = alpha
    return base, Image.fromarray(out.astype(np.uint8), "RGBA")


# ---------------------------------------------------------------- knobs


def convex_hull(points: np.ndarray) -> list[tuple[float, float]]:
    pts = sorted(set(map(tuple, points.tolist())))
    if len(pts) < 3:
        return pts

    def cross(o, a, b):
        return (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])

    lower, upper = [], []
    for p in pts:
        while len(lower) >= 2 and cross(lower[-2], lower[-1], p) <= 0:
            lower.pop()
        lower.append(p)
    for p in reversed(pts):
        while len(upper) >= 2 and cross(upper[-2], upper[-1], p) <= 0:
            upper.pop()
        upper.append(p)
    return lower[:-1] + upper[:-1]


def circle_hull(c0, r0, c1, r1, n: int = 180) -> list[tuple[float, float]]:
    """Convex hull of two circles: the knob seen in perspective is its base circle plus its
    top circle shifted by the lean, joined by the visible part of the wall."""
    pts = []
    for (cx, cy), r in ((c0, r0), (c1, r1)):
        pts += [
            (cx + r * math.cos(2 * math.pi * i / n), cy + r * math.sin(2 * math.pi * i / n))
            for i in range(n)
        ]
    return convex_hull(np.round(np.asarray(pts) * 8) / 8)


def knob_masks(a: np.ndarray, base_r: float, top_r: float, top_off):
    """(soft cap alpha 0..1, pointer mask) for a knob crop centred on the knob base; the top
    circle sits at `top_off` (px) from the base centre."""
    h, w = a.shape[:2]
    top = (w / 2 + top_off[0], h / 2 + top_off[1])
    hull = circle_hull((w / 2, h / 2), base_r, top, top_r)
    k = 4  # supersampled polygon for an anti-aliased edge
    big = Image.new("L", (w * k, h * k), 0)
    ImageDraw.Draw(big).polygon([(x * k, y * k) for x, y in hull], fill=255)
    soft = np.asarray(big.resize((w, h), Image.BOX), dtype=np.float64) / 255
    light = (a.min(axis=2) > 85) & ((a.max(axis=2) - a.min(axis=2)) < 40) & (soft > 0.5)
    grown = Image.fromarray((light * 255).astype(np.uint8)).filter(ImageFilter.MaxFilter(5))
    return soft, np.asarray(grown) > 0


def inpaint_rotate(a: np.ndarray, mask: np.ndarray, centre, inside: np.ndarray):
    """Fill masked pixels with pixels rotated about `centre` by one, two or three flutes; the
    fluted body is close to rotationally symmetric, so this hides the reference pointer.
    Sources must lie inside the cap (`inside`) and outside the mask."""
    out = a.copy()
    ys, xs = np.nonzero(mask)
    acc = np.zeros((len(ys), a.shape[2]))
    cnt = np.zeros(len(ys))
    cx, cy = centre
    for deg in (18, -18, 36, -36, 54, -54):  # average every valid rotated source: no seams
        t = math.radians(deg)
        sx = np.rint(cx + (xs - cx) * math.cos(t) - (ys - cy) * math.sin(t)).astype(int)
        sy = np.rint(cy + (xs - cx) * math.sin(t) + (ys - cy) * math.cos(t)).astype(int)
        sx, sy = np.clip(sx, 0, a.shape[1] - 1), np.clip(sy, 0, a.shape[0] - 1)
        ok = ~mask[sy, sx] & inside[sy, sx]
        acc[ok] += a[sy[ok], sx[ok]]
        cnt[ok] += 1
    got = cnt > 0
    out[ys[got], xs[got]] = acc[got] / cnt[got, None]
    if (~got).any():  # no clean source (pointer overhanging the rim): use the cap's mean colour
        out[ys[~got], xs[~got]] = a[inside].mean(axis=0)
    return out


def decontaminate(rgb: np.ndarray, alpha: np.ndarray, radius: float = 3.0) -> np.ndarray:
    """Replace the colour of semi-transparent edge pixels with the colour of the opaque cap
    next to them, so the plate the reference had behind the edge (red) does not show as a
    light fringe on other plates."""
    solid = (alpha > 0.98).astype(np.float64)
    num = np.stack(
        [
            np.asarray(
                Image.fromarray(np.clip(rgb[..., c] * solid, 0, 255).astype(np.uint8)).filter(
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
            Image.fromarray((solid * 255).astype(np.uint8)).filter(
                ImageFilter.GaussianBlur(radius)
            ),
            dtype=np.float64,
        )[..., None]
        / 255
    )
    spread = num / np.maximum(den, 1e-3)
    return np.where(solid[..., None] > 0, rgb, np.where(den > 0.01, spread, rgb))


def knob_body(ref: Image.Image, centre, top_off) -> Image.Image:
    """The reference knob cap without its pointer and without plate, ring or scale (alpha)."""
    edge = KNOB_2X * 129.0 / 128.0
    sub = ref.convert("RGB").resize((KNOB_2X, KNOB_2X), Image.LANCZOS, box=centre_box(centre, edge))
    a = np.asarray(sub, dtype=np.float64)
    base_r = 0.40 * 128  # EMPIRICAL: base radius 51.7 reference px = 0.4 u
    top_r = 0.88 * base_r  # EMPIRICAL: top face plus shoulder, about 45 reference px
    soft, pointer = knob_masks(a, base_r, top_r, top_off)
    cap_px = (soft > 0.95) & (a[..., 0] - a[..., 1] < 22) & (a.min(axis=2) < 110)
    # top-face pixels turn about the leaning top centre, wall pixels about the base centre
    c = KNOB_2X / 2
    top_c = (c + top_off[0], c + top_off[1])
    yy, xx = np.mgrid[:KNOB_2X, :KNOB_2X]
    on_top = np.hypot(xx - top_c[0], yy - top_c[1]) < 0.8 * top_r
    filled = inpaint_rotate(a, pointer & ~on_top, (c, c), cap_px & ~on_top)
    filled = inpaint_rotate(filled, pointer & on_top, top_c, cap_px & on_top)
    # trim the outer rim band where the fitted outline still shows plate, ring or numerals
    inner = Image.fromarray(((soft > 0.5) * 255).astype(np.uint8)).filter(ImageFilter.MinFilter(13))
    band = (soft > 0) & (np.asarray(inner) == 0)
    red = np.clip((filled[..., 0] - filled[..., 1] - 25) / 25, 0, 1)
    white = np.clip((filled.min(axis=2) - 110) / 40, 0, 1)
    keep = np.where(band & ~pointer, (1 - red) * (1 - white), 1.0)
    alpha = soft * keep
    rgba = np.dstack([decontaminate(filled, alpha), alpha * 255])
    return Image.fromarray(np.clip(rgba, 0, 255).astype(np.uint8), "RGBA")


def knob_scale(ref: Image.Image, centre, body_alpha: np.ndarray) -> Image.Image:
    """White numerals around the DRIVE knob as an alpha layer (no label, LED or knob)."""
    edge = KNOB_2X * 129.0 / 128.0
    sub = ref.convert("RGB").resize((KNOB_2X, KNOB_2X), Image.LANCZOS, box=centre_box(centre, edge))
    a = np.asarray(sub, dtype=np.float64)
    h, w = a.shape[:2]
    yy, xx = np.mgrid[:h, :w]
    r = np.hypot(xx - w / 2, yy - h / 2)
    ang = np.degrees(np.arctan2(xx - w / 2, yy - h / 2))  # 0 = straight down
    keep = (r > 0.29 * w) & (r < 0.47 * w) & (np.abs(ang) > 33) & (body_alpha < 0.05)
    # the reference DRIVE pointer points left and covers part of the "2": drop its tip
    keep &= ~((np.abs(yy - h / 2 - 0.02 * h) < 0.035 * h) & (xx < w / 2) & (r < 0.40 * w))
    keep &= ~((yy < 0.15 * h) & (xx > 0.72 * w))  # tail of the "ON" label, right of the "6"
    alpha = np.clip((a[..., 1] - 50) / 150, 0, 1) * keep
    rgb = np.zeros_like(a) + np.array([238.0, 236.0, 234.0])
    return Image.fromarray(np.dstack([rgb, alpha * 255]).astype(np.uint8), "RGBA")


# ---------------------------------------------------------------- driver


def bake(
    ref_path: Path, proof: Path = P.PROOF_DIR, crops: dict | None = None, raw: Path = P.RAW_DIR
) -> list[str]:
    """Write every reference-derived sprite at 2x (reference scale) and 1x; return names."""
    crops = crops or load_crops()
    ref = Image.open(ref_path).convert("RGB")
    k = crops["ref_px_per_unit"]
    two_x: dict[str, Image.Image] = {}
    for name, (x0, y0, x1, y1) in crops["buttons"].items():
        # the well boxes include the anti-aliased step to the plate: inset 1 px
        two_x[f"button_{name}"] = at_scale(crop(ref, (x0 + 1, y0 + 1, x1 - 1, y1 - 1)), 2, k)
    for name, spec in crops["triangles"].items():
        base, lit = triangle_layers(ref, spec)
        two_x[f"tri_{name}"] = at_scale(base, 2, k)
        two_x[f"tri_lit_{spec['dir']}"] = at_scale(lit, 2, k)
    for name, spec in crops["knobs"].items():
        src = crops["knobs"][spec.get("body_from", name)]  # same part, cleaner cut
        body = knob_body(ref, src["centre"], src["top_offset"])
        two_x[f"knob_body_{name}"] = body
        if name == "drive":
            alpha = np.asarray(body.getchannel("A"), dtype=np.float64) / 255
            two_x["knob_scale"] = knob_scale(ref, spec["centre"], alpha)
    for name, spec in crops.get("caps", {}).items():
        # caps are 3D renders (assets/blender/parts_cap.py) textured by projecting these
        # reference crops from the cap's camera; the texture is a render input, not a sprite
        size = tuple(2 * v for v in spec["size"])
        tex = RC.projection_texture(ref, spec["centre"], size, spec["kind"])
        P.save_png(tex, raw / f"proj_{name}.png")
    for seed, (name, spec) in enumerate(crops["tiles"].items()):
        two_x[name] = plate_tile(ref, spec["target"], spec["tol"], seed)
    for name, img in two_x.items():
        P.save_png(img, proof / f"{name}_2x.png")
        one = img.resize((max(1, img.width // 2), max(1, img.height // 2)), Image.LANCZOS)
        P.save_png(one, proof / f"{name}_1x.png")
    return sorted(two_x)


def main(argv: list[str]) -> int:
    opts = dict(zip(argv[::2], argv[1::2]))
    if "--ref" not in opts:
        print(__doc__)
        return 2
    names = bake(Path(opts["--ref"]), Path(opts.get("--proof", P.PROOF_DIR)))
    print("\n".join(names))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
