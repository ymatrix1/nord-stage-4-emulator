"""Procedural materials for the B7 proof (runs inside Blender). No image textures.
Original art (D16).

Base colours come from assets/panel/theme_samples.json (colours measured on the owner's
reference render; numbers only, D16). `GAIN` holds EMPIRICAL per-channel factors so the rendered
flat colour lands on the measured colour under the scene light.
"""

from __future__ import annotations

import json
from pathlib import Path

import bpy

SAMPLES_JSON = Path(__file__).resolve().parents[1] / "panel" / "theme_samples.json"


def srgb_to_linear(c: float) -> float:
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def rgb_linear(rgb8, gain=1.0) -> tuple[float, float, float, float]:
    g = gain if isinstance(gain, tuple) else (gain,) * 3
    return (*[min(1.0, srgb_to_linear(v / 255) * k) for v, k in zip(rgb8, g)], 1.0)


# EMPIRICAL: per-channel albedo gains (linear) so each rendered flat surface lands on its
# measured colour under the scene light; solved by rendering, taking the median of a flat patch
# and multiplying by target / rendered in linear light (docs/research/panel-blender-proof.md,
# checked by `test_committed_sprites_match_sampled_colours`).
GAIN = {
    "red": (1.058, 0.649, 0.837),
    "slate": (1.072, 1.123, 1.136),
    "silver": (1.091, 1.101, 1.078),
    "cap_black": (0.38, 0.307, 0.24),
    "cap_white": (1.059, 1.05, 1.039),
    "cap_grey": (1.418, 1.407, 1.368),
    "ring": (1.08, 1.08, 1.06),
    "knob_side": (1.8, 1.8, 1.8),
    "knob_top": (0.7, 0.66, 0.6),
}


def theme() -> dict:
    return json.loads(SAMPLES_JSON.read_text(encoding="utf-8"))["colors"]


def set_input(node, name: str, value) -> None:
    if name in node.inputs:
        node.inputs[name].default_value = value


def _bump(nt, bsdf, height_socket, strength, distance=0.02):
    bump = nt.nodes.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = strength
    bump.inputs["Distance"].default_value = distance
    nt.links.new(height_socket, bump.inputs["Height"])
    nt.links.new(bump.outputs["Normal"], bsdf.inputs["Normal"])


def _tex(nt, kind):
    """Texture node driven by object coordinates, so scales are per Blender unit (the default
    generated coordinates are normalised to the bounding box)."""
    coord = nt.nodes.get("coord") or nt.nodes.new("ShaderNodeTexCoord")
    coord.name = "coord"
    tex = nt.nodes.new(kind)
    nt.links.new(coord.outputs["Object"], tex.inputs["Vector"])
    return tex


def _speckle(nt, bsdf, base, fleck, scale):
    """Fine voronoi colour speckle (anodised / powder-coated metal)."""
    vor = _tex(nt, "ShaderNodeTexVoronoi")
    vor.inputs["Scale"].default_value = scale
    ramp = nt.nodes.new("ShaderNodeValToRGB")
    ramp.color_ramp.elements[0].position = 0.0
    ramp.color_ramp.elements[1].position = 0.4
    ramp.color_ramp.elements[0].color = tuple(min(1.0, c * (1 + fleck * 2)) for c in base[:3]) + (
        1.0,
    )
    ramp.color_ramp.elements[1].color = tuple(c * (1 - fleck) for c in base[:3]) + (1.0,)
    nt.links.new(vor.outputs["Distance"], ramp.inputs["Fac"])
    nt.links.new(ramp.outputs["Color"], bsdf.inputs["Base Color"])


