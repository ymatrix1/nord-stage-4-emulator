"""Original geometry for the B7 proof (runs inside Blender): knob cap, arc ring, fader caps, plates.

ORIGINAL ART (D16): every mesh is authored here from primitives. The owner's reference images
were used only to read proportions (see assets/panel/theme_samples.json "proportions").
Units: 64 px per Blender unit at 1x. Z is up, the viewer is at -Y.
"""

from __future__ import annotations

import math

import bmesh
import bpy

# ASSUMED unless noted. EMPIRICAL ratios come from theme_samples.json "proportions".
KNOB_R = 0.40  # cap radius
KNOB_H = (
    0.54 * 2 * KNOB_R
)  # EMPIRICAL: height / diameter, 0.6 read on the reference, trimmed for the lower tilt
KNOB_FLUTES = 20  # EMPIRICAL: counted on the Master Level close-up
KNOB_POINTER_W = 0.14 * 2 * KNOB_R  # EMPIRICAL (0.11 measured on the flat top, wider on the bevel)
RING_OUTER = 1.25 * KNOB_R  # EMPIRICAL
RING_T0, RING_T1 = 0.07 * 2 * KNOB_R, 0.20 * 2 * KNOB_R  # EMPIRICAL: thin start, thick end
RING_SWEEP = 270.0
RING_Z = (
    0.8 * KNOB_H
)  # layer pivot: lifts the ring so it centres on the cap top, as on the reference

FADER_W = 0.62  # EMPIRICAL: 0.85 x knob diameter, rounded
SLOT_W = 0.18 * FADER_W  # EMPIRICAL
SLOT_LEN, SLOT_DEPTH, PLATE_T = 3.2, 0.16, 0.2
CAP_LIFT = 0.03  # cap floats above the plate so the slot shows under it


def link(obj):
    bpy.context.scene.collection.objects.link(obj)
    return obj


def mesh_obj(name, bm, mat, smooth=False, bevel=0.0, segments=3):
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
        mod.width, mod.segments, mod.limit_method = bevel, segments, "ANGLE"
        mod.angle_limit = math.radians(25)
    return obj


def box(name, size, center, mat, bevel=0.0):
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    bmesh.ops.scale(bm, vec=size, verts=bm.verts)
    bmesh.ops.translate(bm, vec=center, verts=bm.verts)
    return mesh_obj(name, bm, mat, bevel=bevel)


def knob_cap(m):
    """Fluted knob: 20 flat facets split by narrow grooves, rounded shoulder, flat satin top,
    white pointer bar on the top that wraps down the side. Returns [cap, top, pointer parts]."""
    pitch = 2 * math.pi / KNOB_FLUTES
    # per pitch: facet start, facet end, groove floor start, groove floor end (fraction of pitch)
    offs = ((0.0, 0.0), (0.80, 0.0), (0.86, 1.0), (0.94, 1.0))  # (angle frac, in-groove)
    # (radius scale, z, groove depth as fraction of R): base, mid, shoulder start, shoulder end
    prof = (
        (1.0, 0.0, 0.05),
        (0.975, 0.75 * KNOB_H, 0.05),
        (0.93, 0.93 * KNOB_H, 0.03),
        (0.86, KNOB_H, 0.0),
    )
    bm = bmesh.new()
    rings = []
    for rs, z, depth in prof:
        ring = []
        for k in range(KNOB_FLUTES):
            for f, groove in offs:
                a = (k + f) * pitch
                r = KNOB_R * rs * (1 - depth * groove)
                ring.append(bm.verts.new((r * math.cos(a), r * math.sin(a), z)))
        rings.append(ring)
    n = len(rings[0])
    for r0, r1 in zip(rings, rings[1:]):
        for i in range(n):
            j = (i + 1) % n
            bm.faces.new((r0[i], r0[j], r1[j], r1[i]))
    bm.faces.new(rings[0][::-1])
    cap = mesh_obj("knob_cap", bm, m["knob"], bevel=0.007, segments=2)
    # satin top disc, very slightly domed
    bm = bmesh.new()
    bmesh.ops.create_cone(
        bm, cap_ends=True, segments=96, radius1=KNOB_R * 0.865, radius2=KNOB_R * 0.80, depth=0.014
    )
    bmesh.ops.translate(bm, vec=(0, 0, KNOB_H + 0.004), verts=bm.verts)
    top = mesh_obj("knob_top", bm, m["knob_top"], smooth=True, bevel=0.004, segments=2)
    # pointer: bar on the top face, then a strip down the facet it sits on
    w = KNOB_POINTER_W
    bar = box(
        "ptr_top",
        (w, KNOB_R * 0.68, 0.014),
        (0, KNOB_R * 0.55, KNOB_H + 0.012),
        m["pointer"],
        0.004,
    )
    side = box(
        "ptr_side",
        (w, 0.016, KNOB_H * 0.5),
        (0, KNOB_R * 0.955, KNOB_H * 0.74),
        m["pointer"],
        0.004,
    )
    side.rotation_euler[0] = math.radians(-3)
    return [cap, top, bar, side], pitch


def knob_ring(m):
    """White arc indicator ring on the panel: 270 deg sweep, thin start, thick end, flat."""
    bm = bmesh.new()
    steps = 96
    outer, inner = [], []
    for i in range(steps + 1):
        u = i / steps
        a = math.radians(
            90 + RING_SWEEP / 2 - RING_SWEEP * u
        )  # -135 deg (lower left) first, clockwise
        t = RING_T0 + (RING_T1 - RING_T0) * u
        outer.append(bm.verts.new((RING_OUTER * math.cos(a), RING_OUTER * math.sin(a), RING_Z)))
        r = RING_OUTER - t
        inner.append(bm.verts.new((r * math.cos(a), r * math.sin(a), RING_Z)))
    for i in range(steps):
        bm.faces.new((outer[i], outer[i + 1], inner[i + 1], inner[i]))
    return [mesh_obj("knob_ring", bm, m["ring"], smooth=True)]


def fader_cap(m, key):
    """Chunky stepped block like the real drawbar cap: rounded rear shoulder, long flat top, a
    bevel ledge, a lower nose block set forward and shifted left. No grip lines (the reference
    caps have none). `key` picks the material (cap_black / cap_white / cap_grey)."""
    y0, y1, ht, hn = -0.34, 0.45, 0.60, 0.30
    prof = [
        (y0, hn),
        (y0, ht - 0.04),
        (y0 + 0.04, ht),
        (y1 - 0.06, ht),
        (y1, ht - 0.07),
        (y1, 0.0),
        (y0 + 0.0, 0.0),
    ]
    bm = bmesh.new()
    lo = [bm.verts.new((-FADER_W / 2, y, z + CAP_LIFT)) for y, z in prof]
    hi = [bm.verts.new((FADER_W / 2, y, z + CAP_LIFT)) for y, z in prof]
    bm.faces.new(lo[::-1])
    bm.faces.new(hi)
    for i in range(len(prof)):
        j = (i + 1) % len(prof)
        bm.faces.new((lo[i], lo[j], hi[j], hi[i]))
    body = mesh_obj("fader_body", bm, m[key], bevel=0.03, segments=4)
    nose = box("fader_nose", (0.56, 0.20, hn), (-0.09, y0 - 0.08, CAP_LIFT + hn / 2), m[key], 0.03)
    return [body, nose]


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


def plate_red(m):
    return [box("plate_red", (2.8, 2.8, PLATE_T), (0, 0, -PLATE_T / 2), m["red"])]
