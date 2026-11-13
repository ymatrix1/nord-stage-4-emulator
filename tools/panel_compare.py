#!/usr/bin/env python3
"""Look-development helper: put composed proof sprites beside the same features of the owner's
private reference render, at the same feature scale, with a simple difference metric.

The reference is the owner's private image (D16): it is read locally, cropped and scaled in
memory, and the sheet is written to `.reference/` (git-ignored). Nothing derived from it is
committed; only the measured numbers in `assets/panel/theme_samples.json` are.

    python tools/panel_compare.py --ref <NS4-Panel-organ.png> [--proof <dir>] [--out <png>]
"""

from __future__ import annotations

import math
import sys
from pathlib import Path

import numpy as np
import panel_assets as P
import panel_preview as V
from PIL import Image

OUT = P.ROOT / ".reference" / "panel-compare-v3.png"
# EMPIRICAL: reference px per Blender unit (knob base 103.4 px = 0.8 u). Our 2x sprites have
# 128 px per unit, so reference crops are scaled by 128 / 129.
REF_PX_PER_U = 129.0
# EMPIRICAL: feature centres on the 1961 x 1185 reference (base centre of each control, read
# from circle fits and silhouette extents) and the knob frame whose pointer matches it.
FEATURES = {
    "master_knob": ((132.3, 173.2), ("knob", 59, "knob_ring")),
    "drive_knob": ((132.0, 485.0), ("knob", 10, "knob_scale")),
    "drawbar_black": ((656.0, 972.0), ("cap", "drawbar_cap_black")),
    "drawbar_white": ((954.0, 1006.0), ("cap", "drawbar_cap_white")),
    "layer_grey": ((311.0, 242.0), ("cap", "layer_cap_grey")),
    "button_grey": ((134.0, 684.0), ("button", "grey")),
    "button_black": ((131.0, 881.0), ("button", "black")),
    "plate_red": ((200.0, 300.0), ("tile", "plate_red")),
    "plate_slate": ((640.0, 400.0), ("tile", "plate_slate")),
    "plate_silver": ((1400.0, 85.0), ("tile", "plate_silver")),
}
TILE_VIEW = (96, 48)  # 2x px of plate shown in a row (small: the reference bands are thin)


def ours(proof: Path, spec: tuple, scale: int = 2) -> Image.Image:
    kind = spec[0]
    if kind == "knob":
        return V.composed_knob(proof, spec[1], scale, spec[2])
    if kind == "cap":
        return V.composed_cap(proof, spec[1], scale)
    if kind == "button":
        return V.composed_button(proof, spec[1], scale)
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
    for name, (centre, spec) in FEATURES.items():
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
