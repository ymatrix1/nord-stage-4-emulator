"""Headless Blender scene for the Phase 6b B7 proof: knob cap + arc ring, fader caps, plates.

Run (Blender 5.2, no GUI, no add-ons):

    blender --background --factory-startup --python-exit-code 1 \
        --python assets/blender/build_proof.py -- --out <dir>

or `python tools/panel_assets.py render`. Writes raw frames to <dir>; `tools/panel_assets.py pack`
stitches and hashes them. Modules: proof_materials.py (materials), proof_parts.py (geometry).

ORIGINAL ART (D16): all geometry is authored from primitives and all materials are procedural.
Nothing is imported, traced or sampled from any photo or model of the hardware. The owner's
reference images were used only to read proportions and colour numbers
(assets/panel/theme_samples.json).

One top-left light for the whole scene (D15): a sun at azimuth top-left, elevation ~61 deg.
"""

from __future__ import annotations

import math
import sys
from pathlib import Path

import bpy
from mathutils import Vector

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parents[1] / "tools")]
import panel_assets as P  # noqa: E402  (shared constants, no bpy)
import proof_materials as M  # noqa: E402
import proof_parts as G  # noqa: E402

# ASSUMED: light placement, plausible for a top-left key light; fixed by D15.
SUN_DIR = Vector((-1.0, 1.0, 2.6))  # from scene origin toward the light
SUN_STRENGTH = 1.7
SUN_ANGLE = math.radians(22)  # soft penumbra
AMBIENT = 0.58
SAMPLES = 128
# EMPIRICAL: the reference renders look down at the panel at about 35-40 deg; every sprite
# shares this orthographic camera so layers stay aligned.
CAM_TILT = math.radians(P.CAM_TILT_DEG)


def setup_scene(scale: int, w_px: int, h_px: int) -> None:
    sc = bpy.context.scene
    sc.render.engine = "CYCLES"
    cy = sc.cycles
    cy.device, cy.samples, cy.seed, cy.use_animated_seed = "CPU", SAMPLES, 0, False
    cy.use_denoising, cy.denoiser = True, "OPENIMAGEDENOISE"
    cy.use_adaptive_sampling = False
    sc.render.resolution_x, sc.render.resolution_y = w_px * scale, h_px * scale
    sc.render.resolution_percentage = 100
    sc.render.film_transparent = True
    ims = sc.render.image_settings
    ims.file_format, ims.color_mode, ims.color_depth, ims.compression = "PNG", "RGBA", "8", 100
    sc.view_settings.view_transform, sc.view_settings.look = "Standard", "None"
    sc.render.filter_size = 1.0
    world = sc.world or bpy.data.worlds.new("w")
    sc.world = world
    world.use_nodes = True
    bg = world.node_tree.nodes["Background"]
    bg.inputs["Color"].default_value = (0.85, 0.85, 0.88, 1.0)
    bg.inputs["Strength"].default_value = AMBIENT
    cam = G.link(bpy.data.objects.new("cam", bpy.data.cameras.new("cam")))
    cam.data.type = "ORTHO"
    cam.data.ortho_scale = max(w_px, h_px) / P.PX_PER_UNIT
    cam.location = (0, -6 * math.sin(CAM_TILT), 6 * math.cos(CAM_TILT))
    cam.rotation_euler = (CAM_TILT, 0, 0)
    sc.camera = cam
    sun = G.link(bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN")))
    sun.data.energy, sun.data.angle = SUN_STRENGTH, SUN_ANGLE
    sun.location = SUN_DIR * 5
    sun.rotation_euler = (-SUN_DIR).to_track_quat("-Z", "Y").to_euler()


def clear() -> None:
    for o in list(bpy.data.objects):
        bpy.data.objects.remove(o, do_unlink=True)
    for coll in (bpy.data.meshes, bpy.data.materials, bpy.data.lights, bpy.data.cameras):
        for d in list(coll):
            coll.remove(d)


def render(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    bpy.context.scene.render.filepath = str(path)
    bpy.ops.render.render(write_still=True)


def flags(objs, catcher=False, holdout=False, hide=False) -> None:
    for o in objs:
        o.is_shadow_catcher, o.is_holdout, o.hide_render = catcher, holdout, hide


def fresh(scale: int, size: tuple[int, int]) -> dict:
    clear()
    m = M.mats()
    setup_scene(scale, *size)
    return m


QUICK = "--quick" in sys.argv
QUICK_FRAMES = (0, 16, 32, 47, 63)


def render_knob(out: Path, scale: int) -> None:
    """Cap and its shadow per frame (plate is a shadow catcher, not drawn), then the arc ring."""
    m = fresh(scale, (P.KNOB_FRAME_PX, P.KNOB_FRAME_PX))
    flags(G.plate_red(M.catcher_mats()), catcher=True)
    parts, pitch = G.knob_cap(m)
    mesh_rot0 = -0.4 * pitch  # centres a flute facet under the pointer
    for i in QUICK_FRAMES if QUICK else range(P.KNOB_FRAMES):
        rot = -math.radians(P.knob_angle_deg(i))  # frame angle is clockwise
        parts[0].rotation_euler[2] = parts[1].rotation_euler[2] = rot + mesh_rot0
        parts[2].rotation_euler[2] = parts[3].rotation_euler[2] = rot
        render(P.frame_path(out, scale, i))
    m = fresh(scale, (P.KNOB_FRAME_PX, P.KNOB_FRAME_PX))
    G.knob_ring(m)
    cy = bpy.context.scene.cycles
    cy.use_denoising, cy.samples = False, 64  # flat diffuse decal: converges clean without OIDN
    render(out / f"knob_ring_{scale}x.png")


def render_fader(out: Path, scale: int) -> None:
    """One cap sprite per colour (plate hidden), then one shadow layer (cap holdout)."""
    for color in P.FADER_COLORS:
        m = fresh(scale, (P.FADER_FRAME_PX, P.FADER_FRAME_PX))
        flags(G.plate_slot(m), hide=True)
        G.fader_cap(m, f"cap_{color}")
        render(out / f"fader_cap_{color}_{scale}x.png")
    m = fresh(scale, (P.FADER_FRAME_PX, P.FADER_FRAME_PX))
    flags(G.plate_slot(M.catcher_mats()), catcher=True)
    flags(G.fader_cap(m, "cap_black"), holdout=True)
    render(out / f"fader_shadow_{scale}x.png")


def main(out: Path, quick: bool = False) -> None:
    """`quick` is for look-development only: 2x, few samples, five knob frames, no packing."""
    global SAMPLES
    if quick:
        SAMPLES = 48
    for scale in (2,) if quick else P.SCALES:
        for name, make, size in (
            ("plate_red", G.plate_red, P.PLATE_PX["plate_red"]),
            ("plate_slate_slot", G.plate_slot, P.PLATE_PX["plate_slate_slot"]),
        ):
            make(fresh(scale, size))
            render(out / f"{name}_{scale}x.png")
        render_knob(out, scale)
        render_fader(out, scale)


if __name__ == "__main__":
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    main(Path(argv[argv.index("--out") + 1]) if "--out" in argv else P.RAW_DIR, QUICK)
