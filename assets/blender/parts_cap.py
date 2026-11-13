"""Drawbar / layer-fader cap and its slotted plate for the B7 proof (runs inside Blender).

Original art (D16): geometry authored from primitives (lofted sections); shapes follow the
owner's photos of the real parts. Sizes are numbers measured on the owner's private
reference (1 u = 129 reference px, see parts_knob.REF_PX_PER_U).
"""

from __future__ import annotations

import bpy
from proof_geom import box, loft

# Shape EMPIRICAL from photos of the real parts (owner, 2026-11-13): the drawbar cap is one
# part: a tall rear block whose sides and front slope inward, and a long lower nose with a
# chamfered front, narrower and offset left (reference). Plan size EMPIRICAL from the
# reference (100 x 231 px = 0.78 x 1.79 u at the base). Black and white caps share this geometry.
CAP_W = 0.80
CAP_BACK, CAP_FRONT = 0.90, -0.89
CAP_HT, CAP_HN = 0.90, 0.42  # EMPIRICAL: owner photo of the panel, block about as tall as
# the cap is wide, nose about half of that
BLOCK_FRONT = -0.32  # EMPIRICAL (close-up photo): the block front slopes forward onto the nose
BLOCK_TOP_FRONT = 0.02  # top edge of that face
NOSE_TOP_FRONT = -0.58  # front edge of the short nose top, then the sloped nose front
# EMPIRICAL (reference, every drawbar): the nose top face spans this, left of centre; its
# right wall slopes down to the full-width base
NOSE_TOP_X = (-0.43, 0.12)
NOSE_LEFT = -0.45  # EMPIRICAL: the nose base sticks out left of the block (silhouette fit)
NOSE_FRONT_H = 0.12  # EMPIRICAL (photos): the nose front slopes down low toward the player
CAP_TAPER = 0.12  # EMPIRICAL: block top is about 0.88 of its base width (photo, front view)
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
    """Drawbar cap, one loft from back to front over a full-width base: tall rear block with
    tapered sides and a sloped front, then the lower nose (about half the height) whose top
    face sits left because its right wall slopes, ending in a chamfer (reference, photos)."""
    ht, hn = CAP_HT, CAP_HN
    blk = (-CAP_W / 2 * (1 - CAP_TAPER), CAP_W / 2 * (1 - CAP_TAPER))
    side = CAP_W / 2 * (1 - CAP_TAPER * NOSE_FRONT_H / CAP_HT)  # same wall slope as the block
    base = (-side, side)
    secs = [
        (CAP_BACK, ht * 0.86, blk),
        (CAP_BACK - 0.08, ht, blk),
        (BLOCK_TOP_FRONT, ht, blk),
        (BLOCK_FRONT, hn, NOSE_TOP_X, NOSE_LEFT),
        (NOSE_TOP_FRONT, hn, NOSE_TOP_X, NOSE_LEFT),
        (CAP_FRONT, NOSE_FRONT_H, base, NOSE_LEFT),  # chamfered front, no separate band
    ]
    rings = [_section(y, z, tx, *lx) for y, z, tx, *lx in secs]
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
