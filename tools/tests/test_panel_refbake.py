"""Tests for tools/panel_refbake.py on synthetic images (the private reference is not needed)."""

import math

import numpy as np
import panel_assets as P
import panel_refbake as R
import pytest
from PIL import Image, ImageDraw


def test_at_scale_maps_reference_pixels_to_our_grid():
    img = Image.new("RGB", (129, 258))
    assert R.at_scale(img, 2).size == (128, 256)
    assert R.at_scale(img, 1).size == (64, 128)


def test_clean_mask_rejects_far_colours_and_structure():
    a = np.full((40, 40, 3), (130.0, 27.0, 38.0))
    a[5:8, 5:30] = (250, 250, 250)  # a white text stroke
    m = R.clean_mask(a, (130, 27, 38), 30)
    assert m[30, 30] and not m[6, 10]


def test_clean_windows_and_quilt_shape():
    mask = np.zeros((50, 50), bool)
    mask[10:40, 10:40] = True
    wins = R.clean_windows(mask, 8)
    assert wins and all(10 <= x <= 32 and 10 <= y <= 32 for x, y in wins)
    rng = np.random.default_rng(1)
    a = rng.uniform(90, 110, size=(50, 50, 3))
    q = R.quilt(a, wins, 32, 8)
    assert q.shape == (32, 32, 3) and 85 < q.mean() < 115
    with pytest.raises(ValueError):
        R.quilt(a, [], 32, 8)


def test_plate_tile_is_seamless_and_on_target():
    rng = np.random.default_rng(2)
    a = np.clip(rng.normal((130, 27, 38), 6, size=(300, 300, 3)), 0, 255).astype(np.uint8)
    tile = R.plate_tile(Image.fromarray(a, "RGB"), (130, 27, 38), 40)
    assert tile.size == (R.TILE_2X, R.TILE_2X)
    mean = np.asarray(tile.convert("RGB"), dtype=float).mean(axis=(0, 1))
    assert np.allclose(mean, (130, 27, 38), atol=4)


def _matrix() -> tuple[Image.Image, dict]:
    """2 x 1 grid on dark grey: left cell has a grey triangle, right cell a lit red one."""
    img = Image.new("RGB", (40, 20), (40, 40, 40))
    d = ImageDraw.Draw(img)
    d.polygon([(4, 10), (14, 4), (14, 16)], fill=(120, 120, 120))
    d.polygon([(24, 10), (34, 4), (34, 16)], fill=(230, 40, 40))
    spec = {"box": [0, 0, 40, 20], "grid": [2, 1], "lit": [1, 0], "unlit_src": [0, 0]}
    return img, spec


def test_triangle_layers_unlight_the_base_and_keep_the_glow():
    img, spec = _matrix()
    base, lit = R.triangle_layers(img, spec)
    assert base.getpixel((30, 10))[:3] == (120, 120, 120)  # lit cell now shows the unlit one
    assert lit.getpixel((10, 10))[3] > 200 and lit.getpixel((1, 1))[3] == 0


def test_cell_box_splits_evenly():
    assert R.cell_box([0, 0, 30, 20], [3, 2], 2, 1) == (20, 10, 30, 20)


def test_hulls():
    hull = R.convex_hull(np.array([[0, 0], [4, 0], [4, 4], [0, 4], [2, 2]]))
    assert len(hull) == 4 and (2, 2) not in hull
    pts = np.array(R.circle_hull((0, 0), 10, (-5, 0), 8))
    assert pts[:, 0].min() == pytest.approx(-13, abs=0.3) and pts[:, 0].max() == pytest.approx(
        10, abs=0.3
    )


def test_inpaint_rotate_fills_from_rotated_sources():
    a = np.zeros((41, 41, 3))
    yy, xx = np.mgrid[:41, :41]
    a[..., 0] = np.hypot(xx - 20, yy - 20)  # radial ramp: rotation invariant
    mask = np.zeros((41, 41), bool)
    mask[18:23, 25:35] = True
    a[mask] = 255
    out = R.inpaint_rotate(a, mask, (20, 20), np.ones((41, 41), bool))
    assert np.abs(out[mask, 0] - np.hypot(xx - 20, yy - 20)[mask]).max() < 1.6


def _knob_ref() -> tuple[Image.Image, tuple[float, float]]:
    """Red plate with a dark cap (two circles), a light pointer bar and white numerals."""
    img = Image.new("RGB", (400, 400), (130, 27, 38))
    d = ImageDraw.Draw(img)
    c = (200, 200)
    d.ellipse((c[0] - 51, c[1] - 51, c[0] + 51, c[1] + 51), fill=(50, 46, 44))
    d.ellipse((c[0] - 70, c[1] - 52, c[0] + 20, c[1] + 38), fill=(60, 56, 52))
    d.rectangle((c[0] - 40, c[1] - 4, c[0] - 5, c[1] + 4), fill=(210, 210, 208))
    for i in range(11):
        t = math.radians(-135 + 27 * i)
        x, y = c[0] + 72 * math.sin(t), c[1] - 72 * math.cos(t)
        d.rectangle((x - 3, y - 5, x + 3, y + 5), fill=(240, 238, 236))
    return img, c


def test_knob_body_removes_pointer_and_plate():
    img, c = _knob_ref()
    body = R.knob_body(img, c, (-25, -7))
    assert body.size == (R.KNOB_2X, R.KNOB_2X)
    a = np.asarray(body, dtype=float)
    assert a[2, 2, 3] == 0  # plate corner is transparent
    m = R.KNOB_2X // 2
    px = a[m, m - 20]  # where the pointer was
    assert px[3] > 200 and px[:3].min() < 90


def test_knob_scale_keeps_numerals_only():
    img, c = _knob_ref()
    body = R.knob_body(img, c, (-25, -7))
    scale = R.knob_scale(img, c, np.asarray(body.getchannel("A"), dtype=float) / 255)
    alpha = np.asarray(scale.getchannel("A"))
    assert alpha.max() > 200  # numerals present
    m = R.KNOB_2X // 2
    assert alpha[m, m] == 0 and alpha[2, 2] == 0  # no knob, no corner


def test_bake_writes_every_reference_sprite(tmp_path):
    img, c = _knob_ref()
    big = Image.new("RGB", (900, 600), (55, 64, 76))
    big.paste(img, (0, 0))
    m, spec = _matrix()
    big.paste(m, (500, 50))
    crops = {
        "ref_px_per_unit": 129.0,
        "buttons": {"b1": [450, 300, 588, 377]},
        "triangles": {"t1": {**spec, "box": [500, 50, 540, 70], "dir": "left"}},
        "knobs": {"drive": {"centre": list(c), "top_offset": [-25, -7], "pointer_deg": -90}},
        "tiles": {"plate_slate": {"target": [55, 64, 76], "tol": 30}},
    }
    big.save(tmp_path / "ref.png")
    names = R.bake(tmp_path / "ref.png", tmp_path, crops)
    assert {"button_b1", "tri_t1", "tri_lit_left", "knob_body_drive", "knob_scale"} <= set(names)
    for n in names:
        one, two = Image.open(tmp_path / f"{n}_1x.png"), Image.open(tmp_path / f"{n}_2x.png")
        assert abs(two.width - 2 * one.width) <= 1, n


def test_committed_ref_sprites_exist_at_both_scales():
    for n in P.ref_sprites():
        for s in P.SCALES:
            assert (P.PROOF_DIR / f"{n}_{s}x.png").exists(), n


def test_main_usage():
    assert R.main([]) == 2
