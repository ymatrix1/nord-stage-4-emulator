"""Validate the panel layout against high-resolution real-panel crops.

    cargo run -p nord-gui --example layout_dump_gui > target/layout_check/layout_gui.json
    python tools/layout_overlay.py organ [--source gui|layout] [--anchors "px,py,mx,my;..."]
                                         [--shots] [--out DIR] [--crops DIR]

For one section it
  1. maps crop pixels <-> panel mm with an affine fit per axis from anchor points (default: the
     crop's section outline corners = the section rectangle of the dump; `--anchors` overrides),
  2. draws every control rectangle of the section with its id over the crop
     (`<out>/<crop>_overlay.png`), and, with `--shots`, a side-by-side with our own screenshot
     (`python tools/gui_shots.py --scale 4 --sections <section>`),
  3. reads `crates/nord-panel/data/photo_anchors_<section>.csv` (id, photo_x, photo_y: the
     real control's centre in crop px, read by eye from the crop) and prints per control the
     offset in mm between our rectangle centre and the real centre, flagging |offset| > 2 mm.

The crops live in `.reference/crops` (git-ignored, never committed); output goes to
`target/layout_check/` (git-ignored). Needs Pillow.
"""

import argparse
import csv
import json
import math
import os
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
DEFAULT_OUT = os.path.join(ROOT, "target", "layout_check")


def find_crops_dir(start=ROOT):
    """`$NS4_CROPS`, else the nearest `.reference/crops` up the tree (a worktree sits inside
    the main checkout, which owns the git-ignored reference dir)."""
    env = os.environ.get("NS4_CROPS")
    if env:
        return env
    d = start
    while True:
        cand = os.path.join(d, ".reference", "crops")
        if os.path.isdir(cand):
            return cand
        parent = os.path.dirname(d)
        if parent == d:
            return os.path.join(start, ".reference", "crops")
        d = parent


DEFAULT_CROPS = find_crops_dir()
DATA_DIR = os.path.join(ROOT, "crates", "nord-panel", "data")
THRESHOLD_MM = 2.0

# crop name -> (file, section whose rectangle the outline maps to, outline in crop px
# (x0, y0, x1, y1) or None = whole image, sections drawn on it).
CROPS = {
    "organ": (
        "NS4-Panel-organ.max-2000x2000.format-png.png",
        "organ",
        (250.0, 12.0, 1948.0, 1168.0),
        ("master", "organ"),
    ),
    "program": (
        "NS4_Panel-crop-left.jpg",
        "program",
        (2663.0, 0.0, 3754.0, 1146.0),
        ("program",),
    ),
    "piano": ("NS4-Panel-piano.max-2000x2000.format-png.png", "piano", None, ("piano",)),
    "synth": ("NS4-Panel-synth.max-2000x2000.format-png.png", "synth", None, ("synth",)),
    "effects": (
        "NS4-Panel-effects.max-2000x2000.format-png.png",
        "effects",
        None,
        ("effects",),
    ),
}
SECTION_CROP = {
    "master": "organ",
    "organ": "organ",
    "program": "program",
    "piano": "piano",
    "synth": "synth",
    "effects": "effects",
}


def fit_axis(pairs):
    """Least-squares `mm = a * px + b` from `(px, mm)` pairs; returns `(a, b)`."""
    if len(pairs) < 2:
        raise ValueError("need at least two anchors per axis")
    n = len(pairs)
    sx = sum(p for p, _ in pairs)
    sy = sum(m for _, m in pairs)
    sxx = sum(p * p for p, _ in pairs)
    sxy = sum(p * m for p, m in pairs)
    den = n * sxx - sx * sx
    if abs(den) < 1e-9:
        raise ValueError("anchors share the same coordinate on one axis")
    a = (n * sxy - sx * sy) / den
    return a, (sy - a * sx) / n


