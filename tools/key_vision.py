"""Top-down keyboard vision: which keys are under the player's fingertips.

Calibrated for the fixed overhead camera of the Nord Stage 4 Compact 73 demo video
(``_zksT69H30c``): frames are cropped to the keyboard strip (``DEFAULT_KEYBOARD_CROP``) and scaled
to 1920 px wide. All geometry constants are EMPIRICAL (measured by eye on that video, one frame,
±1 key) and are overridable. Detection is a candidate set (fingers resting on keys count), meant
to *confirm* audio-detected notes (``audio_notes.py``), not to replace them.
"""

from __future__ import annotations

from dataclasses import dataclass

import numpy as np

# Normalized crop (x0, y0, x1, y1) of the full frame holding the keys (EMPIRICAL, 3840x2160 video)
DEFAULT_KEYBOARD_CROP = (0.07, 0.78, 0.93, 0.98)
# Compact 73 spans E1..E7 (MIDI 28..100): 43 white keys
FIRST_MIDI = 28
N_WHITE = 43
# Keyboard left/right edges as fractions of the cropped width (EMPIRICAL: 48/1920, 1880/1920)
DEFAULT_EDGES = (0.025, 0.979)
# Fraction of crop height covered by black keys (EMPIRICAL: ~170/251)
BLACK_KEY_BOTTOM = 0.68
PANEL_STRIP = 0.11  # top strip of the crop showing the red panel edge

_WHITE_STEPS = (1, 2, 2, 1, 2, 2, 2)  # E->F, F->G, G->A, A->B, B->C, C->D, D->E


@dataclass(frozen=True)
class Key:
    midi: int
    is_black: bool
    x_center: float  # fraction of cropped width
    x_half: float  # half width, fraction of cropped width


def build_keys(edges: tuple[float, float] = DEFAULT_EDGES) -> list[Key]:
    """All 73 keys with horizontal extents (fractions of the cropped frame width)."""
    left, right = edges
    wk = (right - left) / N_WHITE
    keys: list[Key] = []
    midi = FIRST_MIDI
    for i in range(N_WHITE):
        keys.append(Key(midi, False, left + (i + 0.5) * wk, 0.5 * wk))
        step = _WHITE_STEPS[i % 7]
        if step == 2 and i < N_WHITE - 1:
            keys.append(Key(midi + 1, True, left + (i + 1.0) * wk, 0.3 * wk))
        midi += step
    return sorted(keys, key=lambda k: k.midi)


def skin_mask(rgb: np.ndarray) -> np.ndarray:
    """Boolean skin mask (red panel strip excluded). ``rgb`` is HxWx3 uint8."""
    im = rgb.astype(np.int16)
    r, g, b = im[..., 0], im[..., 1], im[..., 2]
    mask = (r > 110) & (r > g + 12) & (g > b - 5) & ((r - b) > 25) & ((r - b) < 140) & (g > 55)
    mask[: int(PANEL_STRIP * mask.shape[0]), :] = False
    return mask


def fingertips(
    mask: np.ndarray, min_height: float = 0.12, min_gap_px: int = 10
) -> list[tuple[int, int]]:
    """Fingertip candidates ``(x, y)``: local minima of the per-column topmost skin pixel.

    Thin vertical key-edge artefacts are removed by a 3 px column-run requirement.
    """
    h, w = mask.shape
    # require each column to have a solid skin run (>= min_height of height) to ignore edge lines
    solid = np.zeros_like(mask)
    run_min = max(3, int(min_height * h))
    # width filter: a 15-column window must hold >= 30 % skin (fingers wide, key edges 1-3 px)
    col_frac = mask.sum(axis=0) / h
    win = 15
    wide = np.convolve(col_frac, np.ones(win) / win, mode="same") >= 0.30
    for x in range(w):
        col = mask[:, x]
        if not wide[x] or col.sum() < run_min:
            continue
        ys = np.flatnonzero(col)
        solid[ys, x] = True
    top = np.full(w, h, dtype=np.int64)
    for x in range(w):
        ys = np.flatnonzero(solid[:, x])
        if ys.size:
            top[x] = int(ys[0])
    # smooth over 5 columns then pick local minima (fingers point toward the top of the frame)
    k = 5
    padded = np.pad(top.astype(np.float64), (k, k), mode="edge")
    smooth = np.convolve(padded, np.ones(2 * k + 1) / (2 * k + 1), mode="valid")
    tips: list[tuple[int, int]] = []
    for x in range(1, w - 1):
        if smooth[x] >= h - 1:
            continue
        window = smooth[max(0, x - min_gap_px) : x + min_gap_px + 1]
        if smooth[x] <= window.min() and smooth[x] < smooth[max(0, x - 1)] + 1e-9:
            if not tips or x - tips[-1][0] >= min_gap_px:
                tips.append((x, int(top[x])))
    return tips


def key_for_point(keys: list[Key], x_frac: float, y_frac: float) -> Key | None:
    """Black key if the point is on the black-key zone inside a black key, else the white key."""
    if y_frac < BLACK_KEY_BOTTOM:
        for k in keys:
            if k.is_black and abs(x_frac - k.x_center) <= k.x_half:
                return k
    best: Key | None = None
    for k in keys:
        if not k.is_black and abs(x_frac - k.x_center) <= k.x_half:
            best = k
            break
    return best


def candidate_keys(rgb: np.ndarray, keys: list[Key] | None = None) -> set[int]:
    """MIDI notes under detected fingertips in one cropped keyboard frame."""
    keys = keys or build_keys()
    mask = skin_mask(rgb)
    h, w = mask.shape
    out: set[int] = set()
    for x, y in fingertips(mask):
        k = key_for_point(keys, x / w, y / h)
        if k is not None:
            out.add(k.midi)
    return out


def covered_keys(
    rgb: np.ndarray, keys: list[Key] | None = None, min_fraction: float = 0.2
) -> set[int]:
    """MIDI notes whose column is >= ``min_fraction`` covered by skin (hand footprint, superset)."""
    keys = keys or build_keys()
    mask = skin_mask(rgb)
    h, w = mask.shape
    y0 = int(PANEL_STRIP * h)
    out: set[int] = set()
    for k in keys:
        x0 = max(0, int((k.x_center - k.x_half) * w))
        x1 = min(w, int((k.x_center + k.x_half) * w))
        if x1 > x0 and mask[y0:, x0:x1].mean() >= min_fraction:
            out.add(k.midi)
    return out
