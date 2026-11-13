#!/usr/bin/env python3
"""Phase 6b B7 proof: pack Blender renders into committed PNGs, hash and measure them.

Pipeline (Blender is a development dependency only; CI never renders):

    python tools/panel_assets.py render      # headless Blender -> assets/panel/proof/_raw
    python tools/panel_assets.py pack        # stitch filmstrips, seamless tiles, manifest, sizes
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
    "drawbar": (1.3, 2.97),
    "layer": (5.29, -2.73),
    "button": (6.59, 0.70),
    "plate": (0.0, 0.0),
}

# Single sprites (not the knob filmstrip): name -> (1x width, 1x height) in px.
SPRITES = {
    "knob_ring": (96, 96),  # white arc, separate layer (Master Level style)
    "knob_scale": (96, 96),  # numerals 0..10, separate layer (DRIVE style)
    "drawbar_cap_black": (96, 160),
    "drawbar_cap_white": (96, 160),
    "drawbar_shadow": (96, 160),  # shared by both drawbar colours
    "layer_cap_grey": (96, 96),
    "layer_shadow": (96, 96),
    "button_grey": (80, 48),  # up state, well included
    "button_black": (80, 48),
    "plate_red": (128, 128),  # seamless tiles
    "plate_slate": (128, 128),
    "plate_silver": (128, 128),
    "plate_slate_slot": (128, 256),  # drawbar slot on slate, not a tile
}
TILES = ("plate_red", "plate_slate", "plate_silver")
DRAWBAR_COLORS = ("black", "white")

# ASSUMED: control counts until B2 (ns4-3n0d) verifies them against the manual; chosen from a
# rough read of the reference photo (3 sections x ~12 knobs, 9+9 drawbars, 3 layer faders).
PANEL_ESTIMATE = {"knobs": 45, "faders": 30, "buttons": 150}
# ASSUMED: distinct knob styles (each needs its own filmstrip): small, medium, large, encoder.
KNOB_STYLES = 4
# ASSUMED: button down and lit states are not rendered yet; each colour will need two more
# sprites of the up-state size.
BUTTON_EXTRA_STATES = 2


def knob_angle_deg(frame: int) -> float:
    """Pointer angle of filmstrip frame `frame`, clockwise degrees from 12 o'clock."""
    if not 0 <= frame < KNOB_FRAMES:
        raise ValueError(f"frame {frame} outside 0..{KNOB_FRAMES - 1}")
    return -KNOB_SWEEP_DEG / 2 + KNOB_SWEEP_DEG * frame / (KNOB_FRAMES - 1)


def frame_path(raw: Path, scale: int, frame: int) -> Path:
    return raw / f"knob_{scale}x" / f"f{frame:03d}.png"


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


def stitch_filmstrip(raw: Path, scale: int, out: Path) -> None:
    """Pack the 64 knob frames into an 8x8 grid PNG (row-major, frame 0 top-left)."""
    from PIL import Image

    edge = KNOB_FRAME_PX * scale
    sheet = Image.new("RGBA", (edge * KNOB_GRID, edge * KNOB_GRID), (0, 0, 0, 0))
    for i in range(KNOB_FRAMES):
        with Image.open(frame_path(raw, scale, i)) as im:
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
        names.append(f"knob_filmstrip_{s}x.png")
        names += [f"{n}_{s}x.png" for n in SPRITES]
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

    for s in SCALES:
        knob = f"knob_filmstrip_{s}x.png"
        shared = [knob] + [f"{n}_{s}x.png" for n in SPRITES]
        extra = (KNOB_STYLES - 1) * px(knob) + BUTTON_EXTRA_STATES * sum(
            px(f"button_{c}_{s}x.png") for c in ("grey", "black")
        )
        out["per_scale"][f"{s}x"] = {
            "knob_filmstrip_png_bytes": f[knob]["bytes"],
            "sprite_png_bytes": {n: f[f"{n}_{s}x.png"]["bytes"] for n in SPRITES},
            "shared_sprite_set_png_bytes": sum(f[n]["bytes"] for n in shared),
            "shared_sprite_set_rgba_bytes": 4 * sum(px(n) for n in shared),
            "full_panel_png_bytes": sum(f[n]["bytes"] for n in shared)
            + (KNOB_STYLES - 1) * f[knob]["bytes"],
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


def pack(raw: Path = RAW_DIR, proof: Path = PROOF_DIR) -> dict:
    from PIL import Image

    for s in SCALES:
        stitch_filmstrip(raw, s, proof / f"knob_filmstrip_{s}x.png")
        for name in SPRITES:
            src = raw / f"{name}_{s}x.png"
            if name in TILES:
                with Image.open(src) as im:
                    save_png(make_seamless(im), proof / src.name)
            else:
                (proof / src.name).write_bytes(src.read_bytes())
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
