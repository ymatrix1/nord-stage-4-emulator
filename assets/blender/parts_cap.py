"""Drawbar / layer-fader cap and its slotted plate for the B7 proof (runs inside Blender).

Original art (D16): geometry authored from primitives (lofted sections); shapes follow the
owner's photos of the real parts. Sizes are numbers measured on the owner's private
reference (1 u = 129 reference px, see parts_knob.REF_PX_PER_U).
"""

from __future__ import annotations

import bpy
from proof_geom import box, loft

# Shape EMPIRICAL, measured on the owner's photo of a loose cap (2026-11-13, oblique view,
# lengths scaled by the 113 px cap width at the block top, verticals by the block's front
# corner edge; about +/-15 % from perspective). One part: base lip behind a tall rear block,
# block front face sloping forward onto a long full-width nose (rounded join), nose top
# chamfered at the front onto an upright front wall. Plan size EMPIRICAL from the reference
# (100 x 231 px = 0.78 x 1.79 u at the base). Black and white caps share this geometry.
CAP_W = 0.80
CAP_BACK, CAP_FRONT = 0.90, -0.89
# Profile EMPIRICAL from the owner's side photo (2026-11-13, white cap, 342 px base length =
# 1.79 u, 0.0052 u/px, about +/-10 % from the low camera); width taper and foot flare from
# the owner's rear photo of three caps.
CAP_HT, CAP_HN = 1.00, 0.47  # block 215 px, nose 100 px tall
BLOCK_TOP_FRONT = 0.25  # block top front edge at 217 px from the nose tip
BLOCK_FRONT = 0.04  # block front face meets the nose at 177 px: steep forward slope
NOSE_TOP_FRONT = -0.66  # flat nose top ends at 44 px, then the chamfer
NOSE_FRONT_H = 0.10  # short upright nose front wall under the chamfer
LIP_L, LIP_H = 0.07, 0.20  # base step behind the block: 15 px out, 45 px tall
CAP_TAPER = 0.20  # rear photo: block top about 0.8 of its base width
FLARE_H = 0.14  # rear photo: walls flare out over the bottom ~0.14 u (concave foot)
STEP = 0.04  # ASSUMED: run of the near-vertical back wall, so the bevel is not clamped
CAP_BEVEL = 0.12  # EMPIRICAL: reference cap corners read about 8 px rounded at 2x
CAP_LIFT = 0.03  # cap floats above the plate so the slot shows under it
FOOT_WALL = 0.06  # ASSUMED: upright strip at the foot of every wall
# Layer-fader cap EMPIRICAL from its photo: a frustum, top smaller than the base on all sides,
# rounded edges. Base 96 x 74 reference px.
LAYER_W, LAYER_L, LAYER_H = 0.74, 0.61, 0.36
LAYER_TOP = (0.82, 0.50)  # top width / length as a fraction of the base
LAYER_BEVEL = 0.07
# EMPIRICAL: slot is about 22 px wide on the reference, 0.22 of the cap width.
SLOT_W, SLOT_LEN, SLOT_DEPTH, PLATE_T = 0.17, 3.4, 0.16, 0.2


def _section(y, z, top_x, left=-CAP_W / 2):
    """Cross-section at y: bottom from `left` to the right edge, top edge between `top_x`."""
    hb, zl = CAP_W / 2, CAP_LIFT
    v = min(FOOT_WALL, z * 0.5)  # short upright wall: every bottom edge is 90 deg, so the
    # bevel shrinks the outline the same everywhere (sloped walls would bevel deeper)
    f = min(FLARE_H, z * 0.6)  # flared foot: steep inward kink, then the tapered wall
    fx = hb - (hb - top_x[1]) * 0.5 * f / max(z, 1e-6) - 0.03 * (f / FLARE_H)
    return [
        (left, y, zl),
        (hb, y, zl),
        (hb, y, zl + v),
        (fx, y, zl + f),
        (top_x[1], y, zl + z),
        (top_x[0], y, zl + z),
        (-fx, y, zl + f),
        (left, y, zl + v),
    ]


def fader_cap(m, key):
    """Drawbar cap, one loft from back to front: base lip, tall rear block whose front face
    slopes onto the full-width nose, chamfered nose top, upright front wall (the end cap of
    the loft). Smooth shading plus an unclamped bevel round every edge (photo)."""
    ht, hn = CAP_HT, CAP_HN
    top = (-CAP_W / 2 * (1 - CAP_TAPER), CAP_W / 2 * (1 - CAP_TAPER))
    secs = [
        (CAP_BACK, LIP_H),
        (CAP_BACK - LIP_L, LIP_H),
        (CAP_BACK - LIP_L - STEP, ht),
        (BLOCK_TOP_FRONT, ht),
        (BLOCK_FRONT, hn),
        (NOSE_TOP_FRONT, hn),
        (CAP_FRONT, NOSE_FRONT_H),
    ]
    rings = [_section(y, z, top) for y, z in secs]
    return [loft("cap_body", rings, m[key], smooth=True, bevel=CAP_BEVEL, segments=8)]


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
