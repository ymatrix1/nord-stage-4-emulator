"""Headless Blender scene for the Phase 6b B7 proof: ONE knob cap and ONE fader cap.

Run (Blender 5.2, no GUI, no add-ons):

    blender --background --factory-startup --python assets/blender/build_proof.py -- --out <dir>

or `python tools/panel_assets.py render`. Writes raw frames to <dir>; `tools/panel_assets.py pack`
stitches and hashes them.

ORIGINAL ART (D16): all geometry below is authored here from primitives; all materials are
procedural. Nothing is imported, traced or sampled from any photo or model of the hardware.
Photos were used only to measure proportions and to pick theme colours (tokens from
docs/specs/visual-style-and-panel.md section 2).

One top-left light for the whole scene (D15): a sun at azimuth top-left, elevation ~55 deg.
"""

from __future__ import annotations

import math
import sys
from pathlib import Path

import bmesh
import bpy
from mathutils import Vector

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools"))
import panel_assets as P  # noqa: E402  (shared constants, no bpy)

# Theme tokens, sRGB hex. VERIFIED: copied from the spec token table
# (docs/specs/visual-style-and-panel.md s2).
RED = "#8E1B1F"  # chassis_red_primary
SLATE = "#323D47"  # module_slate_bg
CAP_GREY = "#8C9196"  # button_grey_layer, also the fader cap token in spec 4.1
RUBBER = "#1C1F22"  # button_black_pill
POINTER = "#ECEBE6"  # drawbar_white

# ASSUMED: light placement, plausible for a top-left key light; fixed by D15.
SUN_DIR = Vector((-1.0, 1.0, 2.0))  # from scene origin toward the light
SUN_STRENGTH = 3.2
SUN_ANGLE = math.radians(9)  # soft penumbra
SAMPLES = 128
# ASSUMED: camera tilts 14 deg toward the player (orthographic) so cap sides and slot walls show;
# every sprite shares this camera so layers stay aligned. Confirm against the real panel in B8.
CAM_TILT = math.radians(14)

# Geometry, Blender units (64 px per unit at 1x). ASSUMED unless noted; proportions measured
# as ratios from the reference photo only (cap diameter ~ fader-cap width).
KNOB_R, KNOB_H, KNOB_RIBS = 0.36, 0.34, 36
FADER_W, FADER_D, FADER_H_FRONT, FADER_H_BACK = 0.92, 0.78, 0.30, 0.50
SLOT_W, SLOT_LEN, SLOT_DEPTH, PLATE_T = 0.22, 3.2, 0.16, 0.2


def srgb_to_linear(c: float) -> float:
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def hex_rgba(h: str) -> tuple[float, float, float, float]:
    v = [int(h[i : i + 2], 16) / 255 for i in (1, 3, 5)]
    return (*[srgb_to_linear(x) for x in v], 1.0)


def set_input(node, name: str, value) -> None:
    if name in node.inputs:
        node.inputs[name].default_value = value


def material(
    name: str,
    color: str,
    rough: float,
    metal: float = 0.0,
    coat: float = 0.0,
    bump: float = 0.0,
    fleck: float = 0.0,
    spec: float = 0.5,
) -> bpy.types.Material:
    """Principled material. `bump` = fine procedural grain; `fleck` = metallic-flake colour
    variation (procedural noise through a colour ramp), the sparkle of lacquer or anodised metal."""
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    nt = mat.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    base = hex_rgba(color)
    set_input(bsdf, "Base Color", base)
    set_input(bsdf, "Roughness", rough)
    set_input(bsdf, "Metallic", metal)
    set_input(bsdf, "Coat Weight", coat)
    set_input(bsdf, "Coat Roughness", 0.03)
    set_input(bsdf, "Specular IOR Level", spec)  # keeps theme colours from washing out
    if bump:
        noise = nt.nodes.new("ShaderNodeTexNoise")
        noise.inputs["Scale"].default_value = 900.0
        noise.inputs["Detail"].default_value = 3.0
        node = nt.nodes.new("ShaderNodeBump")
        node.inputs["Strength"].default_value = bump
        nt.links.new(noise.outputs["Fac"], node.inputs["Height"])
        nt.links.new(node.outputs["Normal"], bsdf.inputs["Normal"])
    if fleck:
        vor = nt.nodes.new("ShaderNodeTexVoronoi")
        vor.inputs["Scale"].default_value = 420.0
        ramp = nt.nodes.new("ShaderNodeValToRGB")
        ramp.color_ramp.elements[0].position, ramp.color_ramp.elements[1].position = 0.0, 0.35
        dark = tuple(c * (1.0 - fleck) for c in base[:3]) + (1.0,)
        light = tuple(min(1.0, c * (1.0 + fleck * 2.5)) for c in base[:3]) + (1.0,)
        ramp.color_ramp.elements[0].color, ramp.color_ramp.elements[1].color = light, dark
        nt.links.new(vor.outputs["Distance"], ramp.inputs["Fac"])
        nt.links.new(ramp.outputs["Color"], bsdf.inputs["Base Color"])
    return mat


