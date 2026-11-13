#!/usr/bin/env python3
"""Phase 6b B7 proof: pack Blender renders into committed PNGs, hash and measure them.

Pipeline (Blender is a development dependency only; CI never renders):

    python tools/panel_assets.py render      # headless Blender -> assets/panel/proof/_raw
    python tools/panel_refbake.py --ref <reference.png>   # reference-derived sprites (D17)
    python tools/panel_assets.py pack        # stitch filmstrips, manifest, sizes
    python tools/panel_assets.py preview     # assets/panel/proof/preview_2x.png
    python tools/panel_assets.py verify      # check committed PNGs against the manifest (CI-safe)

The same constants are imported by `assets/blender/build_proof.py` so the scene and the
packer cannot drift apart.
"""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PROOF_DIR = ROOT / "assets" / "panel" / "proof"
RAW_DIR = PROOF_DIR / "_raw"  # git-ignored scratch frames
MANIFEST = PROOF_DIR / "manifest.json"
SCENE_SCRIPT = ROOT / "assets" / "blender" / "build_proof.py"
THEME_SAMPLES = ROOT / "assets" / "panel" / "theme_samples.json"  # measured colours, numbers only

# EMPIRICAL: 1 Blender unit = 64 px at 1x. The reference render has 129 px per unit (knob base
# diameter 103.4 px = 0.8 u), so our 2x sprites (128 px per unit) sit at the reference scale.
PX_PER_UNIT = 64
SCALES = (1, 2)
KNOB_FRAMES = 64
KNOB_GRID = 8  # 8 x 8 grid of frames per filmstrip
KNOB_FRAME_PX = 96  # 1x frame edge; 2x doubles it
# EMPIRICAL: rotary scale on the reference runs 0 at lower-left to 10 at lower-right, a 270
# degree sweep; frame 0 = -135 deg, frame 63 = +135 deg (clockwise positive).
KNOB_SWEEP_DEG = 270.0

# EMPIRICAL: the reference is a perspective render looking straight down. The Master Level top
# face is shifted 23 px off its base at 848 px off-centre, which for a 0.4 u tall cap puts the
# camera about 13.5 u (1750 reference px) above the panel.
CAM_HEIGHT_U = 13.5
# EMPIRICAL: camera offset from each sprite class (units, +x right, +y up the panel), read from
# where the matching control sits relative to the reference image centre (980, 592): Master
# Level knob, 16' drawbar cap, layer fader A cap, ORGAN button. Plates are flat (no lean).
LEAN = {
    "knob": (6.57, -1.5),
    # EMPIRICAL: the DRIVE pointer sits 5.5 px below the base centre on the reference, so the
    # optical axis is above that knob (the reference is cropped off-centre vertically).
    "knob_master": (6.57, 1.0),
    "knob_drive": (6.57, 1.4),
    "drawbar": (1.3, 2.97),
    "layer": (5.29, -2.73),
    "button": (6.59, 0.70),
    "plate": (0.0, 0.0),
    # EMPIRICAL: per cap, from where it sits on the reference; the optical axis is about
    # 290 px above the image centre (DRIVE knob lean), tuned against the cap silhouettes.
    "drawbar_cap_black": (2.56, 5.2),
    "drawbar_cap_white": (0.2, 5.5),
    "layer_cap_grey": (5.29, -0.45),
}

# Blender-rendered single sprites: name -> (1x width, 1x height) in px.
SPRITES = {
    "knob_ring": (96, 96),  # white arc, separate layer (Master Level style)
    "knob_shadow_master": (96, 96),  # shadow of the reference-derived Master Level cap
    "knob_shadow_drive": (96, 96),
    "plate_slate_slot": (128, 256),  # drawbar slot on slate, not a tile
    # caps: 3D geometry textured by projecting the reference from the cap's camera (D17)
    "drawbar_cap_black": (96, 160),
    "drawbar_shadow_black": (96, 160),
    "drawbar_cap_white": (96, 160),
    "drawbar_shadow_white": (96, 160),
    "layer_cap_grey": (96, 96),
    "layer_shadow_grey": (96, 96),
}
# Knobs whose cap body is cut from the reference (D17); each gets a Blender pointer filmstrip.
REF_KNOBS = ("master", "drive")
FILMSTRIPS = ("knob_filmstrip", *(f"pointer_{k}" for k in REF_KNOBS))
TILES = ("plate_red", "plate_slate", "plate_silver")  # reference-derived, seamless
REF_CROPS = ROOT / "assets" / "panel" / "ref_crops.json"


