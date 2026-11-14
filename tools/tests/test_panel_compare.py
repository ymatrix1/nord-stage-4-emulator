"""Tests for tools/panel_compare.py (synthetic references only; no private photos needed)."""

import panel_compare as C
from PIL import Image


def test_ref_crop_scales_to_our_frame_and_pads_outside():
    ref = Image.new("RGB", (200, 100), (10, 20, 30))
    out = C.ref_crop(ref, (100.0, 50.0), (64, 32))
    assert out.size == (64, 32)
    assert out.getpixel((32, 16))[:3] == (10, 20, 30)
    edge = C.ref_crop(ref, (199.0, 99.0), (64, 64))  # mostly outside the image: no error
    assert edge.size == (64, 64)


def test_diff_metric_zero_for_identical_and_positive_otherwise():
    a = Image.new("RGB", (4, 4), (100, 50, 25))
    m = C.diff_metric(a, a.copy())
    assert m["mad"] == 0 and m["ours_mean"] == [100.0, 50.0, 25.0]
    assert C.diff_metric(a, Image.new("RGB", (4, 4), (110, 50, 25)))["mad"] > 3


def test_features_cover_every_reference_style_inside_the_frame():
    feats = C.features()
    for name, ((x, y), spec) in feats.items():
        assert 0 < x < 1961 and 0 < y < 1185, name
        assert spec[0] in {"refknob", "cap", "sprite", "tri", "tile"}, name
    assert sum(n.startswith("button_") for n in feats) >= 15
    assert {"knob_master", "knob_drive", "tri_organ_model", "tri_vib_chorus"} <= set(feats)
    assert feats["knob_master"][1][2] == 61  # pointer 127 deg clockwise


def test_build_and_main(tmp_path, capsys):
    ref = tmp_path / "ref.png"
    Image.new("RGB", (1961, 1185), (50, 60, 70)).save(ref)
    sheet, metrics = C.build(ref)
    assert set(metrics) == set(C.features())
    assert sheet.width > 2 * 2 * 192
    out = tmp_path / "sheet.png"
    assert C.main(["--ref", str(ref), "--out", str(out)]) == 0
    assert out.exists() and "knob_master" in capsys.readouterr().out
    assert C.main([]) == 2
