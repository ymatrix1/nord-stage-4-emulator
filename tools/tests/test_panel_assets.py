"""Tests for tools/panel_assets.py and the shared constants of assets/blender/build_proof.py."""

import ast
import json
from pathlib import Path

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


def test_size_report_scales_with_resolution(tmp_path):
    manifest = P.build_manifest(_fake_proof(tmp_path))
    rep = P.size_report(manifest)
    assert set(rep["per_scale"]) == {"1x", "2x"}
    for v in rep["per_scale"].values():
        assert v["full_panel_png_bytes_excl_buttons"] > v["shared_sprite_set_png_bytes"] > 0


def test_committed_manifest_matches_files():
    """CI check: committed PNGs still match their recorded hashes (no Blender needed)."""
    manifest = json.loads(P.MANIFEST.read_text(encoding="utf-8"))
    assert P.verify_manifest(manifest) == []
    assert manifest["files"]["knob_filmstrip_2x.png"]["width"] == 2 * P.KNOB_FRAME_PX * P.KNOB_GRID


def test_compose_preview_size():
    sheet = P.compose_preview()
    e = P.KNOB_FRAME_PX * 2
    assert sheet.size == (e * 5 + 32, e * 2 + 8)


def test_composed_layers_are_opaque_and_ordered():
    """Plate + ring + knob frame and plate + shadow + cap must be fully opaque composites."""
    for img in (P.composed_knob(P.PROOF_DIR, 0), P.composed_fader(P.PROOF_DIR, "white")):
        assert img.getchannel("A").getextrema() == (255, 255)


def test_fader_caps_are_distinct_colours_and_share_one_shadow():
    names = P.asset_names()
    assert sum(n.startswith("fader_shadow_") for n in names) == len(P.SCALES)
    for color in P.FADER_COLORS:
        assert f"fader_cap_{color}_1x.png" in names


def test_knob_ring_is_its_own_layer():
    assert "knob_ring_1x.png" in P.asset_names()
    ring = Image.open(P.PROOF_DIR / "knob_ring_2x.png").convert("RGBA")
    assert ring.getchannel("A").getextrema()[0] == 0  # transparent outside the arc


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


def test_fader_caps_fit_the_frame():
    for color in P.FADER_COLORS:
        alpha = (
            Image.open(P.PROOF_DIR / f"fader_cap_{color}_2x.png").convert("RGBA").getchannel("A")
        )
        assert alpha.getbbox() is not None
        left, top, right, bottom = alpha.getbbox()
        assert left > 4 and top > 4 and right < alpha.width - 4 and bottom < alpha.height - 4


def test_cap_sprites_match_between_scales():
    """1x and 2x describe the same silhouette: bbox of the 2x cap is twice the 1x bbox (+-2 px)."""
    for color in P.FADER_COLORS:
        b1 = (
            Image.open(P.PROOF_DIR / f"fader_cap_{color}_1x.png")
            .convert("RGBA")
            .getchannel("A")
            .getbbox()
        )
        b2 = (
            Image.open(P.PROOF_DIR / f"fader_cap_{color}_2x.png")
            .convert("RGBA")
            .getchannel("A")
            .getbbox()
        )
        assert all(abs(2 * a - b) <= 3 for a, b in zip(b1, b2))


def test_theme_samples_are_measured_data_with_sources():
    data = json.loads(P.THEME_SAMPLES.read_text(encoding="utf-8"))
    assert data["status"] == "MEASURED" and "never committed" in data["source"]
    for name, c in data["colors"].items():
        assert c["note"], name
        for key in ("median", "p10", "p90", "use"):
            assert len(c[key]) == 3 and all(0 <= v <= 255 for v in c[key]), (name, key)
    assert data["proportions"]["knob_flute_count"] > 0


def test_committed_plates_match_sampled_colours():
    """Rendered flat plates land near the measured reference colours (each channel within 6/255)."""
    import numpy as np

    colors = json.loads(P.THEME_SAMPLES.read_text(encoding="utf-8"))["colors"]
    for asset, key in (
        ("plate_red_2x.png", "panel_red"),
        ("plate_slate_slot_2x.png", "panel_slate"),
    ):
        px = np.asarray(Image.open(P.PROOF_DIR / asset).convert("RGB"))[20:100, 20:100].reshape(
            -1, 3
        )
        got, want = np.median(px, axis=0), colors[key]["use"]
        assert max(abs(got - want)) <= 6, (asset, got, want)


def test_main_usage_and_verify(capsys):
    assert P.main([]) == 2
    assert P.main(["verify"]) == 0
    assert "all hashes match" in capsys.readouterr().out


def test_blender_script_shares_constants_and_parses():
    src = (P.ROOT / "assets" / "blender" / "build_proof.py").read_text(encoding="utf-8")
    ast.parse(src)  # bpy is unavailable outside Blender; syntax only
    assert "import panel_assets as P" in src
    assert "P.knob_angle_deg" in src and "P.KNOB_FRAMES" in src
    for name in ("build_proof.py", "proof_parts.py", "proof_materials.py"):
        text = (P.ROOT / "assets" / "blender" / name).read_text(encoding="utf-8")
        ast.parse(text)
        assert len(text.splitlines()) < 400, name