class Mapping:
    """Crop px <-> panel mm, one scale and offset per axis."""

    def __init__(self, anchors):
        self.ax, self.bx = fit_axis([(a[0], a[2]) for a in anchors])
        self.ay, self.by = fit_axis([(a[1], a[3]) for a in anchors])

    def to_mm(self, px, py):
        return self.ax * px + self.bx, self.ay * py + self.by

    def to_px(self, mx, my):
        return (mx - self.bx) / self.ax, (my - self.by) / self.ay

    def anisotropy(self):
        """Vertical over horizontal mm per crop px (1.0 = same scale on both axes)."""
        return self.ay / self.ax


def parse_anchors(text):
    """`"px,py,mmx,mmy;px,py,mmx,mmy"` -> list of 4-tuples."""
    out = []
    for item in text.split(";"):
        item = item.strip()
        if item:
            vals = [float(v) for v in item.split(",")]
            if len(vals) != 4:
                raise ValueError(f"anchor needs 4 numbers: {item!r}")
            out.append(tuple(vals))
    return out


def outline_anchors(outline, rect):
    """Corner anchors mapping the crop outline `(x0, y0, x1, y1)` onto a mm rect dict."""
    x0, y0, x1, y1 = outline
    return [
        (x0, y0, rect["x"], rect["y"]),
        (x1, y1, rect["x"] + rect["w"], rect["y"] + rect["h"]),
    ]


def load_photo_anchors(path):
    """`{id: (photo_x, photo_y)}` from the committed CSV."""
    with open(path, newline="", encoding="utf-8") as f:
        return {r["id"]: (float(r["photo_x"]), float(r["photo_y"])) for r in csv.DictReader(f)}


def pick_rect(control, source):
    """Rectangle of a control: the drawn one (`gui`) when asked and present."""
    if source == "gui" and "gui" in control:
        return control["gui"]
    return control["rect"]


def centre(r):
    return r["x"] + r["w"] / 2, r["y"] + r["h"] / 2


def compute_offsets(controls, photo, mapping, source="gui"):
    """Per control with a photo anchor: our centre minus the real centre, in mm.

    Returns rows `(id, dx, dy, dist)`; positive dx means ours sits right of the real control.
    """
    by_id = {c["id"]: c for c in controls}
    rows = []
    for cid, (px, py) in photo.items():
        c = by_id.get(cid)
        if c is None:
            continue
        ox, oy = centre(pick_rect(c, source))
        rx, ry = mapping.to_mm(px, py)
        rows.append((cid, ox - rx, oy - ry, math.hypot(ox - rx, oy - ry)))
    return rows


def format_report(rows, threshold=THRESHOLD_MM):
    """Text table sorted by distance; flagged rows are marked `!`."""
    lines = [f"{'id':34s} {'dx mm':>7s} {'dy mm':>7s} {'|d| mm':>7s}"]
    for cid, dx, dy, d in sorted(rows, key=lambda r: -r[3]):
        flag = " !" if d > threshold else ""
        lines.append(f"{cid:34s} {dx:7.2f} {dy:7.2f} {d:7.2f}{flag}")
    if rows:
        flagged = sum(1 for r in rows if r[3] > threshold)
        mean = sum(r[3] for r in rows) / len(rows)
        lines.append(
            f"{len(rows)} controls, {flagged} over {threshold} mm, "
            f"mean {mean:.2f} mm, max {max(r[3] for r in rows):.2f} mm"
        )
    return "\n".join(lines)