def mats() -> dict[str, bpy.types.Material]:
    return {
        "red": material("red_lacquer", RED, 0.5, coat=0.12, spec=0.08, bump=0.02, fleck=0.12),
        "slate": material("dark_slate", SLATE, 0.5, metal=0.3, spec=0.1, bump=0.12, fleck=0.15),
        "rubber": material("rubber", RUBBER, 0.38, bump=0.08),
        "cap": material("matte_cap", CAP_GREY, 0.55, bump=0.05),
        "pointer": material("pointer", POINTER, 0.45),
        "slot": material("slot_floor", "#07090B", 0.8),
    }


def link(obj: bpy.types.Object) -> bpy.types.Object:
    bpy.context.scene.collection.objects.link(obj)
    return obj


def mesh_obj(name: str, bm: bmesh.types.BMesh, mat, smooth: bool = True, bevel: float = 0.0):
    me = bpy.data.meshes.new(name)
    bm.normal_update()
    bm.to_mesh(me)
    bm.free()
    obj = link(bpy.data.objects.new(name, me))
    obj.data.materials.append(mat)
    for poly in me.polygons:
        poly.use_smooth = smooth
    if bevel:
        mod = obj.modifiers.new("bevel", "BEVEL")
        mod.width, mod.segments, mod.limit_method = bevel, 3, "ANGLE"
    return obj


def box(
    name: str,
    size: tuple[float, float, float],
    center: tuple[float, float, float],
    mat,
    bevel: float = 0.0,
):
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    bmesh.ops.scale(bm, vec=size, verts=bm.verts)
    bmesh.ops.translate(bm, vec=center, verts=bm.verts)
    return mesh_obj(name, bm, mat, smooth=False, bevel=bevel)


def knob_cap(m: dict, angle_deg: float) -> list:
    """Fluted rubber cap: deep vertical flutes, rounded shoulder, slightly dished top, a glossy
    smooth skirt ring at the base and a bevelled pointer inlay."""
    bm = bmesh.new()
    n = KNOB_RIBS * 2
    rings = []
    # (radius scale, z, flute depth): base, flute end, shoulder, rim, dished centre
    for rs, z, rib in (
        (1.0, 0.04, 0.09),
        (0.98, KNOB_H * 0.72, 0.09),
        (0.93, KNOB_H * 0.92, 0.03),
        (0.84, KNOB_H, 0.0),
        (0.62, KNOB_H - 0.012, 0.0),
    ):
        ring = []
        for i in range(n):
            a = 2 * math.pi * i / n
            r = KNOB_R * rs * (1.0 - (rib if i % 2 else 0.0))
            ring.append(bm.verts.new((r * math.cos(a), r * math.sin(a), z)))
        rings.append(ring)
    for r0, r1 in zip(rings, rings[1:]):
        for i in range(n):
            j = (i + 1) % n
            bm.faces.new((r0[i], r0[j], r1[j], r1[i]))
    bm.faces.new(rings[-1])
    cap = mesh_obj("knob_cap", bm, m["rubber"], smooth=True, bevel=0.006)
    bm = bmesh.new()
    bmesh.ops.create_cone(
        bm, cap_ends=True, segments=96, radius1=KNOB_R * 1.12, radius2=KNOB_R * 1.06, depth=0.07
    )
    bmesh.ops.translate(bm, vec=(0, 0, 0.035), verts=bm.verts)
    skirt = mesh_obj("knob_skirt", bm, m["rubber"], smooth=True, bevel=0.012)
    pointer = box(
        "knob_pointer",
        (0.045, KNOB_R * 0.62, 0.016),
        (0, KNOB_R * 0.50, KNOB_H - 0.004),
        m["pointer"],
        bevel=0.006,
    )
    rot = -math.radians(angle_deg)  # Blender Z is counter-clockwise; frame angle is clockwise
    for o in (cap, pointer):
        o.rotation_euler = (0, 0, rot)
    return [cap, skirt, pointer]


