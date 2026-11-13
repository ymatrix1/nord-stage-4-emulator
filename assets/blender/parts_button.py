"""Pill push-button (up state) in its panel well for the B7 proof (runs inside Blender).

Original art (D16): a rounded rectangular base with a raised stadium-shaped top, sitting in a
black well. Sizes measured on the owner's private reference (1 u = 129 reference px): the base
is 135 x 64 px, the pill 98 x 46 px, the well about 2 px larger than the base.
"""

from __future__ import annotations

from proof_geom import box, slab, stadium_points

BTN_W, BTN_H = 1.05, 0.50  # EMPIRICAL: base 135 x 64 reference px
PILL_W, PILL_H = 0.76, 0.36  # EMPIRICAL: pill 98 x 46 reference px
PILL_DX = -0.03  # EMPIRICAL: pill sits about 4 px left of the base centre
WELL_MARGIN = 0.02  # EMPIRICAL: well shows about 2 px around the base
BASE_Z, PILL_Z = 0.06, 0.15  # ASSUMED: heights; only the lean and the shading show them
CORNER = 0.035  # EMPIRICAL: small base corner radius


def rounded_rect(w, h, r, n=4):
    """Counter-clockwise outline of a w x h rectangle with corner radius r."""
    import math

    pts = []
    for cx, cy, a0 in (
        (w / 2 - r, -h / 2 + r, -90),
        (w / 2 - r, h / 2 - r, 0),
        (-w / 2 + r, h / 2 - r, 90),
        (-w / 2 + r, -h / 2 + r, 180),
    ):
        for i in range(n + 1):
            a = math.radians(a0 + 90 * i / n)
            pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    return pts


def button(m, base_key, pill_key):
    well = box(
        "btn_well",
        (BTN_W + 2 * WELL_MARGIN, BTN_H + 2 * WELL_MARGIN, 0.02),
        (0, 0, -0.01),
        m["well"],
    )
    base = slab(
        "btn_base",
        rounded_rect(BTN_W, BTN_H, CORNER),
        -0.02,
        BASE_Z,
        m[base_key],
        bevel=0.012,
        taper=0.012,
    )
    pill = slab(
        "btn_pill",
        stadium_points(PILL_DX, 0.0, PILL_W, PILL_H, n=20),
        BASE_Z - 0.01,
        PILL_Z,
        m[pill_key],
        bevel=0.03,
        taper=0.012,
        segments=6,
    )
    return [well, base, pill]
