#!/usr/bin/env python3
"""Look-development helper: put composed proof sprites beside the same features of the owner's
private reference render, at the same feature scale, with a simple difference metric.

The reference is the owner's private image (D16): it is read locally, cropped and scaled in
memory, and the sheet is written to `.reference/` (git-ignored). Nothing derived from it is
committed; only the measured numbers in `assets/panel/theme_samples.json` are.

    python tools/panel_compare.py --ref <NS4-Panel-organ.png> [--proof <dir>] [--out <png>]
"""

from __future__ import annotations

import json
import math
import sys
from pathlib import Path

import numpy as np
import panel_assets as P
import panel_preview as V
import panel_refbake as R
from PIL import Image

OUT = P.ROOT / ".reference" / "panel-compare-v6.png"
# EMPIRICAL: reference px per Blender unit (knob base 103.4 px = 0.8 u). Our 2x sprites have
# 128 px per unit, so reference crops are scaled by 128 / 129.
REF_PX_PER_U = 129.0
# EMPIRICAL: centres of the plate patches on the 1961 x 1185 reference (base centre of
# each control, from silhouette extents and a small correlation search).
CAP_FEATURES = {
    "plate_red": ((200.0, 300.0), ("tile", "plate_red")),
    "plate_slate": ((640.0, 400.0), ("tile", "plate_slate")),
    "plate_silver": ((1400.0, 85.0), ("tile", "plate_silver")),
}
TILE_VIEW = (96, 48)  # 2x px of plate shown in a row (small: the reference bands are thin)


def features(crops: dict | None = None) -> dict:
    """Every compared feature: name -> (reference centre, how to compose ours). Knobs, buttons
    and triangle selectors come from `assets/panel/ref_crops.json`, one row per instance."""
    crops = crops or json.loads(P.REF_CROPS.read_text(encoding="utf-8"))
    out = {}
    for name, k in crops["knobs"].items():
        frame = round((k["pointer_deg"] + 135) / 270 * (P.KNOB_FRAMES - 1))
        out[f"knob_{name}"] = (tuple(k["centre"]), ("refknob", name, frame))
    for name, (x0, y0, x1, y1) in crops["buttons"].items():
        out[f"button_{name}"] = (((x0 + x1) / 2, (y0 + y1) / 2), ("sprite", f"button_{name}"))
    for name, t in crops["triangles"].items():
        x0, y0, x1, y1 = t["box"]
        out[f"tri_{name}"] = (((x0 + x1) / 2, (y0 + y1) / 2), ("tri", name, t))
    for name, c in crops.get("caps", {}).items():
        out[name] = (tuple(c["centre"]), ("cap", name))
    out.update(CAP_FEATURES)
    return out


def ours(proof: Path, spec: tuple, scale: int = 2) -> Image.Image:
    kind = spec[0]
    if kind == "refknob":
        return V.composed_ref_knob(proof, spec[1], spec[2], scale)
    if kind == "cap":
        return V.composed_cap(proof, spec[1], scale)
    if kind == "sprite":
        return Image.open(proof / f"{spec[1]}_{scale}x.png").convert("RGBA")
    if kind == "tri":  # unlit matrix with the lit overlay where the reference LED is on
        t = spec[2]
        base = Image.open(proof / f"tri_{spec[1]}_{scale}x.png").convert("RGBA")
        lit = Image.open(proof / f"tri_lit_{t['dir']}_{scale}x.png").convert("RGBA")
        cell = R.cell_box(t["box"], t["grid"], *t["lit"])
        k = P.PX_PER_UNIT * scale / REF_PX_PER_U
        base.alpha_composite(
            lit, (round((cell[0] - t["box"][0]) * k), round((cell[1] - t["box"][1]) * k))
        )
        return base
    tile = Image.open(proof / f"{spec[1]}_{scale}x.png").convert("RGBA")
    return V.centre_crop(tile, *TILE_VIEW)


def ref_crop(ref: Image.Image, centre: tuple[float, float], size: tuple[int, int], scale=2):
    """Crop the reference around `centre` covering the same panel area as a sprite of `size`
    px at `scale`, resized to `size`."""
    k = REF_PX_PER_U / (P.PX_PER_UNIT * scale)
    w, h = size[0] * k, size[1] * k
    x0, y0 = math.floor(centre[0] - w / 2), math.floor(centre[1] - h / 2)
    sub = ref.convert("RGBA").crop((x0, y0, math.ceil(x0 + w + 1), math.ceil(y0 + h + 1)))
    fx, fy = centre[0] - w / 2 - x0, centre[1] - h / 2 - y0  # crop() pads outside the image
    return sub.resize(size, Image.LANCZOS, box=(fx, fy, fx + w, fy + h))


def diff_metric(a: Image.Image, b: Image.Image) -> dict:
    """Mean colour of each image and mean absolute difference per channel (0..255)."""
    x = np.asarray(a.convert("RGB"), dtype=np.float64)
    y = np.asarray(b.convert("RGB"), dtype=np.float64)
    return {
        "ours_mean": [round(v, 1) for v in x.mean(axis=(0, 1))],
        "ref_mean": [round(v, 1) for v in y.mean(axis=(0, 1))],
        "mad": round(float(np.abs(x - y).mean()), 2),
    }


def build(ref_path: Path, proof: Path = P.PROOF_DIR, zoom: int = 2):
    """Return (sheet, metrics). Each row: ours | reference, both upscaled by `zoom`."""
    ref = Image.open(ref_path).convert("RGB")
    rows, metrics = [], {}
    for name, (centre, spec) in features().items():
        a = ours(proof, spec)
        b = ref_crop(ref, centre, a.size)
        metrics[name] = diff_metric(a, b)
        big = [im.resize((im.width * zoom, im.height * zoom), Image.LANCZOS) for im in (a, b)]
        rows.append(big)
    return V.stack(rows, 8), metrics


def main(argv: list[str]) -> int:
    opts = dict(zip(argv[::2], argv[1::2]))
    if "--ref" not in opts:
        print(__doc__)
        return 2
    out = Path(opts.get("--out", OUT))
    sheet, metrics = build(Path(opts["--ref"]), Path(opts.get("--proof", P.PROOF_DIR)))
    out.parent.mkdir(parents=True, exist_ok=True)
    sheet.convert("RGB").save(out)
    for name, m in metrics.items():
        print(f"{name:14s} mad {m['mad']:6.2f} ours {m['ours_mean']} ref {m['ref_mean']}")
    print(out)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
