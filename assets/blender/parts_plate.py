"""Flat plate tiles for the B7 proof (runs inside Blender): red leather, slate, silver header.

Each tile is a flat box larger than its frame, so the frame shows only its top face; the packer
(`tools/panel_assets.py`) makes the render seamless. Materials are procedural (D16).
"""

from __future__ import annotations

from proof_geom import box

PLATE_T = 0.2


def plate(m, key: str):
    return [box(f"plate_{key}", (4.0, 4.0, PLATE_T), (0, 0, -PLATE_T / 2), m[key])]
