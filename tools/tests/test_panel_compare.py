"""Tests for tools/panel_compare.py (synthetic references only; no private photos needed)."""

import panel_compare as C
from PIL import Image


def test_scaled_keeps_aspect():
    assert C.scaled(Image.new("RGB", (100, 50)), 0.5).size == (50, 25)
    assert C.scaled(Image.new("RGB", (2, 2)), 0.01).size == (1, 1)


def test_side_by_side_layout():
    a, b = Image.new("RGBA", (10, 20), (255, 0, 0, 255)), Image.new("RGB", (6, 8), (0, 255, 0))
    s = C.side_by_side(a, b, gap=4)
    assert s.size == (20, 20)
    assert s.getpixel((0, 0))[:3] == (255, 0, 0)
    assert s.getpixel((14, 0))[:3] == (0, 255, 0)


def test_build_and_main(tmp_path, capsys):
    knob = tmp_path / "knob.png"
    caps = tmp_path / "caps.png"
    Image.new("RGB", (134, 146), (140, 30, 40)).save(knob)
    Image.new("RGB", (1961, 1185), (50, 60, 70)).save(caps)
    out = tmp_path / "sheet.png"
    assert C.main(["--knob-ref", str(knob), "--caps-ref", str(caps), "--out", str(out)]) == 0
    assert Image.open(out).width > 2 * C.OUR_KNOB_PX
    assert C.main([]) == 2
