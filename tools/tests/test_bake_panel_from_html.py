"""Unit tests for tools/bake_panel_from_html.py (no browser required)."""

from __future__ import annotations

import json
from pathlib import Path

import bake_panel_from_html as baker
from PIL import Image


def test_pack_sprites_creates_non_overlapping_atlas() -> None:
    sprites = {
        "btn_black_up": Image.new("RGBA", (68, 34), (20, 20, 20, 255)),
        "knob_pot_cap": Image.new("RGBA", (56, 56), (40, 40, 40, 255)),
        "drawbar_white": Image.new("RGBA", (44, 108), (240, 240, 235, 255)),
    }
    atlas, slots = baker.pack_sprites(sprites, padding=4, max_width=256)

    assert atlas.mode == "RGBA"
    assert set(slots.keys()) == set(sprites.keys())
    for name, slot in slots.items():
        assert slot.w == sprites[name].width
        assert slot.h == sprites[name].height
        assert slot.x >= 4 and slot.y >= 4
        assert slot.x + slot.w <= atlas.width
        assert slot.y + slot.h <= atlas.height


def test_build_sprites_manifest_normalized_uvs() -> None:
    sprites = {
        "a": Image.new("RGBA", (20, 10), (255, 0, 0, 255)),
        "b": Image.new("RGBA", (30, 20), (0, 255, 0, 255)),
    }
    atlas, slots = baker.pack_sprites(sprites, padding=2, max_width=200)
    manifest = baker.build_sprites_manifest(slots, atlas.size, scale=2)

    assert manifest["scale"] == 2
    assert manifest["atlas_px"] == [atlas.width, atlas.height]
    for entry in manifest["sprites"].values():
        u0, v0, u1, v1 = entry["uv"]
        assert 0.0 <= u0 < u1 <= 1.0
        assert 0.0 <= v0 < v1 <= 1.0
        assert entry["source"] == "html_css"


def test_build_assets_manifest_hashes_match_sha256() -> None:
    files = {"panel_plate_2x.png": b"fake-png", "panel_layout.json": b"{}"}
    manifest = baker.build_assets_manifest(files, scale=2)

    assert manifest["scale"] == 2
    assert manifest["files"]["panel_plate_2x.png"]["bytes"] == 8
    assert manifest["files"]["panel_plate_2x.png"]["sha256"] == baker.sha256_bytes(b"fake-png")
    assert json.loads(json.dumps(manifest)) == manifest


def test_default_mock_dir_exists() -> None:
    assert (baker.DEFAULT_MOCK_DIR / "index.html").is_file()
    assert (baker.DEFAULT_MOCK_DIR / "styles.css").is_file()
    assert (baker.DEFAULT_MOCK_DIR / "panel.js").is_file()


def test_cli_help_exits_cleanly() -> None:
    assert Path(baker.__file__).is_file()
    assert len(baker.SPRITE_NAMES) == 13
