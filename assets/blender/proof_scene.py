"""Scene, light, camera and render helpers for the B7 proof (runs inside Blender).

One top-left light for the whole scene (D15). The camera is a PERSPECTIVE camera that looks
straight down at the panel, like the reference render: a control away from the optical axis shows
its top shifted outward and one side wall (the "lean"). `lean` is the camera offset from the
sprite centre in Blender units; the lens shift keeps the sprite centred in its frame, so the
panel plane stays isotropic and layers (plate, ring, shadow, sprite) stay aligned.

Original art (D16): no image, HDRI or photo is loaded; light and world are procedural.
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

# ASSUMED: light placement, plausible for a top-left key light; fixed by D15.
SUN_DIR = Vector((-1.0, 1.0, 2.6))  # from scene origin toward the light
SUN_STRENGTH = 1.7
SUN_ANGLE = math.radians(22)  # soft penumbra
AMBIENT = 0.58  # diffuse ambient strength (v2 value)
AMBIENT_RGB = (0.85, 0.85, 0.88)
# EMPIRICAL: what glossy rays see, as (|sin elevation|, brightness): a bright ring 35 to 46 deg
# up, dim zenith and horizon. A surface tilted about 20 deg mirrors the ring into the camera, so
# rounded edges and knob rims glint while flat tops and steep walls stay dark, as on the
# reference; tuned at the same feature size.
GLOSS_RING = ((0.0, 0.12), (0.50, 0.15), (0.56, 1.0), (0.72, 1.0), (0.78, 0.18), (1.0, 0.18))
GLOSS_GAIN = 2.0  # EMPIRICAL: ring brightness (world strength; diffuse ambient is divided by it)
SAMPLES = 128


def setup_scene(scale: int, w_px: int, h_px: int, lean=(0.0, 0.0)) -> None:
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
    studio_world(sc)
    cam = link_obj(bpy.data.objects.new("cam", bpy.data.cameras.new("cam")))
    cam.data.type = "PERSP"
    fit_u = max(w_px, h_px) / P.PX_PER_UNIT  # frame size on the panel plane, longer side
    cam.data.sensor_width = 36.0
    cam.data.sensor_fit = "AUTO"
    cam.data.lens = P.CAM_HEIGHT_U * 36.0 / fit_u  # frame covers fit_u at the panel plane
    cam.data.clip_end = 100.0
    cam.data.shift_x, cam.data.shift_y = -lean[0] / fit_u, -lean[1] / fit_u
    cam.location = (lean[0], lean[1], P.CAM_HEIGHT_U)
    cam.rotation_euler = (0.0, 0.0, 0.0)  # straight down, screen up is +Y
    sc.camera = cam
    sun = link_obj(bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN")))
    sun.data.energy, sun.data.angle = SUN_STRENGTH, SUN_ANGLE
    sun.location = SUN_DIR * 5
    sun.rotation_euler = (-SUN_DIR).to_track_quat("-Z", "Y").to_euler()
    sun.visible_glossy = False  # matte tops like the reference; glints come from GLOSS_RING


def studio_world(sc) -> None:
    """Shadowless ambient. Diffuse light is uniform (as in v2); glossy rays instead see a ring
    softbox (GLOSS_RING), which gives the soft edge highlights of the reference caps and knob
    rims without lightening flat tops or sloped faces. The sun stays the only shadow-casting
    light (D15)."""
    world = sc.world or bpy.data.worlds.new("w")
    sc.world = world
    world.use_nodes = True
    nt = world.node_tree
    nt.nodes.clear()
    out = nt.nodes.new("ShaderNodeOutputWorld")
    bg = nt.nodes.new("ShaderNodeBackground")
    bg.inputs["Strength"].default_value = GLOSS_GAIN
    coord = nt.nodes.new("ShaderNodeTexCoord")
    sep = nt.nodes.new("ShaderNodeSeparateXYZ")
    absz = nt.nodes.new("ShaderNodeMath")
    absz.operation = "ABSOLUTE"
    ramp = nt.nodes.new("ShaderNodeValToRGB")
    els = ramp.color_ramp.elements
    while len(els) < len(GLOSS_RING):
        els.new(0.5)
    for el, (pos, v) in zip(els, GLOSS_RING):
        el.position, el.color = pos, (v, v, v * 1.02, 1.0)
    path = nt.nodes.new("ShaderNodeLightPath")
    mix = nt.nodes.new("ShaderNodeMix")
    mix.data_type = "RGBA"
    mix.inputs["A"].default_value = (*(c * AMBIENT / GLOSS_GAIN for c in AMBIENT_RGB), 1.0)
    nt.links.new(coord.outputs["Generated"], sep.inputs["Vector"])
    nt.links.new(sep.outputs["Z"], absz.inputs[0])
    nt.links.new(absz.outputs["Value"], ramp.inputs["Fac"])
    nt.links.new(ramp.outputs["Color"], mix.inputs["B"])
    nt.links.new(path.outputs["Is Glossy Ray"], mix.inputs["Factor"])
    nt.links.new(mix.outputs["Result"], bg.inputs["Color"])
    nt.links.new(bg.outputs["Background"], out.inputs["Surface"])


def link_obj(obj):
    bpy.context.scene.collection.objects.link(obj)
    return obj


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
