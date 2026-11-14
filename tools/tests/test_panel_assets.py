"""Tests for tools/panel_assets.py and the shared constants of assets/blender/build_proof.py."""

import ast
import json
from pathlib import Path

import numpy as np
import panel_assets as P
import pytest
from PIL import Image


def test_knob_angle_endpoints_and_monotonic():
    assert P.knob_angle_deg(0) == -135.0
    assert P.knob_angle_deg(P.KNOB_FRAMES - 1) == 135.0
    angles = [P.knob_angle_deg(i) for i in range(P.KNOB_FRAMES)]
    assert angles == sorted(angles)


def test_knob_angle_rejects_out_of_range():
    with pytest.raises(ValueError):
        P.knob_angle_deg(64)


def test_grid_holds_every_frame():
    assert P.KNOB_GRID**2 == P.KNOB_FRAMES


def test_png_size_and_hash(tmp_path):
    f = tmp_path / "a.png"
    Image.new("RGBA", (5, 7)).save(f)
    assert P.png_size(f) == (5, 7)
    assert len(P.sha256(f)) == 64
    bad = tmp_path / "b.png"
    bad.write_bytes(b"nope" * 8)
    with pytest.raises(ValueError):
        P.png_size(bad)


def _fake_proof(tmp_path: Path, scale_edge: int = 4) -> Path:
    """Write every manifest asset as a tiny PNG with distinct content."""
    for k, name in enumerate(P.asset_names()):
        Image.new("RGBA", (scale_edge, scale_edge), (k, 0, 0, 255)).save(tmp_path / name)
    return tmp_path


def test_manifest_roundtrip_and_verify(tmp_path):
    proof = _fake_proof(tmp_path)
    manifest = P.build_manifest(proof)
    assert set(manifest["files"]) == set(P.asset_names())
    assert P.verify_manifest(manifest, proof) == []
    Image.new("RGBA", (4, 4), (255, 255, 255, 255)).save(proof / P.asset_names()[0])
    assert P.verify_manifest(manifest, proof) == [f"hash mismatch {P.asset_names()[0]}"]
    (proof / P.asset_names()[1]).unlink()
    assert "missing " + P.asset_names()[1] in P.verify_manifest(manifest, proof)


def test_stitch_filmstrip_is_deterministic(tmp_path):
    raw = tmp_path / "raw"
    edge = P.KNOB_FRAME_PX
    for i in range(P.KNOB_FRAMES):
        p = P.frame_path(raw, 1, i)
        p.parent.mkdir(parents=True, exist_ok=True)
        Image.new("RGBA", (edge, edge), (i, 0, 0, 255)).save(p)
    a, b = tmp_path / "a.png", tmp_path / "b.png"
    P.stitch_filmstrip(raw, 1, a)
    P.stitch_filmstrip(raw, 1, b)
    assert a.read_bytes() == b.read_bytes()
    sheet = Image.open(a)
    assert sheet.size == (edge * 8, edge * 8)
    assert sheet.getpixel((edge + 1, edge + 1))[0] == 9  # frame 9 = row 1, col 1


def test_stitch_rejects_wrong_frame_size(tmp_path):
    raw = tmp_path / "raw"
    for i in range(P.KNOB_FRAMES):
        p = P.frame_path(raw, 1, i)
        p.parent.mkdir(parents=True, exist_ok=True)
        Image.new("RGBA", (10, 10)).save(p)
    with pytest.raises(ValueError):
        P.stitch_filmstrip(raw, 1, tmp_path / "x.png")


def _wrap_jump(a: np.ndarray) -> float:
    """Mean step across the left/right and top/bottom wrap, relative to an inner step."""
    wrap = np.abs(a[:, 0] - a[:, -1]).mean() + np.abs(a[0] - a[-1]).mean()
    inner = np.abs(a[:, 1] - a[:, 0]).mean() + np.abs(a[1] - a[0]).mean()
    return wrap / max(inner, 1e-9)


def test_make_seamless_removes_the_wrap_seam():
    ramp = np.tile(np.linspace(0, 255, 64), (64, 1))
    rgb = np.stack([ramp, ramp.T, np.full_like(ramp, 90)], axis=-1).astype(np.uint8)
    img = Image.fromarray(rgb, "RGB")
    assert _wrap_jump(np.asarray(img, dtype=float)) > 20  # a ramp has a hard seam
    out = P.make_seamless(img)
    a = np.asarray(out.convert("RGB"), dtype=float)
    assert out.size == img.size and out.getchannel("A").getextrema() == (255, 255)
    assert _wrap_jump(a) < 3
    assert np.allclose(a.mean(axis=(0, 1)), rgb.mean(axis=(0, 1)), atol=3)