def draw_overlay(img, controls, photo, mapping, source="gui", frames=()):
    """Semi-transparent control rectangles with ids, real centres as crosses; returns RGB."""
    from PIL import Image, ImageDraw

    base = img.convert("RGBA")
    layer = Image.new("RGBA", base.size, (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    for fr in frames:
        r = fr["rect"]
        x0, y0 = mapping.to_px(r["x"], r["y"])
        x1, y1 = mapping.to_px(r["x"] + r["w"], r["y"] + r["h"])
        d.rectangle([x0, y0, x1, y1], outline=(255, 255, 0, 150), width=2)
    for c in controls:
        r = pick_rect(c, source)
        x0, y0 = mapping.to_px(r["x"], r["y"])
        x1, y1 = mapping.to_px(r["x"] + r["w"], r["y"] + r["h"])
        est = c.get("estimated")
        fill = (255, 140, 0, 70) if est else (0, 180, 255, 70)
        edge = (255, 140, 0, 230) if est else (0, 120, 255, 230)
        d.rectangle([x0, y0, x1, y1], fill=fill, outline=edge, width=2)
        d.text((x0 + 2, y0 + 1), c["id"].split(".", 1)[-1], fill=(255, 255, 255, 255))
        if c["id"] in photo:
            px, py = photo[c["id"]]
            cx, cy = mapping.to_px(*centre(r))
            d.line([cx, cy, px, py], fill=(255, 60, 60, 220), width=2)
            d.line([px - 8, py, px + 8, py], fill=(60, 255, 60, 255), width=2)
            d.line([px, py - 8, px, py + 8], fill=(60, 255, 60, 255), width=2)
    return Image.alpha_composite(base, layer).convert("RGB")


def side_by_side(left, right):
    """Two images next to each other, scaled to the same height."""
    from PIL import Image

    h = min(left.height, right.height)
    a = left.resize((round(left.width * h / left.height), h))
    b = right.resize((round(right.width * h / right.height), h))
    out = Image.new("RGB", (a.width + b.width + 10, h), (20, 20, 20))
    out.paste(a, (0, 0))
    out.paste(b, (a.width + 10, 0))
    return out


def load_layout(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def build_mapping(layout, crop, anchors_text):
    _, sec, outline, _ = CROPS[crop]
    if anchors_text:
        return Mapping(parse_anchors(anchors_text))
    if outline is None:
        raise SystemExit(f"crop {crop!r} has no built-in outline: pass --anchors")
    return Mapping(outline_anchors(outline, layout["sections"][sec]))


def main(argv=None):
    from PIL import Image

    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("section", choices=sorted(SECTION_CROP))
    ap.add_argument("--layout", default=os.path.join(DEFAULT_OUT, "layout_gui.json"))
    ap.add_argument("--source", choices=["gui", "layout"], default="gui")
    ap.add_argument("--anchors", default="", help='"px,py,mm_x,mm_y;..." (>= 2)')
    ap.add_argument("--crops", default=DEFAULT_CROPS)
    ap.add_argument("--out", default=DEFAULT_OUT)
    ap.add_argument("--csv", default="", help="photo anchor CSV (default: crates/nord-panel/data)")
    ap.add_argument(
        "--shots", action="store_true", help="also write the side-by-side with our shot"
    )
    args = ap.parse_args(argv)

    if not os.path.exists(args.layout):
        sys.exit(f"missing {args.layout}: run cargo run -p nord-gui --example layout_dump_gui")
    layout = load_layout(args.layout)
    crop = SECTION_CROP[args.section]
    fname, _, _, secs = CROPS[crop]
    mapping = build_mapping(layout, crop, args.anchors)
    csv_path = args.csv or os.path.join(DATA_DIR, f"photo_anchors_{args.section}.csv")
    photo = load_photo_anchors(csv_path) if os.path.exists(csv_path) else {}
    controls = [c for c in layout["controls"] if c["section"] == args.section]
    frames = [f for f in layout["frames"] if f["section"] == args.section]
    os.makedirs(args.out, exist_ok=True)

    img = Image.open(os.path.join(args.crops, fname))
    over = draw_overlay(img, controls, photo, mapping, args.source, frames)
    png = os.path.join(args.out, f"{args.section}_overlay.png")
    over.save(png)
    print(png)
    if args.shots:
        subprocess.run(
            [
                sys.executable,
                os.path.join(ROOT, "tools", "gui_shots.py"),
                "--scale",
                "4",
                "--sections",
                args.section,
                "--no-build",
                "--out",
                os.path.join(args.out, "shots"),
            ],
            check=True,
        )
        shot = Image.open(os.path.join(args.out, "shots", f"{args.section}.png"))
        both = os.path.join(args.out, f"{args.section}_side_by_side.png")
        side_by_side(over, shot).save(both)
        print(both)
    print(f"scale mm/px: x {mapping.ax:.5f} y {mapping.ay:.5f} (y/x = {mapping.anisotropy():.3f})")
    print(format_report(compute_offsets(controls, photo, mapping, args.source)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
