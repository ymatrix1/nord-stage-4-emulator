import audio_notes as an
import key_vision as kv
import numpy as np
import video_ref as vr


def test_build_keys_geometry():
    keys = kv.build_keys()
    assert len(keys) == 73
    assert sum(k.is_black for k in keys) == 30
    assert keys[0].midi == 28 and keys[-1].midi == 100
    assert [k.midi for k in keys] == sorted(k.midi for k in keys)
    whites = [k for k in keys if not k.is_black]
    assert len(whites) == kv.N_WHITE
    assert all(b.x_center > a.x_center for a, b in zip(whites, whites[1:], strict=False))


def synthetic_frame(x_frac: float, width_px: int = 60) -> np.ndarray:
    h, w = 251, 1920
    img = np.full((h, w, 3), 245, dtype=np.uint8)  # white keys
    cx = int(x_frac * w)
    # finger-shaped skin blob pointing up (top at y=80), wide enough to pass the width filter
    img[80:, cx - width_px // 2 : cx + width_px // 2] = (200, 140, 120)
    return img


def test_skin_mask_selects_skin_not_white_or_panel():
    img = synthetic_frame(0.5)
    img[:10, :] = (200, 20, 20)  # red panel strip
    m = kv.skin_mask(img)
    assert m[150, 960] and not m[150, 100] and not m[5, 960]


def test_candidate_keys_finds_key_under_finger():
    keys = kv.build_keys()
    target = next(k for k in keys if k.midi == 60)
    img = synthetic_frame(target.x_center)
    got = kv.candidate_keys(img, keys)
    assert any(abs(m - 60) <= 1 for m in got), got


def test_covered_keys_superset_of_finger_column():
    keys = kv.build_keys()
    target = next(k for k in keys if k.midi == 72)
    assert 72 in kv.covered_keys(synthetic_frame(target.x_center), keys)
    assert kv.covered_keys(np.full((251, 1920, 3), 245, dtype=np.uint8), keys) == set()


def test_video_support_fraction():
    ev = [an.NoteEvent(60, 0.0, 1.0, 1.0), an.NoteEvent(70, 0.0, 1.0, 1.0)]
    frames = [{60}] * 4 + [{61}] * 2  # 6 frames at 4 fps = 1.5 s
    sup = vr.video_support(ev, frames, fps=4.0, pad_s=0.0, tolerance=1)
    assert sup[0] == 1.0 and sup[1] == 0.0
    assert vr.video_support(ev, [], fps=4.0) == [0.0, 0.0]


def test_normalize_source_path(monkeypatch):
    monkeypatch.setattr(vr.sys, "platform", "win32")
    assert vr.normalize_source_path("/c/dev/x/y.webm") == "C:/dev/x/y.webm"
    assert vr.normalize_source_path("https://youtu.be/abc") == "https://youtu.be/abc"
    monkeypatch.setattr(vr.sys, "platform", "linux")
    assert vr.normalize_source_path("/c/dev/x") == "/c/dev/x"