def test_make_seamless_keeps_noise_contrast():
    rng = np.random.default_rng(0)
    noise = rng.integers(60, 200, size=(64, 64, 3)).astype(np.uint8)
    out = np.asarray(P.make_seamless(Image.fromarray(noise, "RGB")).convert("RGB"), dtype=float)
    assert out.std() > 0.8 * noise.std()  # renormalised blend does not wash the grain out


def test_size_report_scales_with_resolution(tmp_path):
    rep = P.size_report(P.build_manifest(_fake_proof(tmp_path)))
    assert set(rep["per_scale"]) == {"1x", "2x"}
    for v in rep["per_scale"].values():
        assert v["full_panel_png_bytes"] > v["shared_sprite_set_png_bytes"] > 0
        assert v["full_panel_rgba_bytes"] > v["shared_sprite_set_rgba_bytes"]
        assert set(v["sprite_png_bytes"]) == {*P.FILMSTRIPS, *P.SPRITES, *P.ref_sprites()}
        assert v["button_styles"] == len([n for n in P.ref_sprites() if n.startswith("button_")])
        assert v["knob_styles"] >= len(P.FILMSTRIPS)


def test_lean_table_covers_every_sprite_class():
    assert P.CAM_HEIGHT_U > 5
    assert set(P.LEAN) >= {"knob", "drawbar", "layer", "plate"}
    assert {f"knob_{k}" for k in P.REF_KNOBS} <= set(P.LEAN)
    assert P.LEAN["plate"] == (0.0, 0.0)
    assert set(P.TILES) <= set(P.ref_sprites())
    assert not set(P.SPRITES) & set(P.ref_sprites())  # each sprite has one source


def test_committed_manifest_matches_files():
    """CI check: committed PNGs still match their recorded hashes (no Blender needed)."""
    manifest = json.loads(P.MANIFEST.read_text(encoding="utf-8"))
    assert manifest["version"] == 2
    assert set(manifest["files"]) == set(P.asset_names())
    assert P.verify_manifest(manifest) == []
    assert manifest["files"]["knob_filmstrip_2x.png"]["width"] == 2 * P.KNOB_FRAME_PX * P.KNOB_GRID
    for name, (w, h) in P.SPRITES.items():
        for s in P.SCALES:
            meta = manifest["files"][f"{name}_{s}x.png"]
            assert (meta["width"], meta["height"]) == (w * s, h * s), name


def _alpha(name: str):
    return Image.open(P.PROOF_DIR / name).convert("RGBA").getchannel("A")


def test_sprites_fit_their_frames():
    """Caps and layers have a transparent margin: nothing is clipped by the frame."""
    for name in P.SPRITES:
        if name == "plate_slate_slot":
            continue
        alpha = _alpha(f"{name}_2x.png")
        if "shadow" in name:  # catcher layer: faint everywhere, dark only near the cap
            w, h = alpha.size
            side = w // 3  # skip the middle third: the slot recess itself darkens the catcher
            boxes = [
                (0, 0, side, 2),
                (w - side, 0, w, 2),
                (0, h - 2, side, h),
                (w - side, h - 2, w, h),
            ]
            border = [alpha.crop(b).getextrema()[1] for b in boxes]
            assert max(border) < 48, (name, border)
            continue
        left, top, right, bottom = alpha.getbbox()
        w, h = (v * 2 for v in P.SPRITES[name])
        assert left > 1 and top > 1 and right < w - 1 and bottom < h - 1, name


def test_sprite_silhouettes_match_between_scales():
    """1x and 2x describe the same silhouette: the 2x bbox is twice the 1x bbox (+-3 px)."""
    for name in ("drawbar_cap_black", "drawbar_cap_white", "layer_cap_grey"):
        b1, b2 = (
            _alpha(f"{name}_{k}x.png").point(lambda v: 255 if v > 64 else 0).getbbox()
            for k in (1, 2)
        )
        assert all(abs(2 * a - b) <= 3 for a, b in zip(b1, b2)), name


