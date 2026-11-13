"""Tests for tools/panel_refcaps.py on a synthetic drawbar scene (no private reference needed)."""

import numpy as np
import panel_refcaps as RC
from PIL import Image, ImageDraw

SLATE = (52, 62, 75)


def _scene(cap_rgb=(232, 230, 226)) -> Image.Image:
    """Slate plate, a vertical slot, a stepped cap with a dark foot, a red LED and a shadow."""
    img = Image.new("RGB", (300, 400), SLATE)
    d = ImageDraw.Draw(img)
    d.rectangle((140, 0, 160, 399), fill=(20, 20, 20))  # slot
    d.rectangle((205, 100, 225, 115), fill=(240, 40, 40))  # LED, outside the cap columns
    d.rectangle((115, 245, 205, 270), fill=(30, 36, 45))  # shadow on the slate
    d.rectangle((100, 100, 200, 230), fill=cap_rgb)  # top block
    d.rectangle((92, 230, 180, 250), fill=cap_rgb)  # nose, shifted left
    return img


def test_ref_window_size():
    a = RC.ref_window(_scene(), (150, 200), (192, 320))
    assert a.shape == (320, 192, 3)


def test_colour_mask_rejects_slate_led_and_slot():
    a = np.asarray(_scene(), dtype=np.float64)
    m = RC.colour_mask(a, "white")
    assert m[150, 150] and not m[20, 50] and not m[107, 215]
    assert m[20, 150]  # dark neutral (slot) passes the colour test; cap_mask drops it by width


def test_flood_helpers():
    t = np.zeros((10, 10), bool)
    t[3:7, 3:7] = True
    t[0, 0] = True  # a separate speck
    assert RC.largest_from_centre(t).sum() == 16
    ring = np.zeros((9, 9), bool)
    ring[2:7, 2:7] = True
    ring[4, 4] = False
    assert RC.fill_holes(ring)[4, 4]
    assert not RC.largest_from_centre(np.zeros((4, 4), bool)).any()


def test_cap_mask_drops_thin_lines():
    img = _scene()
    ImageDraw.Draw(img).rectangle((90, 98, 210, 99), fill=(20, 20, 20))  # label edge on top
    m = RC.cap_mask(np.asarray(img, dtype=np.float64), "white")
    assert not m[98, 95] and m[150, 150]


def test_cap_mask_keeps_cap_and_drops_slot_strips():
    a = np.asarray(_scene(), dtype=np.float64)
    m = RC.cap_mask(a, "white")
    assert m[150, 150] and m[240, 100]  # top block and nose
    assert not m[20, 150] and not m[380, 150]  # slot above and below
    assert not m[107, 215]  # LED


def test_projection_texture_fills_around_the_cap_with_cap_colour():
    tex = RC.projection_texture(_scene(), (150, 200), (300, 400), "white")
    a = np.asarray(tex, dtype=np.float64)
    assert tex.size == (300, 400)
    assert a[150, 150].tolist() == [232, 230, 226]  # the cap as in the reference
    assert a[20, 50].min() > 150  # slate far away replaced by a light cap colour
    assert a[20, 150].min() > 150  # and so is the slot
