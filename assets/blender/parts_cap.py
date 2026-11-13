"""Drawbar / layer-fader cap and its slotted plate for the B7 proof (runs inside Blender).

Original art (D16): geometry authored from primitives (lofted sections); shapes follow the
owner's photos of the real parts. Sizes are numbers measured on the owner's private
reference (1 u = 129 reference px, see parts_knob.REF_PX_PER_U).
"""

from __future__ import annotations

import bpy
from proof_geom import box, loft

# Shape EMPIRICAL from the owner's photo of a loose cap (2026-11-13): one part, a tall
# near-square rear block with upright walls, an upright front face filleted into a long
# full-width nose about half its height, the nose front chamfered down, and a thin lip
# along the base behind the block. Plan size EMPIRICAL from the reference (100 x 231 px =
# 0.78 x 1.79 u at the base). Black and white caps share this geometry.
CAP_W = 0.80
CAP_BACK, CAP_FRONT = 0.90, -0.89
CAP_HT, CAP_HN = 0.90, 0.42  # EMPIRICAL (photos): nose about 0.47 of the block height
BLOCK_FRONT = 0.20  # EMPIRICAL (loose-cap photo): block is about 0.4 of the cap length
NOSE_TOP_FRONT = -0.66  # EMPIRICAL: flat nose top ends here, then the chamfer
NOSE_FRONT_H = 0.14  # EMPIRICAL: upright foot left under the chamfer
LIP_L, LIP_H = 0.06, 0.10  # EMPIRICAL (loose-cap photo): base lip behind the block
CAP_TAPER = 0.05  # EMPIRICAL: walls nearly upright (draft only)
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
    return [
        (left, y, zl),
        (hb, y, zl),
        (hb, y, zl + v),
        (top_x[1], y, zl + z),
        (top_x[0], y, zl + z),
        (left, y, zl + v),
    ]


def fader_cap(m, key):
    """Drawbar cap, one loft from back to front: base lip, tall upright rear block, step
    down onto the full-width nose (the bevel makes the fillet), chamfered nose front."""
    ht, hn, e = CAP_HT, CAP_HN, 0.005
    top = (-CAP_W / 2 * (1 - CAP_TAPER), CAP_W / 2 * (1 - CAP_TAPER))
    secs = [
        (CAP_BACK, LIP_H, top),
        (CAP_BACK - LIP_L, LIP_H, top),
        (CAP_BACK - LIP_L - e, ht, top),
        (BLOCK_FRONT, ht, top),
        (BLOCK_FRONT - e, hn, top),
        (NOSE_TOP_FRONT, hn, top),
        (CAP_FRONT, NOSE_FRONT_H, top),
    ]
    rings = [_section(y, z, tx) for y, z, tx in secs]
    return [loft("cap_body", rings, m[key], smooth=False, bevel=CAP_BEVEL, segments=8)]


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