def test_knob_filmstrip_is_transparent_and_cap_is_inside_frame():
    strip = Image.open(P.PROOF_DIR / "knob_filmstrip_1x.png").convert("RGBA")
    e = P.KNOB_FRAME_PX
    for frame in (0, 31, 63):
        cell = strip.crop(
            ((frame % 8) * e, (frame // 8) * e, (frame % 8 + 1) * e, (frame // 8 + 1) * e)
        )
        alpha = cell.getchannel("A")
        assert alpha.getextrema()[0] == 0  # plate not baked in
        assert alpha.crop((0, 0, e, 2)).getextrema()[1] < 16  # cap not clipped at the top
        assert alpha.crop((0, e - 2, e, e)).getextrema()[1] < 16  # or at the bottom


def test_committed_tiles_are_opaque_and_seamless():
    for name in P.TILES:
        img = Image.open(P.PROOF_DIR / f"{name}_2x.png").convert("RGBA")
        assert img.getchannel("A").getextrema() == (255, 255), name
        assert _wrap_jump(np.asarray(img.convert("L"), dtype=float)) < 1.6, name


def test_theme_samples_are_measured_data_with_sources():
    data = json.loads(P.THEME_SAMPLES.read_text(encoding="utf-8"))
    assert data["status"] == "MEASURED" and "never committed" in data["source"]
    for name, c in data["colors"].items():
        assert c["note"], name
        for key in ("median", "p10", "p90", "use"):
            assert len(c[key]) == 3 and all(0 <= v <= 255 for v in c[key]), (name, key)
    assert data["proportions"]["knob_flute_count"] > 0


# (asset at 2x, theme key, flat patch box, max channel error): flat faces the materials are
# calibrated on. Boxes are in 2x sprite pixels.
CALIBRATED = (
    ("plate_red_2x.png", "panel_red", (0, 0, 256, 256), 6),
    ("plate_slate_2x.png", "panel_slate", (0, 0, 256, 256), 6),
    ("plate_silver_2x.png", "header_silver", (0, 0, 256, 256), 6),
    ("plate_slate_slot_2x.png", "panel_slate_drawbars", (20, 20, 100, 100), 6),
    # projected caps carry the reference pixels of their own instance: inside p10..p90
    ("drawbar_cap_white_2x.png", "cap_white_top", (80, 135, 110, 170), "range"),
    ("drawbar_cap_black_2x.png", "cap_black_top", (80, 135, 110, 170), "range"),
    ("layer_cap_grey_2x.png", "cap_grey_top", (80, 80, 110, 100), "range"),
)


def test_committed_sprites_match_sampled_colours():
    """Flat faces of the rendered sprites land near the measured reference colours."""
    colors = json.loads(P.THEME_SAMPLES.read_text(encoding="utf-8"))["colors"]
    for asset, key, box, tol in CALIBRATED:
        px = np.asarray(Image.open(P.PROOF_DIR / asset).convert("RGBA").crop(box)).reshape(-1, 4)
        got, want = np.median(px[px[:, 3] > 250][:, :3], axis=0), colors[key]["use"]
        if tol == "range":
            lo, hi = np.asarray(colors[key]["p10"]) - 3, np.asarray(colors[key]["p90"]) + 3
            assert ((got >= lo) & (got <= hi)).all(), (asset, got, lo, hi)
        else:
            assert max(abs(got - want)) <= tol, (asset, got, want)


def test_main_usage_and_verify(capsys):
    assert P.main([]) == 2
    assert P.main(["verify"]) == 0
    assert "all hashes match" in capsys.readouterr().out


def test_blender_scripts_share_constants_parse_and_stay_small():
    blender = P.ROOT / "assets" / "blender"
    src = (blender / "build_proof.py").read_text(encoding="utf-8")
    assert "import panel_assets as P" in src
    assert "P.knob_angle_deg" in src and "P.KNOB_FRAMES" in src and "P.LEAN" in src
    files = sorted(blender.glob("*.py"))
    assert {f.name for f in files} >= {"build_proof.py", "proof_scene.py", "parts_knob.py"}
    assert "P.REF_KNOBS" in src
    for f in files:
        text = f.read_text(encoding="utf-8")
        ast.parse(text)  # bpy is unavailable outside Blender; syntax only
        assert len(text.splitlines()) < 400, f.name
        assert "ORIGINAL" in text or "Original art" in text or "(D16)" in text, f.name


def test_retexture_slot_keeps_slot_and_uses_tile_grain():
    render = Image.new("RGB", (40, 60), (90, 100, 110))
    render.paste((10, 10, 10), (17, 0, 23, 60))  # the slot
    rng = np.random.default_rng(3)
    tile = Image.fromarray(rng.integers(40, 70, size=(16, 16, 3)).astype(np.uint8), "RGB")
    out = np.asarray(P.retexture_slot(render, tile, (47, 57, 68)).convert("RGB"), dtype=float)
    assert out[30, 20].max() < 20  # slot kept
    plate = out[:, :12].reshape(-1, 3)
    assert np.allclose(plate.mean(axis=0), (47, 57, 68), atol=3)  # tile, shifted to target
    assert plate.std(axis=0).min() > 4  # grain, not the flat render
