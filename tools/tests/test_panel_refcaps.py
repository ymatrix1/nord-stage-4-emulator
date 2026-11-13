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


def test_cap_layers_white_and_black():
    for rgb, kind in (((232, 230, 226), "white"), ((57, 53, 50), "black")):
        cap, shadow = RC.cap_layers(_scene(rgb), (150, 200), (300, 400), kind)
        ca, sa = np.asarray(cap), np.asarray(shadow)
        assert ca[150, 150, 3] == 255 and ca[20, 50, 3] == 0, kind
        assert ca[150, 150, :3].tolist() == list(rgb), kind
        assert sa[258, 160, 3] > 40, kind  # darkened slate below the cap becomes shadow
        assert sa[20, 50, 3] == 0, kind  # clean slate far away casts nothing


def test_matte_ignores_bluish_label_next_to_the_cap():
    a = np.zeros((40, 40, 3)) + (52, 62, 75)
    a[:8] = (181, 192, 206)  # silver label strip above
    a[10:30, 10:30] = (232, 230, 226)  # cap
    m = np.zeros((40, 40), bool)
    m[10:30, 10:30] = True
    alpha, rgb = RC.matte(a, m)
    assert alpha[7, 20] == 0 and alpha[20, 20] == 1


def test_smooth_alpha_removes_nubs_and_antialiases():
    m = np.zeros((60, 60), bool)
    m[10:50, 10:50] = True
    m[20:22, 50:52] = True  # 2 px nub
    m[30, 10] = False  # ragged notch
    al = RC.smooth_alpha(m)
    assert al[30, 30] == 1 and al[21, 51] == 0 and al[30, 10] > 0.5
    assert 0 < al[30, 9] < 1 or 0 < al[30, 10] < 1 or 0 < al[30, 49] < 1  # soft edge