def fader_cap(m: dict) -> list:
    """Wedge cap extruded from a side profile: stepped front, long sloped top, short back face,
    three grip ridges across the slope. Floats 0.03 above the plate."""
    prof = [
        (-0.39, 0.0),
        (-0.39, 0.22),
        (-0.30, 0.30),
        (-0.16, 0.34),
        (0.32, 0.56),
        (0.39, 0.56),
        (0.39, 0.0),
    ]  # (y, z)
    bm = bmesh.new()
    lo = [bm.verts.new((-FADER_W / 2, y, z + 0.03)) for y, z in prof]
    hi = [bm.verts.new((FADER_W / 2, y, z + 0.03)) for y, z in prof]
    bm.faces.new(lo[::-1])
    bm.faces.new(hi)
    for i in range(len(prof)):
        j = (i + 1) % len(prof)
        bm.faces.new((lo[i], lo[j], hi[j], hi[i]))
    body = mesh_obj("fader_cap", bm, m["cap"], smooth=False, bevel=0.03)
    tilt = math.atan2(0.56 - 0.34, 0.32 + 0.16)
    parts = [body]
    for k, y in enumerate((0.06, 0.20)):
        z = 0.03 + 0.34 + (y + 0.16) * math.tan(tilt)
        g = box(
            f"fader_grip{k}", (FADER_W * 0.78, 0.022, 0.012), (0, y, z), m["rubber"], bevel=0.006
        )
        g.rotation_euler = (tilt, 0, 0)
        parts.append(g)
    return parts


def plate_slot(m: dict, hidden_slot: bool = False):
    """Dark slate plate with a recessed fader slot (boolean difference, applied)."""
    plate = box("plate_slate", (2.8, 4.8, PLATE_T), (0, 0, -PLATE_T / 2), m["slate"], bevel=0.0)
    cutter = box("slot_cutter", (SLOT_W, SLOT_LEN, SLOT_DEPTH * 2), (0, 0, 0), m["slot"])
    mod = plate.modifiers.new("slot", "BOOLEAN")
    mod.operation, mod.object, mod.solver = "DIFFERENCE", cutter, "EXACT"
    bpy.context.view_layer.objects.active = plate
    bpy.ops.object.modifier_apply(modifier="slot")
    bpy.data.objects.remove(cutter, do_unlink=True)
    edge = plate.modifiers.new("edge", "BEVEL")
    edge.width, edge.segments, edge.limit_method = 0.012, 2, "ANGLE"
    floor = box("slot_floor", (SLOT_W, SLOT_LEN, 0.01), (0, 0, -SLOT_DEPTH), m["slot"])
    return [plate, floor]


def plate_red(m: dict):
    return [box("plate_red", (2.8, 2.8, PLATE_T), (0, 0, -PLATE_T / 2), m["red"])]


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
    sc.render.image_settings.file_format = "PNG"
    sc.render.image_settings.color_mode = "RGBA"
    sc.render.image_settings.color_depth = "8"
    sc.render.image_settings.compression = 100
    sc.view_settings.view_transform, sc.view_settings.look = "Standard", "None"
    sc.render.filter_size = 1.0
    world = bpy.data.worlds.new("w") if not sc.world else sc.world
    sc.world = world
    world.use_nodes = True
    bg = world.node_tree.nodes["Background"]
    bg.inputs["Color"].default_value = (0.85, 0.85, 0.88, 1.0)
    bg.inputs["Strength"].default_value = 0.22  # ambient fill, ASSUMED
    cam = link(bpy.data.objects.new("cam", bpy.data.cameras.new("cam")))
    cam.data.type = "ORTHO"
    cam.data.ortho_scale = max(w_px, h_px) / P.PX_PER_UNIT
    cam.location = (0, -6 * math.sin(CAM_TILT), 6 * math.cos(CAM_TILT))
    cam.rotation_euler = (CAM_TILT, 0, 0)
    sc.camera = cam
    sun = link(bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN")))
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


def main(out: Path) -> None:
    for scale in P.SCALES:
        for name, make, size in (
            ("plate_red", plate_red, P.PLATE_PX["plate_red"]),
            ("plate_slate_slot", plate_slot, P.PLATE_PX["plate_slate_slot"]),
        ):
            clear()
            m = mats()
            setup_scene(scale, *size)
            make(m)
            render(out / f"{name}_{scale}x.png")

        # knob: cap + its own shadow baked per frame; plate is a shadow catcher (not drawn)
        clear()
        m = mats()
        setup_scene(scale, P.KNOB_FRAME_PX, P.KNOB_FRAME_PX)
        ground = plate_red(m)
        flags(ground, catcher=True)
        parts = knob_cap(m, 0.0)
        for i in range(P.KNOB_FRAMES):
            rot = -math.radians(P.knob_angle_deg(i))
            parts[0].rotation_euler[2] = parts[2].rotation_euler[2] = rot
            render(P.frame_path(out, scale, i))

        # fader: cap sprite (plate hidden), then shadow layer (cap holdout, plate catches)
        clear()
        m = mats()
        setup_scene(scale, P.FADER_FRAME_PX, P.FADER_FRAME_PX)
        ground = plate_slot(m)
        cap = fader_cap(m)
        flags(ground, hide=True)
        render(out / f"fader_cap_{scale}x.png")
        flags(ground, catcher=True)
        flags(cap, holdout=True)
        render(out / f"fader_shadow_{scale}x.png")


if __name__ == "__main__":
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    main(Path(argv[argv.index("--out") + 1]) if "--out" in argv else P.RAW_DIR)
