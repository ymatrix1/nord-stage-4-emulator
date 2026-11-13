"""Tests for tools/panel_preview.py (layer composition of the committed proof sprites)."""

import panel_assets as P
import panel_preview as V
from PIL import Image


def test_tiled_repeats_and_covers():
    tile = Image.new("RGBA", (4, 4), (1, 2, 3, 255))
    tile.putpixel((0, 0), (9, 9, 9, 255))
    out = V.tiled(tile, 10, 6)
    assert out.size == (10, 6)
    assert out.getpixel((4, 4))[:3] == (9, 9, 9) and out.getpixel((8, 0))[:3] == (9, 9, 9)
    assert out.getchannel("A").getextrema() == (255, 255)


def test_centre_crop_size_and_centre():
    img = Image.new("RGB", (9, 9))
    img.putpixel((4, 4), (255, 0, 0))
    out = V.centre_crop(img, 3, 3)
    assert out.size == (3, 3) and out.getpixel((1, 1)) == (255, 0, 0)


def test_stack_layout():
    a, b = Image.new("RGBA", (5, 4), (255, 0, 0, 255)), Image.new("RGBA", (3, 7), (0, 255, 0, 255))
    sheet = V.stack([[a, b], [b]], gap=2)
    assert sheet.size == (5 + 2 + 3, 7 + 2 + 7)
    assert sheet.getpixel((0, 0))[:3] == (255, 0, 0)
    assert sheet.getpixel((7, 0))[:3] == (0, 255, 0)
    assert sheet.getpixel((0, 9))[:3] == (0, 255, 0)


def test_composites_are_opaque():
    """Plate + layer + part composites cover their whole frame."""
    imgs = [
        V.composed_knob(P.PROOF_DIR, 0),
        V.composed_ref_knob(P.PROOF_DIR, "drive", 10),
        V.composed_cap(P.PROOF_DIR, "drawbar_cap_white"),
        V.composed_cap(P.PROOF_DIR, "layer_cap_grey"),
        V.composed_ref_knob(P.PROOF_DIR, "master", 59),
    ]
    for img in imgs:
        assert img.getchannel("A").getextrema() == (255, 255)
    assert imgs[2].size == tuple(2 * v for v in P.CAP_FRAMES["drawbar_cap_white"])


def test_knob_frame_matches_grid_cell():
    e = P.KNOB_FRAME_PX * 2
    assert V.knob_frame(P.PROOF_DIR, 63).size == (e, e)
    assert V.strip_frame(P.PROOF_DIR, 5, 2, "pointer_master").size == (e, e)


def test_compose_preview_covers_all_groups():
    sheet = V.compose_preview()
    e = P.KNOB_FRAME_PX * 2
    assert sheet.width >= 7 * e  # six reference knob frames and one generic
    assert sheet.height > e + 2 * P.CAP_FRAMES["drawbar_cap_black"][1]
