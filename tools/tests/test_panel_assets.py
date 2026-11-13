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


def test_main_usage_and_verify(capsys):
    assert P.main([]) == 2
    assert P.main(["verify"]) == 0
    assert "all hashes match" in capsys.readouterr().out


def test_blender_script_shares_constants_and_parses():
    src = (P.ROOT / "assets" / "blender" / "build_proof.py").read_text(encoding="utf-8")
    ast.parse(src)  # bpy is unavailable outside Blender; syntax only
    assert "import panel_assets as P" in src
    assert "P.knob_angle_deg" in src and "P.KNOB_FRAMES" in src
    assert len(src.splitlines()) < 400
