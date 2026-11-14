"""Knob geometry for the B7 proof (runs inside Blender): fluted cap, bent pointer, arc ring and
the numbered scale arc. Original art (D16); proportions are numbers measured on the owner's
private reference (assets/panel/theme_samples.json), no pixels or geometry copied.

Units: 64 px per Blender unit at 1x. The reference knob is 103 px across its base on a 1961 px
wide render; the proof keeps the cap base diameter at 0.8 u, so 1 u = 129 reference px.
"""

from __future__ import annotations

import math

import bmesh
import bpy
from proof_geom import link, mesh_obj

REF_PX_PER_U = 129.0  # EMPIRICAL: 103.4 reference px base diameter / 0.8 u
KNOB_R = 0.40  # ASSUMED: cap base radius, the unit of scale for every other part
# EMPIRICAL: wall slope and shoulder fitted by eye to the Master Level knob at the same feature
# size; (radius / R, height / H): base, base round, wall, shoulder (3 steps), top rim.
PROFILE = (
    (1.0, 0.0),
    (0.99, 0.06),
    (0.94, 0.5),
    (0.89, 0.80),
    (0.83, 0.93),
    (0.77, 0.99),
    (0.74, 1.0),
)
KNOB_H = 0.40  # ASSUMED: 0.5 of the base diameter (0.6 read on the lean-inflated silhouette)
TOP_R = PROFILE[-1][0] * KNOB_R
DOME = 0.02  # ASSUMED: top face rises this much at the centre; gives the soft top highlight
FLUTES = 20  # EMPIRICAL: counted on the Master Level close-up
# EMPIRICAL: groove depth (fraction of R) per PROFILE ring; the reference grooves run up to the
# rim and notch it, so they only thin out over the shoulder.
GROOVE_DEPTH = (0.025, 0.025, 0.025, 0.022, 0.018, 0.012, 0.008)
POINTER_W = 0.11 * 2 * KNOB_R  # EMPIRICAL: 0.11 of the base diameter, measured on the flat top
POINTER_IN = 0.03  # EMPIRICAL: pointer starts almost at the centre of the top face
POINTER_LIFT = 0.006
# EMPIRICAL: ring fitted to the Master Level close-up (circle fit to its outer edge): outer
# radius 67.7 px, thickness growing about linearly from 3 px to 15 px over the sweep.
RING_OUTER = 67.7 / REF_PX_PER_U
RING_T0, RING_T1 = 3.0 / REF_PX_PER_U, 15.0 / REF_PX_PER_U
RING_SWEEP = 270.0
RING_H = 0.012  # ASSUMED: printed inlay thickness
SCALE_R = 0.56  # EMPIRICAL: DRIVE numeral circle, about 72 reference px from the knob centre
SCALE_TEXT = 0.16  # EMPIRICAL: numeral size; "10" is about 22 reference px wide
SCALE_STEPS = 11  # 0..10 over the 270 degree sweep


def _dome_z(rho: float) -> float:
    return KNOB_H + DOME * (1 - (rho / TOP_R) ** 2)


def _flute_body(m):
    pitch = 2 * math.pi / FLUTES
    offs = ((0.0, 0.0), (0.84, 0.0), (0.88, 1.0), (0.95, 1.0))  # (pitch fraction, in-groove)
    bm = bmesh.new()
    rings = []
    for (rs, hz), depth in zip(PROFILE, GROOVE_DEPTH):
        ring = []
        for k in range(FLUTES):
            for f, groove in offs:
                a = (k + f) * pitch
                r = KNOB_R * rs * (1 - depth * groove)
                ring.append(bm.verts.new((r * math.cos(a), r * math.sin(a), hz * KNOB_H)))
        rings.append(ring)
    n = len(rings[0])
    for r0, r1 in zip(rings, rings[1:]):
        for i in range(n):
            j = (i + 1) % n
            bm.faces.new((r0[i], r0[j], r1[j], r1[i]))
    bm.faces.new(rings[0][::-1])
    return mesh_obj("knob_cap", bm, m["knob"], bevel=0.006, segments=2), pitch


