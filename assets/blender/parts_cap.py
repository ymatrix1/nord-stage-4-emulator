"""Drawbar / layer-fader cap and its slotted plate for the B7 proof (runs inside Blender).

Original art (D16): geometry authored from primitives (lofted sections); shapes follow the
owner's photos of the real parts. Sizes are numbers measured on the owner's private reference (1 u = 129 reference px, see parts_knob.REF_PX_PER_U).
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
CAP_HT, CAP_HN = 0.50, 0.26  # ASSUMED heights (photo ratio about 2 : 1)
BLOCK_FRONT = -0.18  # base of the block's sloped front face
BLOCK_TOP_FRONT = 0.02  # top edge of that face
NOSE_TOP_FRONT, NOSE_CHAMFER_Z = -0.66, 0.08  # front chamfer of the nose
# EMPIRICAL (reference, every drawbar): the nose is narrower than the block and offset left
NOSE_X = (-0.42, 0.18)
FOOT_H = 0.12  # ASSUMED: low base under the nose, full width
CAP_TAPER = 0.18  # EMPIRICAL: block top is about 0.82 of its base width (photo, front view)
CAP_BEVEL = 0.07
CAP_LIFT = 0.03  # cap floats above the plate so the slot shows under it
# Layer-fader cap EMPIRICAL from its photo: a frustum, top smaller than the base on all sides,
# rounded edges. Base 96 x 74 reference px.
LAYER_W, LAYER_L, LAYER_H = 0.74, 0.58, 0.36
LAYER_TOP = (0.82, 0.50)  # top width / length as a fraction of the base
LAYER_BEVEL = 0.07
# EMPIRICAL: slot is about 22 px wide on the reference, 0.22 of the cap width.
SLOT_W, SLOT_LEN, SLOT_DEPTH, PLATE_T = 0.17, 3.4, 0.16, 0.2


def _section(y, z, top_frac, x0=-CAP_W / 2, x1=CAP_W / 2):
    """Trapezoid cross-section at y between x0 and x1, `top_frac` of that width on top."""
    c, hb = (x0 + x1) / 2, (x1 - x0) / 2
    ht, zl = hb * top_frac, CAP_LIFT
    return [(c - hb, y, zl), (c + hb, y, zl), (c + ht, y, zl + z), (c - ht, y, zl + z)]


def _frac(z):
    return 1.0 - CAP_TAPER * z / CAP_HT


def fader_cap(m, key):
    """Drawbar cap: flange, tall rear block with sloped sides and front, long lower nose with
    a chamfered front, lofted from back to front. `key` picks the material."""
    ht, hn = CAP_HT, CAP_HN
    w = (-CAP_W / 2, CAP_W / 2)
    secs = [
        (CAP_BACK, ht * 0.86, *w),
        (CAP_BACK - 0.08, ht, *w),
        (BLOCK_TOP_FRONT, ht, *w),
        (BLOCK_FRONT, hn, *NOSE_X),
        (NOSE_TOP_FRONT, hn, *NOSE_X),
        (CAP_FRONT, NOSE_CHAMFER_Z, *NOSE_X),
    ]
    rings = [_section(y, z, _frac(z), x0, x1) for y, z, x0, x1 in secs]
    body = loft("cap_body", rings, m[key], smooth=False, bevel=CAP_BEVEL, segments=8)
    # the nose sits on a low full-width foot that ends with the nose (seen right of the nose)
    foot = loft(
        "cap_foot",
        [_section(y, FOOT_H, 0.97) for y in (BLOCK_FRONT + 0.1, CAP_FRONT)],
        m[key],
        smooth=False,
        bevel=0.02,
        segments=3,
    )
    return [body, foot]


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