def material(
    name: str,
    rgb8,
    rough: float,
    *,
    gain=1.0,
    metal: float = 0.0,
    coat: float = 0.0,
    spec: float = 0.5,
    grain: float = 0.0,
    grain_scale: float = 60.0,
    pebble: float = 0.0,
    pebble_scale: float = 45.0,
    fleck: float = 0.0,
    fleck_scale: float = 160.0,
) -> bpy.types.Material:
    """Principled material. `grain` = noise bump, `pebble` = voronoi cell bump (leather-like
    pebbled grain), `fleck` = fine voronoi colour speckle (anodised metal)."""
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    nt = mat.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    base = rgb_linear(rgb8, gain)
    set_input(bsdf, "Base Color", base)
    set_input(bsdf, "Roughness", rough)
    set_input(bsdf, "Metallic", metal)
    set_input(bsdf, "Coat Weight", coat)
    set_input(bsdf, "Coat Roughness", 0.05)
    set_input(bsdf, "Specular IOR Level", spec)
    if grain:
        noise = _tex(nt, "ShaderNodeTexNoise")
        noise.inputs["Scale"].default_value = grain_scale
        noise.inputs["Detail"].default_value = 6.0
        noise.inputs["Roughness"].default_value = 0.7
        _bump(nt, bsdf, noise.outputs["Fac"], grain)
    if pebble:
        vor = _tex(nt, "ShaderNodeTexVoronoi")
        vor.feature = "SMOOTH_F1"
        vor.inputs["Scale"].default_value = pebble_scale
        _bump(nt, bsdf, vor.outputs["Distance"], pebble, 0.02)
    if fleck:
        _speckle(nt, bsdf, base, fleck, fleck_scale)
    return mat


def mats() -> dict[str, bpy.types.Material]:
    t = theme()
    u = {k: v["use"] for k, v in t.items()}
    return {
        "red": material(
            "red_leather",
            u["panel_red"],
            0.55,
            gain=GAIN["red"],
            spec=0.12,
            pebble=1.3,
            pebble_scale=34,
            grain=0.25,
            grain_scale=140,
        ),
        "slate": material(
            "dark_slate",
            u["panel_slate"],
            0.5,
            gain=GAIN["slate"],
            spec=0.15,
            metal=0.2,
            fleck=0.12,
            grain=0.3,
            grain_scale=140,
            pebble=0.9,
            pebble_scale=45,
        ),
        "silver": material(
            "header_silver",
            u["header_silver"],
            0.55,
            gain=GAIN["silver"],
            spec=0.2,
            fleck=0.05,
            fleck_scale=220,
            pebble=0.7,
            pebble_scale=40,
        ),
        "slot": material("slot_floor", u["slot_floor"], 0.8),
        "well": material("button_well", u["button_well"], 0.9, spec=0.1),
        "knob": material("knob_rubber", u["knob_side"], 0.32, gain=GAIN["knob_side"], spec=0.8),
        "knob_top": material("knob_top", u["knob_top"], 0.5, gain=GAIN["knob_top"], spec=0.3),
        "pointer": material("pointer", u["pointer"], 0.45),
        "ring": material("ring", u["ring_white"], 0.6, gain=GAIN["ring"], spec=0.2),
        "cap_black": material(
            "cap_black", u["cap_black_top"], 0.32, gain=GAIN["cap_black"], spec=0.8
        ),
        "cap_white": material(
            "cap_white", u["cap_white_top"], 0.35, gain=GAIN["cap_white"], spec=0.6
        ),
        "cap_grey": material(
            "cap_grey",
            u["cap_grey_top"],
            0.4,
            gain=GAIN["cap_grey"],
            spec=0.6,
            grain=0.5,
            grain_scale=200,
        ),
    }


def projected(name: str, image_path) -> bpy.types.Material:
    """Unlit material showing `image_path` in screen space (window coordinates), i.e. the
    reference crop projected from the render camera onto whatever geometry it hits (D17)."""
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    nt = mat.node_tree
    nt.nodes.clear()
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    emit = nt.nodes.new("ShaderNodeEmission")
    coord = nt.nodes.new("ShaderNodeTexCoord")
    tex = nt.nodes.new("ShaderNodeTexImage")
    tex.image = bpy.data.images.load(str(image_path), check_existing=True)
    tex.extension = "EXTEND"
    tex.interpolation = "Cubic"
    nt.links.new(coord.outputs["Window"], tex.inputs["Vector"])
    nt.links.new(tex.outputs["Color"], emit.inputs["Color"])
    nt.links.new(emit.outputs["Emission"], out.inputs["Surface"])
    return mat


def catcher_mats() -> dict[str, bpy.types.Material]:
    """Plain diffuse stand-ins for shadow-catcher grounds: the grain of the real plates would
    otherwise leak into the shadow alpha and bloat the PNGs."""
    flat = material("catcher", (128, 128, 128), 1.0, spec=0.0)
    return {"red": flat, "slate": flat, "slot": flat, "silver": flat}