def _dome_top(m):
    bm = bmesh.new()
    seg, rads = 96, 8
    centre = bm.verts.new((0, 0, _dome_z(0.0)))
    prev = None
    for ri in range(1, rads + 1):
        rho = TOP_R * ri / rads
        ring = [
            bm.verts.new((rho * math.cos(a), rho * math.sin(a), _dome_z(rho)))
            for a in (2 * math.pi * s / seg for s in range(seg))
        ]
        for s in range(seg):
            t = (s + 1) % seg
            if prev is None:
                bm.faces.new((centre, ring[s], ring[t]))
            else:
                bm.faces.new((prev[s], ring[s], ring[t], prev[t]))
        prev = ring
    return mesh_obj("knob_top", bm, m["knob_top"], smooth=True)


def knob_cap(m):
    """Return (parts turned with a flute offset, parts turned with the pointer, flute pitch)."""
    cap, pitch = _flute_body(m)
    return [cap, _dome_top(m)], [pointer(m)], pitch


def pointer_path() -> list[tuple[float, float]]:
    """(radius, z) of the pointer centre line: across the domed top, over the shoulder and part
    nearly down to the base, as on the reference."""
    top = [TOP_R * (POINTER_IN + (1 - POINTER_IN) * i / 6) for i in range(7)]
    wall = [(rs * KNOB_R, hz * KNOB_H) for rs, hz in PROFILE[::-1]][1:5]
    return [(r, _dome_z(r)) for r in top] + wall + [(0.975 * KNOB_R, 0.16 * KNOB_H)]


def pointer(m):
    """Tapered bar on the top face that bends over the shoulder and down the side wall."""
    path = pointer_path()
    bm = bmesh.new()
    left, right = [], []
    for i, (rho, z) in enumerate(path):
        w = POINTER_W * (1.0 - 0.15 * i / (len(path) - 1)) / 2  # slight taper outward
        a, b = path[max(i - 1, 0)], path[min(i + 1, len(path) - 1)]
        tx, tz = b[0] - a[0], b[1] - a[1]
        ln = math.hypot(tx, tz) or 1.0
        nx, nz = tz / ln, -tx / ln
        if nz < 0 or (nz == 0 and nx < 0):
            nx, nz = -nx, -nz
        px, pz = rho + nx * POINTER_LIFT, z + nz * POINTER_LIFT
        left.append(bm.verts.new((-w, px, pz)))
        right.append(bm.verts.new((w, px, pz)))
    for i in range(len(path) - 1):
        bm.faces.new((left[i], right[i], right[i + 1], left[i + 1]))
    obj = mesh_obj("knob_pointer", bm, m["pointer"], smooth=True)
    sol = obj.modifiers.new("solid", "SOLIDIFY")
    sol.thickness, sol.offset = 0.012, -1.0
    bev = obj.modifiers.new("bevel", "BEVEL")
    bev.width, bev.segments = 0.003, 2
    return obj


def knob_ring(m):
    """White arc on the panel: 270 deg sweep from lower left clockwise, thin start, thick end."""
    bm = bmesh.new()
    steps = 120
    outer, inner = [], []
    for i in range(steps + 1):
        u = i / steps
        a = math.radians(90 + RING_SWEEP / 2 - RING_SWEEP * u)  # -135 deg first, clockwise
        t = RING_T0 + (RING_T1 - RING_T0) * u
        for ring, r in ((outer, RING_OUTER), (inner, RING_OUTER - t)):
            ring.append(bm.verts.new((r * math.cos(a), r * math.sin(a), RING_H)))
    for i in range(steps):
        bm.faces.new((outer[i], inner[i], inner[i + 1], outer[i + 1]))
    obj = mesh_obj("knob_ring", bm, m["ring"], smooth=True)
    sol = obj.modifiers.new("solid", "SOLIDIFY")
    sol.thickness, sol.offset = RING_H, -1.0
    return [obj]


def knob_scale(m):
    """Numerals 0..10 along the 270 deg arc, a separate layer for the DRIVE style knob. Uses
    Blender's built-in font (freely licensed), upright like the reference, emboldened by a
    curve offset."""
    objs = []
    for i in range(SCALE_STEPS):
        ang = math.radians(90 + RING_SWEEP / 2 - RING_SWEEP * i / (SCALE_STEPS - 1))
        cu = bpy.data.curves.new(f"num{i}", "FONT")
        cu.body, cu.size = str(i), SCALE_TEXT
        cu.align_x, cu.align_y = "CENTER", "CENTER"
        cu.extrude, cu.offset = 0.003, 0.008
        ob = link(bpy.data.objects.new(f"num{i}", cu))
        ob.location = (SCALE_R * math.cos(ang), SCALE_R * math.sin(ang), 0.003)
        ob.data.materials.append(m["ring"])
        objs.append(ob)
    return objs
