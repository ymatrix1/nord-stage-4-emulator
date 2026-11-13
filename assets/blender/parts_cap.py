"""Drawbar / layer-fader cap and its slotted plate for the B7 proof (runs inside Blender).

Original art (D16): authored from two extruded side profiles. Sizes are numbers measured on the
owner's private reference (1 u = 129 reference px, see parts_knob.REF_PX_PER_U).
"""

from __future__ import annotations

import bmesh
import bpy
from proof_geom import box, loft, mesh_obj

# EMPIRICAL: white drawbar cap on the reference is 100 px wide and 231 px long; the lower nose
# block is narrower (85 px), shifted left, and ends in a chamfer that reads as a 30 px band.
CAP_W = 0.78
CAP_BACK, CAP_FRONT = 0.90, -0.89
# ASSUMED: top block, nose and front skirt heights (only the lean and shading show them).
CAP_HT, CAP_HN, CAP_HS = 0.50, 0.30, 0.04
NOSE_X = (-0.45, 0.16)  # EMPIRICAL: nose left / right edge (black 16' cap, 4x zoom read)
CAP_TAPER = 0.05  # EMPIRICAL: side walls lean in by this fraction of the width at the top
CAP_BEVEL = 0.07  # EMPIRICAL: soft rounded edges, larger than v2's 0.03
CAP_LIFT = 0.03  # cap floats above the plate so the slot shows under it
# EMPIRICAL: grey layer-fader cap on the reference is 96 px wide and 74 px long.
LAYER_W, LAYER_L = 0.75, 0.57
LAYER_H = 0.34  # ASSUMED
LAYER_BEVEL = 0.09  # EMPIRICAL: the layer cap reads as a rounder, pillowy block
# EMPIRICAL: slot is about 22 px wide on the reference, 0.22 of the cap width.
SLOT_W, SLOT_LEN, SLOT_DEPTH, PLATE_T = 0.17, 3.4, 0.16, 0.2


def _extrude_profile(name, prof, x0, x1, mat, z_top, bevel=CAP_BEVEL):
    """Extrude a (y, z) side profile between x0 and x1, then lean the side walls inward with
    height (the sloped wall of the reference caps)."""
    bm = bmesh.new()
    lo = [bm.verts.new((x0, y, z)) for y, z in prof]
    hi = [bm.verts.new((x1, y, z)) for y, z in prof]
    bm.faces.new(lo[::-1])
    bm.faces.new(hi)
    n = len(prof)
    for i in range(n):
        j = (i + 1) % n
        bm.faces.new((lo[i], lo[j], hi[j], hi[i]))
    cx, w = (x0 + x1) / 2, x1 - x0
    for v in bm.verts:
        k = CAP_TAPER * (v.co.z / z_top) * w / 2
        v.co.x -= k if v.co.x > cx else -k
        v.co.z += CAP_LIFT
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return mesh_obj(name, bm, mat, bevel=bevel, segments=8, angle=20)


def _section(y, x0, x1, z):
    """Cross-section ring at y: bottom edge, then the top edge pulled in by the wall slope."""
    k = CAP_TAPER * (x1 - x0) / 2 * min(1.0, z / CAP_HT)
    zl = CAP_LIFT
    return [(x0, y, zl), (x1, y, zl), (x1 - k, y, zl + z), (x0 + k, y, zl + z)]


def fader_cap(m, key):
    """Drawbar cap, lofted from back to front: rounded back chamfer, long flat top, a slope that
    narrows into a lower nose shifted left, then the front skirt at the foot (shadowed by the
    nose). `key` picks cap_black / cap_white."""
    ht, hn, hs = CAP_HT, CAP_HN, CAP_HS
    half = CAP_W / 2
    nx0, nx1 = NOSE_X
    sections = [
        (CAP_BACK, -half, half, ht - 0.12),
        (CAP_BACK - 0.20, -half, half, ht),
        (-0.10, -half, half, ht),
        (-0.38, nx0, nx1, hn),
        (-0.66, nx0, nx1, hn),
        (-0.74, nx0, nx1, 0.06),
    ]
    body = loft(
        "cap_body",
        [_section(*s) for s in sections],
        m[key],
        smooth=False,
        bevel=CAP_BEVEL,
        segments=8,
    )
    skirt = _extrude_profile(
        "cap_skirt",
        [(-0.60, 0.0), (-0.60, hs + 0.05), (-0.70, hs + 0.05), (CAP_FRONT, 0.0)],
        -half + 0.01,
        half - 0.04,
        m[key],
        hs,
    )
    return [body, skirt]


def layer_cap(m, key):
    """Short layer-fader cap: a small back chamfer, a short flat top and a long front slope (the
    reference shows the slope as the largest face)."""
    ht, y0, y1 = LAYER_H, -LAYER_L / 2, LAYER_L / 2
    prof = [(y1, 0.0), (y1, ht - 0.08), (y1 - 0.08, ht), (0.05, ht), (y0 + 0.03, 0.10), (y0, 0.0)]
    return [_extrude_profile("layer_cap", prof, -LAYER_W / 2, LAYER_W / 2, m[key], ht, LAYER_BEVEL)]


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