def ref_sprites(crops: dict | None = None) -> list[str]:
    """Names of the sprites `tools/panel_refbake.py` cuts from the reference (D17)."""
    crops = crops or json.loads(REF_CROPS.read_text(encoding="utf-8"))
    names = [f"button_{b}" for b in crops["buttons"]]
    names += [f"tri_{t}" for t in crops["triangles"]]
    names += sorted({f"tri_lit_{t['dir']}" for t in crops["triangles"].values()})
    names += [f"knob_body_{k}" for k in crops["knobs"]] + ["knob_scale", *TILES]
    return names


DRAWBAR_COLORS = ("black", "white")
# Cap frames (1x px); the caps are cut from the reference (D17), see ref_crops.json "caps".
CAP_FRAMES = {
    "drawbar_cap_black": (96, 160),
    "drawbar_cap_white": (96, 160),
    "layer_cap_grey": (96, 96),
}

# ASSUMED: control counts until B2 (ns4-3n0d) verifies them against the manual; chosen from a
# rough read of the reference photo (3 sections x ~12 knobs, 9+9 drawbars, 3 layer faders).
PANEL_ESTIMATE = {"knobs": 45, "faders": 30, "buttons": 150}
# ASSUMED: distinct knob styles on the whole panel (each needs its own filmstrip): the two
# reference knobs (Master Level, DRIVE) plus the generic style and three more sizes elsewhere.
KNOB_STYLES = 6
# ASSUMED: button down and lit states are not rendered yet; each button sprite will need two
# more of the same size.
BUTTON_EXTRA_STATES = 2


def knob_angle_deg(frame: int) -> float:
    """Pointer angle of filmstrip frame `frame`, clockwise degrees from 12 o'clock."""
    if not 0 <= frame < KNOB_FRAMES:
        raise ValueError(f"frame {frame} outside 0..{KNOB_FRAMES - 1}")
    return -KNOB_SWEEP_DEG / 2 + KNOB_SWEEP_DEG * frame / (KNOB_FRAMES - 1)


def frame_path(raw: Path, scale: int, frame: int, strip: str = "knob") -> Path:
    return raw / f"{strip}_{scale}x" / f"f{frame:03d}.png"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def png_size(path: Path) -> tuple[int, int]:
    """Read width and height from a PNG IHDR without an image library."""
    head = path.read_bytes()[:24]
    if head[:8] != b"\x89PNG\r\n\x1a\n" or head[12:16] != b"IHDR":
        raise ValueError(f"{path} is not a PNG")
    return int.from_bytes(head[16:20], "big"), int.from_bytes(head[20:24], "big")


def save_png(img, out: Path) -> None:
    out.parent.mkdir(parents=True, exist_ok=True)
    img.save(out, format="PNG", optimize=True, compress_level=9)


