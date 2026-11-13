"""Geometry helpers shared by the B7 part modules (runs inside Blender).

ORIGINAL ART (D16): every mesh is authored from primitives. Units: 64 px per Blender unit at 1x.
Z is up, the viewer is at -Y, screen-up is +Y.
"""

from __future__ import annotations

import math

import bmesh
import bpy


def link(obj):
    bpy.context.scene.collection.objects.link(obj)
    return obj


def mesh_obj(name, bm, mat, smooth=False, bevel=0.0, segments=3, angle=25.0):
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
        mod.angle_limit = math.radians(angle)
    return obj


def box(name, size, center, mat, bevel=0.0, segments=3):
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    bmesh.ops.scale(bm, vec=size, verts=bm.verts)
    bmesh.ops.translate(bm, vec=center, verts=bm.verts)
    return mesh_obj(name, bm, mat, bevel=bevel, segments=segments)


def stadium_points(cx, cy, w, h, n=16):
    """Outline of a stadium (pill) of size w x h centred on (cx, cy), counter-clockwise."""
    r = h / 2
    half = w / 2 - r
    pts = []
    for i in range(n + 1):  # right cap, bottom to top
        a = -math.pi / 2 + math.pi * i / n
        pts.append((cx + half + r * math.cos(a), cy + r * math.sin(a)))
    for i in range(n + 1):  # left cap, top to bottom
        a = math.pi / 2 + math.pi * i / n
        pts.append((cx - half + r * math.cos(a), cy + r * math.sin(a)))
    return pts


def loft(name, rings, mat, smooth=True, bevel=0.0, cap_top=True, cap_bottom=True, segments=3):
    """Skin a list of closed rings (each a list of (x, y, z)) into one mesh."""
    bm = bmesh.new()
    vr = [[bm.verts.new(p) for p in ring] for ring in rings]
    n = len(vr[0])
    for r0, r1 in zip(vr, vr[1:]):
        for i in range(n):
            j = (i + 1) % n
            bm.faces.new((r0[i], r0[j], r1[j], r1[i]))
    if cap_bottom:
        bm.faces.new(vr[0][::-1])
    if cap_top:
        bm.faces.new(vr[-1])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return mesh_obj(name, bm, mat, smooth=smooth, bevel=bevel, segments=segments)


def slab(name, outline, z0, z1, mat, bevel=0.0, taper=0.0, smooth=False, segments=3):
    """Extrude a closed 2D outline between z0 and z1; `taper` shrinks the top outline inward
    (a sloped side wall). Outline is a counter-clockwise list of (x, y)."""
    cx = sum(p[0] for p in outline) / len(outline)
    cy = sum(p[1] for p in outline) / len(outline)

    def ring(z, k):
        return [(cx + (x - cx) * k, cy + (y - cy) * k, z) for x, y in outline]

    sx = max(abs(p[0] - cx) for p in outline)
    k_top = 1.0 - taper / sx if taper else 1.0
    return loft(name, [ring(z0, 1.0), ring(z1, k_top)], mat, smooth, bevel, segments=segments)
