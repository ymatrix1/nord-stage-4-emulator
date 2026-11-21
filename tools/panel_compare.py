#!/usr/bin/env python3
"""Look-development helper: put a committed sprite composite beside a private reference crop.

The reference images are the owner's private photos (D16): they are read locally, scaled to the
same feature size as our render and written to `.reference/` (git-ignored). Nothing derived from
them is committed.

    python tools/panel_compare.py --knob-ref <knob.png> --caps-ref <organ.webp>
"""

from __future__ import annotations

import sys
from pathlib import Path

import panel_assets as P
from PIL import Image

OUT = P.ROOT / ".reference" / "panel-compare-v2.png"
# Feature sizes in pixels at 2x, used to match scales: our knob cap diameter and fader cap width
# (units x 64 px x 2), and the same features measured on the reference crops.
OUR_KNOB_PX = 102
OUR_CAP_PX = 79
REF_KNOB_PX = 79  # Master Level cap width on the 134 px wide close-up
REF_CAP_PX = 115  # drawbar cap width on the Organ section render
# Crop boxes (x0, y0, x1, y1) of single caps on the Organ section render (1961 x 1185).
REF_CAP_BOXES = {"black": (590, 840, 720, 1120), "white": (1330, 760, 1460, 1010)}


def scaled(img: Image.Image, factor: float) -> Image.Image:
    size = (max(1, round(img.width * factor)), max(1, round(img.height * factor)))
    return img.resize(size, Image.LANCZOS)


def side_by_side(ours: Image.Image, ref: Image.Image, gap: int = 8) -> Image.Image:
    """Ours left, reference right, top aligned, on a dark background."""
    h = max(ours.height, ref.height)
    sheet = Image.new("RGBA", (ours.width + gap + ref.width, h), (24, 24, 26, 255))
    sheet.paste(ours, (0, 0))
    sheet.paste(ref.convert("RGBA"), (ours.width + gap, 0))
    return sheet


def build(knob_ref: Path, caps_ref: Path, proof: Path = P.PROOF_DIR) -> Image.Image:
    rows = []
    ref = Image.open(knob_ref).convert("RGB")
    rows.append(side_by_side(P.composed_knob(proof, 12), scaled(ref, OUR_KNOB_PX / REF_KNOB_PX)))
    caps = Image.open(caps_ref).convert("RGB")
    for color, box in REF_CAP_BOXES.items():
        crop = scaled(caps.crop(box), OUR_CAP_PX / REF_CAP_PX)
        rows.append(side_by_side(P.composed_fader(proof, color), crop))
    width = max(r.width for r in rows)
    sheet = Image.new("RGBA", (width, sum(r.height + 8 for r in rows)), (24, 24, 26, 255))
    y = 0
    for r in rows:
        sheet.paste(r, (0, y))
        y += r.height + 8
    return sheet


def main(argv: list[str]) -> int:
    opts = dict(zip(argv[::2], argv[1::2]))
    if "--knob-ref" not in opts or "--caps-ref" not in opts:
        print(__doc__)
        return 2
    out = Path(opts.get("--out", OUT))
    out.parent.mkdir(parents=True, exist_ok=True)
    build(Path(opts["--knob-ref"]), Path(opts["--caps-ref"])).convert("RGB").save(out)
    print(out)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
