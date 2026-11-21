"""Procedural materials for the B7 proof (runs inside Blender). No image textures.

Base colours come from assets/panel/theme_samples.json (colours measured on the owner's
reference render; numbers only, D16). `GAIN` is an EMPIRICAL per-material factor so the
rendered flat colour lands on the measured colour under the scene light.
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


# EMPIRICAL: per-channel albedo gains (linear) so the rendered flat plate matches the measured
# colour under the scene light; solved from a render of each plate
# (docs/research/panel-blender-proof.md).
GAIN_RED = (1.12, 0.55, 0.71)
GAIN_SLATE = (0.66, 0.73, 0.80)


def theme() -> dict:
    return json.loads(SAMPLES_JSON.read_text(encoding="utf-8"))["colors"]


def set_input(node, name: str, value) -> None:
    if name in node.inputs:
        node.inputs[name].default_value = value


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
    fleck: float = 0.0,
) -> bpy.types.Material:
    """Principled material. `grain` = procedural bump (leather-like noise), `fleck` = fine
    voronoi colour speckle (anodised metal)."""
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
        noise = nt.nodes.new("ShaderNodeTexNoise")
        noise.inputs["Scale"].default_value = grain_scale
        noise.inputs["Detail"].default_value = 6.0
        noise.inputs["Roughness"].default_value = 0.7
        bump = nt.nodes.new("ShaderNodeBump")
        bump.inputs["Strength"].default_value = grain
        bump.inputs["Distance"].default_value = 0.02
        nt.links.new(noise.outputs["Fac"], bump.inputs["Height"])
        nt.links.new(bump.outputs["Normal"], bsdf.inputs["Normal"])
    if fleck:
        vor = nt.nodes.new("ShaderNodeTexVoronoi")
        vor.inputs["Scale"].default_value = 160.0
        ramp = nt.nodes.new("ShaderNodeValToRGB")
        ramp.color_ramp.elements[0].position = 0.0
        ramp.color_ramp.elements[1].position = 0.4
        ramp.color_ramp.elements[0].color = tuple(
            min(1.0, c * (1 + fleck * 2)) for c in base[:3]
        ) + (1.0,)
        ramp.color_ramp.elements[1].color = tuple(c * (1 - fleck) for c in base[:3]) + (1.0,)
        nt.links.new(vor.outputs["Distance"], ramp.inputs["Fac"])
        nt.links.new(ramp.outputs["Color"], bsdf.inputs["Base Color"])
    return mat


def mats() -> dict[str, bpy.types.Material]:
    t = theme()
    u = {k: v["use"] for k, v in t.items()}
    return {
        "red": material(
            "red_lacquer", u["panel_red"], 0.55, gain=GAIN_RED, spec=0.15, grain=0.9, grain_scale=90
        ),
        "slate": material(
            "dark_slate",
            u["panel_slate"],
            0.5,
            gain=GAIN_SLATE,
            spec=0.15,
            metal=0.2,
            fleck=0.12,
            grain=0.15,
            grain_scale=140,
        ),
        "slot": material("slot_floor", u["slot_floor"], 0.8),
        "knob": material("knob_rubber", u["knob_side"], 0.42, spec=0.35),
        "knob_top": material("knob_top", u["knob_top"], 0.30, spec=0.6, coat=0.25),
        "pointer": material("pointer", u["ring_white"], 0.4),
        "ring": material("ring", u["ring_white"], 0.5),
        "cap_black": material("cap_black", u["cap_black_top"], 0.40, gain=0.45, spec=0.25),
        "cap_white": material("cap_white", u["cap_white_top"], 0.35, spec=0.6),
        "cap_grey": material("cap_grey", u["cap_grey_top"], 0.38, spec=0.6),
    }


def catcher_mats() -> dict[str, bpy.types.Material]:
    """Plain diffuse stand-ins for shadow-catcher grounds: the grain of the real plates would
    otherwise leak into the shadow alpha and bloat the PNGs."""
    flat = material("catcher", (128, 128, 128), 1.0, spec=0.0)
    return {"red": flat, "slate": flat, "slot": flat}
