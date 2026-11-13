"""Headless Blender scene for the Phase 6b B7 proof: generic knob + ring, pointer and shadow
layers for the reference knobs, the slotted plate. Buttons, triangle
selectors, knob bodies, caps, the DRIVE scale and plate tiles come from tools/panel_refbake.py
(D17).

Run (Blender 5.2, no GUI, no add-ons):

    blender --background --factory-startup --python-exit-code 1 \
        --python assets/blender/build_proof.py -- --out <dir> [--quick] [--only knob,button]

or `python tools/panel_assets.py render`. Writes raw frames to <dir>; `tools/panel_assets.py pack`
stitches, makes the tiles seamless and hashes them. Modules: proof_scene.py (camera, light),
proof_materials.py (materials), parts_*.py (geometry).

Original art (D16) for everything rendered here: geometry from primitives, procedural
materials, nothing imported. Proportions and colours are numbers measured on the owner's
reference (assets/panel/theme_samples.json); reference-derived pixels are only used by
tools/panel_refbake.py under D17.
"""

from __future__ import annotations

import math
import sys
from pathlib import Path

import bpy

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parents[1] / "tools")]
import panel_assets as P  # noqa: E402  (shared constants, no bpy)
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


def _turn(fluted, turned, pitch, frame: int) -> None:
    rot = -math.radians(P.knob_angle_deg(frame))  # frame angle is clockwise
    for o in fluted:
        o.rotation_euler[2] = rot - 0.4 * pitch  # centres a flute facet under the pointer
    for o in turned:
        o.rotation_euler[2] = rot


def _frames():
    return QUICK_FRAMES if QUICK else range(P.KNOB_FRAMES)


def render_knob(out: Path, scale: int) -> None:
    """Generic knob style: cap and its shadow per frame (plate is a shadow catcher, not drawn),
    then the arc ring as its own flat layer."""
    m = fresh(scale, "knob", "knob")
    S.flags(PL.plate(M.catcher_mats(), "red"), catcher=True)
    fluted, turned, pitch = K.knob_cap(m)
    for i in _frames():
        _turn(fluted, turned, pitch, i)
        S.render(P.frame_path(out, scale, i))
    K.knob_ring(fresh(scale, "knob_ring", "plate"))
    cy = bpy.context.scene.cycles
    cy.use_denoising, cy.samples = False, 64  # flat diffuse decal: converges clean
    S.render(out / f"knob_ring_{scale}x.png")


def render_ref_knobs(out: Path, scale: int) -> None:
    """Per reference knob (D17): the cap body is cut from the reference by panel_refbake, so
    Blender renders only what turns, the pointer (cap as holdout, so it hides behind the cap),
    and the cap's shadow on the plate, both at that knob's own lean."""
    for name in P.REF_KNOBS:
        m = fresh(scale, f"knob_shadow_{name}", f"knob_{name}")
        S.flags(PL.plate(M.catcher_mats(), "red"), catcher=True)
        fluted, turned, pitch = K.knob_cap(m)
        S.flags(fluted + turned, holdout=True)
        S.render(out / f"knob_shadow_{name}_{scale}x.png")
        m = fresh(scale, f"pointer_{name}", f"knob_{name}")
        fluted, turned, pitch = K.knob_cap(m)
        S.flags(fluted, holdout=True)
        for i in _frames():
            _turn(fluted, turned, pitch, i)
            S.render(P.frame_path(out, scale, i, f"pointer_{name}"))


def render_plates(out: Path, scale: int) -> None:
    """Only the slotted slate plate; the tiles are cut from the reference (D17)."""
    C.plate_slot(fresh(scale, "plate_slate_slot", "plate"))
    S.render(out / f"plate_slate_slot_{scale}x.png")


STAGES = {
    "plates": render_plates,
    "knob": render_knob,
    "ref_knobs": render_ref_knobs,
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
