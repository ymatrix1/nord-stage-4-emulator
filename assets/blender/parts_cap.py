"""Drawbar / layer-fader cap and its slotted plate for the B7 proof (runs inside Blender).

Original art (D16): geometry authored from primitives (lofted sections); shapes follow the
owner's photos of the real parts. Sizes are numbers measured on the owner's private
reference (1 u = 129 reference px, see parts_knob.REF_PX_PER_U).
"""

from __future__ import annotations

import bpy
from proof_geom import box, loft

# Shape EMPIRICAL, measured on the owner's purchased 3D model of the instrument (D17; the
# model stays in .reference/, never committed): one drawbar cap mesh, projected onto the
# panel plane and scaled so its width is CAP_W (scratch command recorded in
# docs/research/panel-blender-proof.md, eighth review). Mesh sampling error about 0.02 u.
# Side profile as (distance from the nose tip, top height), nose tip first: chamfer to the
# tip, flat nose, block front slope, block top, block back slope, rear ledge. The model's
# stray 4-vertex quads near the tip are not part of the cap and are dropped.
CAP_W = 0.80
CAP_PROFILE = (
    (0.00, 0.02),
    (0.20, 0.36),
    (0.79, 0.36),
    (0.84, 0.42),
    (0.99, 1.03),
    (1.24, 1.04),
    (1.30, 0.95),
    (1.37, 0.40),
    (1.40, 0.36),
    (1.587, 0.36),
)
CAP_LEN_SCALE = 1.03  # EMPIRICAL: silhouette fit against the reference (model 1.587 u long)
CAP_FRONT = -0.878  # nose tip, 0.06 u toward the back: the leaned render then fits the reference
CAP_TAPER = 0.03  # model: top of the block 0.78 against a 0.80 base, walls upright
CAP_BEVEL = 0.05  # model: corner radius about 0.05 u
CAP_FLIP = True  # model orientation, nose away from the player (silhouette IoU 0.884 vs 0.868)
CAP_LIFT = 0.03  # cap floats above the plate so the slot shows under it
FOOT_WALL = 0.03  # ASSUMED: upright strip at the foot of every wall
# Layer-fader cap EMPIRICAL from its photo: a frustum, top smaller than the base on all sides,
# rounded edges. Base 96 x 74 reference px.
LAYER_W, LAYER_L, LAYER_H = 0.74, 0.61, 0.36
LAYER_TOP = (0.82, 0.50)  # top width / length as a fraction of the base
LAYER_BEVEL = 0.07
# EMPIRICAL: slot is about 22 px wide on the reference, 0.22 of the cap width.
SLOT_W, SLOT_LEN, SLOT_DEPTH, PLATE_T = 0.17, 3.4, 0.16, 0.2


def _section(y, z, top_x):
    """Cross-section at y: full-width bottom, short upright foot, top edge between `top_x`."""
    hb, zl = CAP_W / 2, CAP_LIFT
    v = min(FOOT_WALL, z * 0.5)  # every bottom edge is 90 deg, so the bevel is even
    return [
        (-hb, y, zl),
        (hb, y, zl),
        (hb, y, zl + v),
        (top_x[1], y, zl + z),
        (top_x[0], y, zl + z),
        (-hb, y, zl + v),
    ]


def fader_cap(m, key):
    """Drawbar cap, one loft along CAP_PROFILE from the nose tip (player side) to the back."""
    top = (-CAP_W / 2 * (1 - CAP_TAPER), CAP_W / 2 * (1 - CAP_TAPER))
    sign = -1 if CAP_FLIP else 1
    rings = [_section(sign * (CAP_FRONT + d * CAP_LEN_SCALE), z, top) for d, z in CAP_PROFILE]
    if CAP_FLIP:
        rings.reverse()
    return [loft("cap_body", rings, m[key], smooth=True, bevel=CAP_BEVEL, segments=6)]


def layer_cap(m, key):
    """Layer-fader cap: rounded frustum, top smaller than the base on all four sides."""
    w, ln, h = LAYER_W / 2, LAYER_L / 2, LAYER_H
    tw, tl = w * LAYER_TOP[0], ln * LAYER_TOP[1]
    rings = [
        [(-w, -ln, CAP_LIFT), (w, -ln, CAP_LIFT), (w, ln, CAP_LIFT), (-w, ln, CAP_LIFT)],
        [
            (-tw, -tl, CAP_LIFT + h),
            (tw, -tl, CAP_LIFT + h),
            (tw, tl, CAP_LIFT + h),
            (-tw, tl, CAP_LIFT + h),
        ],
    ]
    return [loft("layer_cap", rings, m[key], smooth=False, bevel=LAYER_BEVEL, segments=6)]


def plate_slot(m):
    """Dark slate plate with a recessed fader slot (boolean difference, applied)."""
    plate = box("plate_slate", (2.8, 4.8, PLATE_T), (0, 0, -PLATE_T / 2), m["slate"])
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
