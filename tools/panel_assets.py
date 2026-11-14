#!/usr/bin/env python3
"""Phase 6b B7 proof: pack Blender renders into committed PNGs, hash and measure them.

Pipeline (Blender is a development dependency only; CI never renders):

    python tools/panel_assets.py render      # headless Blender -> assets/panel/proof/_raw
    python tools/panel_assets.py pack        # stitch filmstrips, write manifest + size report
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

# EMPIRICAL: 1 Blender unit = 64 px at 1x. Chosen so a 96 px frame covers 1.5 units; measured
# proportions (cap diameter vs fader-cap width about 1:1) come from the owner's reference photo,
# numbers only, no pixels (D16).
PX_PER_UNIT = 64
SCALES = (1, 2)
KNOB_FRAMES = 64
KNOB_GRID = 8  # 8 x 8 grid of frames per filmstrip
KNOB_FRAME_PX = 96  # 1x frame edge; 2x doubles it
# EMPIRICAL: rotary scale on the reference photo runs 0 at lower-left to 10 at lower-right,
# a 270 degree sweep; frame 0 = -135 deg, frame 63 = +135 deg (clockwise positive).
KNOB_SWEEP_DEG = 270.0
FADER_FRAME_PX = 96
FADER_COLORS = ("black", "white", "grey")  # drawbar black / white, layer-fader grey
# ASSUMED: the reference renders are a near top-down perspective view (the arc ring reads
# almost circular, ratio 0.93 = cos 22 deg). One orthographic camera tilted 24 deg for every
# sprite keeps layers aligned while still showing cap sides and fronts.
CAM_TILT_DEG = 30.0
PLATE_PX = {"plate_red": (128, 128), "plate_slate_slot": (128, 256)}

# ASSUMED: control counts until B2 (ns4-3n0d) verifies them against the manual; chosen from a
# rough read of the reference photo (3 sections x ~12 knobs, 9+9 drawbars, 3 layer faders).
PANEL_ESTIMATE = {"knobs": 45, "faders": 30, "buttons": 150}
# ASSUMED: distinct sprite styles (each needs its own filmstrip / cap sprite pair): knob styles =
# small, medium, large, endless encoder; fader styles = grey layer cap, black and white drawbar.
KNOB_STYLES = 4
# ASSUMED: one shared PNG per button state, 3 sprites (up, down, lit-overlay) at 64 px square.
BUTTON_SPRITE_BYTES_FACTOR = 3


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
    out.parent.mkdir(parents=True, exist_ok=True)
    sheet.save(out, format="PNG", optimize=True, compress_level=9)


def asset_names() -> list[str]:
    names = []
    for s in SCALES:
        names += [f"knob_filmstrip_{s}x.png", f"knob_ring_{s}x.png", f"fader_shadow_{s}x.png"]
        names += [f"fader_cap_{c}_{s}x.png" for c in FADER_COLORS]
        names += [f"{p}_{s}x.png" for p in PLATE_PX]
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
    return {"version": 1, "files": files}


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
    """Per-asset bytes plus a full-panel estimate per scale (PNG on disk and RGBA in GPU memory)."""
    estimate = estimate or PANEL_ESTIMATE
    f = manifest["files"]
    out = {"estimate_counts": dict(estimate), "per_scale": {}}

    def px(name: str) -> int:
        return f[name]["width"] * f[name]["height"]

    for s in SCALES:
        knob, ring = f"knob_filmstrip_{s}x.png", f"knob_ring_{s}x.png"
        shadow = f"fader_shadow_{s}x.png"
        caps = [f"fader_cap_{c}_{s}x.png" for c in FADER_COLORS]
        plates = [f"{p}_{s}x.png" for p in PLATE_PX]
        shared = [knob, ring, shadow, *caps, *plates]  # one knob style, all cap colours
        btn_px = (64 * s) ** 2 * BUTTON_SPRITE_BYTES_FACTOR  # buttons not rendered yet (ASSUMED)
        extra_knobs = KNOB_STYLES - 1  # further knob styles cost one more filmstrip + ring each
        out["per_scale"][f"{s}x"] = {
            "knob_filmstrip_png_bytes": f[knob]["bytes"],
            "knob_ring_png_bytes": f[ring]["bytes"],
            "fader_cap_plus_shadow_png_bytes": f[caps[0]]["bytes"] + f[shadow]["bytes"],
            "shared_sprite_set_png_bytes": sum(f[n]["bytes"] for n in shared),
            "shared_sprite_set_rgba_bytes": 4 * sum(px(n) for n in shared),
            "full_panel_png_bytes_excl_buttons": sum(f[n]["bytes"] for n in shared)
            + extra_knobs * (f[knob]["bytes"] + f[ring]["bytes"]),
            "full_panel_rgba_bytes_incl_buttons": 4
            * (sum(px(n) for n in shared) + extra_knobs * (px(knob) + px(ring)) + btn_px),
        }
    return out


PREVIEW_FRAMES = (0, 16, 32, 47, 63)


def _frame(strip, frame: int, edge: int):
    col, row = frame % KNOB_GRID, frame // KNOB_GRID
    return strip.crop((col * edge, row * edge, (col + 1) * edge, (row + 1) * edge))


def composed_knob(proof: Path, frame: int, scale: int = 2):
    """Plate, arc ring layer, then the knob frame (shadow baked in): the draw order in the app."""
    from PIL import Image

    e = KNOB_FRAME_PX * scale
    plate = Image.open(proof / f"plate_red_{scale}x.png").convert("RGBA")
    c = plate.size[0] // 2
    bg = plate.crop((c - e // 2, c - e // 2, c + e // 2, c + e // 2))
    bg.alpha_composite(Image.open(proof / f"knob_ring_{scale}x.png").convert("RGBA"))
    strip = Image.open(proof / f"knob_filmstrip_{scale}x.png").convert("RGBA")
    bg.alpha_composite(_frame(strip, frame, e))
    return bg


def composed_fader(proof: Path, color: str, scale: int = 2):
    """Slate plate with slot, then the shared shadow layer, then the cap sprite."""
    from PIL import Image

    e = FADER_FRAME_PX * scale
    slate = Image.open(proof / f"plate_slate_slot_{scale}x.png").convert("RGBA")
    cx, cy = slate.size[0] // 2, slate.size[1] // 2
    bg = slate.crop((cx - e // 2, cy - e // 2, cx + e // 2, cy + e // 2))
    for n in (f"fader_shadow_{scale}x", f"fader_cap_{color}_{scale}x"):
        bg.alpha_composite(Image.open(proof / f"{n}.png").convert("RGBA"))
    return bg


def compose_preview(proof: Path = PROOF_DIR, scale: int = 2):
    """Proof sheet: five knob frames (top) and the three fader caps (bottom), composed from the
    committed layers exactly as the app would draw them."""
    from PIL import Image

    e, gap = KNOB_FRAME_PX * scale, 8
    sheet = Image.new("RGBA", (e * 5 + gap * 4, e * 2 + gap), (24, 24, 26, 255))
    for i, frame in enumerate(PREVIEW_FRAMES):
        sheet.paste(composed_knob(proof, frame, scale), (i * (e + gap), 0))
    for i, color in enumerate(FADER_COLORS):
        sheet.paste(composed_fader(proof, color, scale), (i * (e + gap), e + gap))
    return sheet


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
    for s in SCALES:
        stitch_filmstrip(raw, s, proof / f"knob_filmstrip_{s}x.png")
        for name in (
            "knob_ring",
            "fader_shadow",
            *PLATE_PX,
            *(f"fader_cap_{c}" for c in FADER_COLORS),
        ):
            src = raw / f"{name}_{s}x.png"
            (proof / src.name).write_bytes(src.read_bytes())
    manifest = build_manifest(proof)
    (proof / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return manifest


def main(argv: list[str]) -> int:
    cmd = argv[0] if argv else ""
    if cmd == "render":
        render()
    elif cmd == "pack":
        report = size_report(pack())
        print(json.dumps(report, indent=2))
    elif cmd == "verify":
        problems = verify_manifest(json.loads(MANIFEST.read_text(encoding="utf-8")))
        print("\n".join(problems) or "all hashes match")
        return 1 if problems else 0
    elif cmd == "preview":
        compose_preview().convert("RGB").save(PROOF_DIR / "preview_2x.png", optimize=True)
    elif cmd == "report":
        print(json.dumps(size_report(json.loads(MANIFEST.read_text(encoding="utf-8"))), indent=2))
    else:
        print(__doc__)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
