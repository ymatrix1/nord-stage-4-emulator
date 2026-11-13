"""Headless Blender scene for the Phase 6b B7 proof: knob + ring + scale, drawbar and layer
caps, pill buttons, plate tiles.

Run (Blender 5.2, no GUI, no add-ons):

    blender --background --factory-startup --python-exit-code 1 \
        --python assets/blender/build_proof.py -- --out <dir> [--quick] [--only knob,button]

or `python tools/panel_assets.py render`. Writes raw frames to <dir>; `tools/panel_assets.py pack`
stitches, makes the tiles seamless and hashes them. Modules: proof_scene.py (camera, light),
proof_materials.py (materials), parts_*.py (geometry).

ORIGINAL ART (D16): all geometry is authored from primitives and all materials are procedural.
Nothing is imported, traced or sampled from any photo or model of the hardware. The owner's
reference images were used only to read proportions and colour numbers
(assets/panel/theme_samples.json).
"""

from __future__ import annotations

import math
import sys
from pathlib import Path

import bpy

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parents[1] / "tools")]
import panel_assets as P  # noqa: E402  (shared constants, no bpy)
import parts_button as B  # noqa: E402
import parts_cap as C  # noqa: E402
import parts_knob as K  # noqa: E402
import parts_plate as PL  # noqa: E402
import proof_materials as M  # noqa: E402
import proof_scene as S  # noqa: E402

ARGV = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
QUICK = "--quick" in ARGV
QUICK_FRAMES = (0, 9, 16, 32, 47, 63)


def fresh(scale: int, name: str, lean: str) -> dict:
    S.clear()
    m = M.mats()
    S.setup_scene(scale, *P.SPRITES.get(name, (P.KNOB_FRAME_PX,) * 2), lean=P.LEAN[lean])
    return m


def render_knob(out: Path, scale: int) -> None:
    """Cap and its shadow per frame (plate is a shadow catcher, not drawn), then the arc ring
    and the numbered scale as their own flat layers."""
    m = fresh(scale, "knob", "knob")
    S.flags(PL.plate(M.catcher_mats(), "red"), catcher=True)
    fluted, turned, pitch = K.knob_cap(m)
    mesh_rot0 = -0.4 * pitch  # centres a flute facet under the pointer
    for i in QUICK_FRAMES if QUICK else range(P.KNOB_FRAMES):
        rot = -math.radians(P.knob_angle_deg(i))  # frame angle is clockwise
        for o in fluted:
            o.rotation_euler[2] = rot + mesh_rot0
        for o in turned:
            o.rotation_euler[2] = rot
        S.render(P.frame_path(out, scale, i))
    for name, make in (("knob_ring", K.knob_ring), ("knob_scale", K.knob_scale)):
        make(fresh(scale, name, "plate"))
        cy = bpy.context.scene.cycles
        cy.use_denoising, cy.samples = False, 64  # flat diffuse decal: converges clean
        S.render(out / f"{name}_{scale}x.png")


def render_caps(out: Path, scale: int) -> None:
    """One sprite per cap (plate hidden), then one shadow layer per cap shape (cap holdout)."""
    jobs = [(f"drawbar_cap_{c}", "drawbar", f"cap_{c}", C.fader_cap) for c in P.DRAWBAR_COLORS]
    jobs.append(("layer_cap_grey", "layer", "cap_grey", C.layer_cap))
    for name, lean, mat, make in jobs:
        m = fresh(scale, name, lean)
        S.flags(C.plate_slot(m), hide=True)
        make(m, mat)
        S.render(out / f"{name}_{scale}x.png")
    for name, lean, make in (
        ("drawbar_shadow", "drawbar", C.fader_cap),
        ("layer_shadow", "layer", C.layer_cap),
    ):
        m = fresh(scale, name, lean)
        S.flags(C.plate_slot(M.catcher_mats()), catcher=True)
        S.flags(make(m, "cap_black"), holdout=True)
        S.render(out / f"{name}_{scale}x.png")


def render_buttons(out: Path, scale: int) -> None:
    for color in ("grey", "black"):
        m = fresh(scale, f"button_{color}", "button")
        B.button(m, f"btn_{color}", f"btn_{color}_pill")
        S.render(out / f"button_{color}_{scale}x.png")


def render_plates(out: Path, scale: int) -> None:
    for name, key in (("plate_red", "red"), ("plate_slate", "slate"), ("plate_silver", "silver")):
        PL.plate(fresh(scale, name, "plate"), key)
        cy = bpy.context.scene.cycles
        cy.use_denoising, cy.samples = False, 256  # the denoiser would smear the fine grain
        S.render(out / f"{name}_{scale}x.png")
    C.plate_slot(fresh(scale, "plate_slate_slot", "plate"))
    S.render(out / f"plate_slate_slot_{scale}x.png")


STAGES = {
    "plates": render_plates,
    "knob": render_knob,
    "caps": render_caps,
    "buttons": render_buttons,
}


def main(out: Path) -> None:
    """`--quick` is for look-development only: 2x, few samples, six knob frames.
    `--only a,b` renders a subset of STAGES."""
    if QUICK:
        S.SAMPLES = 48
    only = ARGV[ARGV.index("--only") + 1].split(",") if "--only" in ARGV else list(STAGES)
    for scale in (2,) if QUICK else P.SCALES:
        for stage in only:
            STAGES[stage](out, scale)


if __name__ == "__main__":
    main(Path(ARGV[ARGV.index("--out") + 1]) if "--out" in ARGV else P.RAW_DIR)