def stitch_filmstrip(raw: Path, scale: int, out: Path, strip: str = "knob") -> None:
    """Pack 64 frames into an 8x8 grid PNG (row-major, frame 0 top-left)."""
    from PIL import Image

    edge = KNOB_FRAME_PX * scale
    sheet = Image.new("RGBA", (edge * KNOB_GRID, edge * KNOB_GRID), (0, 0, 0, 0))
    for i in range(KNOB_FRAMES):
        with Image.open(frame_path(raw, scale, i, strip)) as im:
            if im.size != (edge, edge):
                raise ValueError(f"frame {i} is {im.size}, expected {(edge, edge)}")
            sheet.paste(im.convert("RGBA"), ((i % KNOB_GRID) * edge, (i // KNOB_GRID) * edge))
    save_png(sheet, out)


def make_seamless(img):
    """Return an opaque copy of `img` that wraps without seams: the image is cross-faded with a
    copy rolled by half its size (weight 1 at the centre, 0 at the borders), and the blend is
    renormalised so the grain keeps its contrast where both copies mix."""
    import numpy as np
    from PIL import Image

    a = np.asarray(img.convert("RGB"), dtype=np.float64)
    h, w = a.shape[:2]
    b = np.roll(a, (h // 2, w // 2), axis=(0, 1))
    wy = np.sin(np.pi * (np.arange(h) + 0.5) / h) ** 2
    wx = np.sin(np.pi * (np.arange(w) + 0.5) / w) ** 2
    wa = np.outer(wy, wx)[..., None]
    mean = a.mean(axis=(0, 1))
    mix = ((a - mean) * wa + (b - mean) * (1 - wa)) / np.sqrt(wa**2 + (1 - wa) ** 2)
    out = np.clip(np.rint(mean + mix), 0, 255).astype(np.uint8)
    return Image.fromarray(out, "RGB").convert("RGBA")


def asset_names() -> list[str]:
    names = []
    for s in SCALES:
        names += [f"{n}_{s}x.png" for n in (*FILMSTRIPS, *SPRITES, *ref_sprites())]
    return names


def build_manifest(proof: Path = PROOF_DIR) -> dict:
    files = {}
    for name in asset_names():
        path = proof / name
        w, h = png_size(path)
        files[name] = {
            "sha256": sha256(path),
            "bytes": path.stat().st_size,
            "width": w,
            "height": h,
        }
    return {"version": 2, "files": files}


def verify_manifest(manifest: dict, proof: Path = PROOF_DIR) -> list[str]:
    """Return a list of problems; empty means every committed PNG matches its hash."""
    problems = []
    for name, meta in manifest["files"].items():
        path = proof / name
        if not path.exists():
            problems.append(f"missing {name}")
        elif sha256(path) != meta["sha256"]:
            problems.append(f"hash mismatch {name}")
    return problems


def size_report(manifest: dict, estimate: dict | None = None) -> dict:
    """Per-scale bytes plus a full-panel estimate (PNG on disk and RGBA in GPU memory)."""
    estimate = estimate or PANEL_ESTIMATE
    f = manifest["files"]
    out = {"estimate_counts": dict(estimate), "per_scale": {}}

    def px(name: str) -> int:
        return f[name]["width"] * f[name]["height"]

    buttons = [n for n in ref_sprites() if n.startswith("button_")]
    for s in SCALES:
        knob = f"knob_filmstrip_{s}x.png"
        singles = [*SPRITES, *ref_sprites()]
        shared = [f"{n}_{s}x.png" for n in (*FILMSTRIPS, *singles)]
        extra = (KNOB_STYLES - len(FILMSTRIPS)) * px(knob) + BUTTON_EXTRA_STATES * sum(
            px(f"{b}_{s}x.png") for b in buttons
        )
        out["per_scale"][f"{s}x"] = {
            "knob_styles": KNOB_STYLES,
            "button_styles": len(buttons),
            "knob_filmstrip_png_bytes": f[knob]["bytes"],
            "sprite_png_bytes": {n: f[f"{n}_{s}x.png"]["bytes"] for n in (*FILMSTRIPS, *singles)},
            "shared_sprite_set_png_bytes": sum(f[n]["bytes"] for n in shared),
            "shared_sprite_set_rgba_bytes": 4 * sum(px(n) for n in shared),
            "full_panel_png_bytes": sum(f[n]["bytes"] for n in shared)
            + (KNOB_STYLES - len(FILMSTRIPS)) * f[knob]["bytes"],
            "full_panel_rgba_bytes": 4 * (sum(px(n) for n in shared) + extra),
        }
    return out


def find_blender() -> str:
    env = os.environ.get("BLENDER")
    if env:
        return env
    guess = Path("C:/Program Files/Blender Foundation/Blender 5.2/blender.exe")
    return str(guess) if guess.exists() else "blender"


def render(raw: Path = RAW_DIR) -> None:
    cmd = [
        find_blender(),
        "--background",
        "--factory-startup",
        "--python-exit-code",
        "1",  # Blender exits 0 on a script error unless told otherwise
        "--python",
        str(SCENE_SCRIPT),
        "--",
        "--out",
        str(raw),
    ]
    subprocess.run(cmd, check=True)


def retexture_slot(render, tile, target, slot_lum: float = 40.0):
    """Keep the Blender slot (dark pixels and their 1 px edge) and replace the rendered slate
    around it with the reference slate tile (D17), shifted to the measured slate colour of the
    drawbar area, so the plate behind the caps has the reference grain."""
    import numpy as np
    from PIL import Image, ImageFilter

    from_render = np.asarray(render.convert("RGB"), dtype=np.float64)
    lum = from_render.mean(axis=2)
    slot = Image.fromarray(((lum < slot_lum) * 255).astype(np.uint8)).filter(
        ImageFilter.MaxFilter(3)
    )
    keep = np.asarray(slot.filter(ImageFilter.GaussianBlur(0.8)), dtype=np.float64)[..., None] / 255
    t = np.asarray(tile.convert("RGB"), dtype=np.float64)
    reps = (-(-from_render.shape[0] // t.shape[0]), -(-from_render.shape[1] // t.shape[1]), 1)
    t = np.tile(t, reps)[: from_render.shape[0], : from_render.shape[1]]
    t = t - t.mean(axis=(0, 1)) + np.asarray(target, dtype=np.float64)
    out = from_render * keep + t * (1 - keep)
    return Image.fromarray(np.clip(np.rint(out), 0, 255).astype(np.uint8), "RGB").convert("RGBA")


def pack(raw: Path = RAW_DIR, proof: Path = PROOF_DIR) -> dict:
    """Stitch the Blender filmstrips and copy the Blender sprites (the slot plate gets the
    reference slate grain); the reference-derived sprites are already written by
    panel_refbake.py, which must run first. Then rebuild the manifest."""
    from PIL import Image

    for s in SCALES:
        for strip in FILMSTRIPS:
            name = "knob" if strip == "knob_filmstrip" else strip
            stitch_filmstrip(raw, s, proof / f"{strip}_{s}x.png", name)
        for name in SPRITES:
            src = raw / f"{name}_{s}x.png"
            (proof / src.name).write_bytes(src.read_bytes())
        slot = proof / f"plate_slate_slot_{s}x.png"
        with Image.open(slot) as im, Image.open(proof / f"plate_slate_{s}x.png") as tile:
            target = json.loads(REF_CROPS.read_text(encoding="utf-8"))["slot_plate_target"]
            save_png(retexture_slot(im, tile, target), slot)
    manifest = build_manifest(proof)
    (proof / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return manifest


def main(argv: list[str]) -> int:
    cmd = argv[0] if argv else ""
    if cmd == "render":
        render()
    elif cmd == "pack":
        print(json.dumps(size_report(pack()), indent=2))
    elif cmd == "verify":
        problems = verify_manifest(json.loads(MANIFEST.read_text(encoding="utf-8")))
        print("\n".join(problems) or "all hashes match")
        return 1 if problems else 0
    elif cmd == "preview":
        import panel_preview

        out = PROOF_DIR / "preview_2x.png"
        panel_preview.compose_preview(PROOF_DIR).convert("RGB").save(out, optimize=True)
        print(out)
    elif cmd == "report":
        print(json.dumps(size_report(json.loads(MANIFEST.read_text(encoding="utf-8"))), indent=2))
    else:
        print(__doc__)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
