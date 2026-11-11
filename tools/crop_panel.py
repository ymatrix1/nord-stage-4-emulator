"""Extract and crop high-fidelity panel textures from reference images.

Used to prepare 1:1 hardware photorealistic texture assets for crates/nord-gui.
"""

from __future__ import annotations

import os
from typing import Tuple

from PIL import Image


def calculate_keyboard_bounds(
    im: Image.Image,
    threshold: int = 240,
) -> Tuple[int, int, int, int]:
    """Finds the bounding box (xmin, ymin, xmax, ymax) of non-white pixels."""
    # Convert to grayscale and threshold to find non-white pixels
    gray = im.convert("L")
    # Invert so non-white pixels become bright
    mask = gray.point(lambda p: 255 if p < threshold else 0, mode="1")
    bbox = mask.getbbox()
    if bbox is None:
        return (0, 0, im.width, im.height)
    return bbox


def crop_panel_and_keyboard(
    image_path: str,
    output_dir: str = "crates/nord-gui/assets",
    panel_fraction: float = 0.52,
) -> Tuple[str, str]:
    """Crops the keyboard and upper front panel from a reference image and saves textures."""
    im = Image.open(image_path)
    w, h = im.size

    # Downsample for quick bounding box calculation
    small_w = 1160
    small_h = max(1, int(1160 / (w / h)))
    small = im.resize((small_w, small_h), Image.Resampling.BOX)

    s_xmin, s_ymin, s_xmax, s_ymax = calculate_keyboard_bounds(small)
    scale_x = w / small_w
    scale_y = h / small_h

    xmin = int(s_xmin * scale_x)
    ymin = int(s_ymin * scale_y)
    xmax = int(s_xmax * scale_x)
    ymax = int(s_ymax * scale_y)

    kb_crop = im.crop((xmin, ymin, xmax, ymax))
    os.makedirs(output_dir, exist_ok=True)

    # 1. Full keyboard (1920px wide)
    aspect_kb = kb_crop.width / kb_crop.height
    kb_path = os.path.join(output_dir, "keyboard_full.jpg")
    kb_resized = kb_crop.resize((1920, int(1920 / aspect_kb)), Image.Resampling.LANCZOS)
    kb_resized.save(kb_path, quality=88)

    # 2. Upper front panel
    panel_h = int(kb_crop.height * panel_fraction)
    panel_crop = kb_crop.crop((0, 0, kb_crop.width, panel_h))
    aspect_panel = panel_crop.width / panel_crop.height
    panel_path = os.path.join(output_dir, "panel_texture.jpg")
    panel_resized = panel_crop.resize((1920, int(1920 / aspect_panel)), Image.Resampling.LANCZOS)
    panel_resized.save(panel_path, quality=90)

    return (kb_path, panel_path)


if __name__ == "__main__":
    ref_image = os.path.join(".reference", "NS4_HA88_TopDown-01_241008.jpg")
    if not os.path.exists(ref_image):
        ref_image = os.path.join(".reference", "foto.jpg")
    if os.path.exists(ref_image):
        kb, pan = crop_panel_and_keyboard(ref_image)
        print(f"Generated textures:\n  Keyboard: {kb}\n  Panel: {pan}")
    else:
        print(f"Reference image not found at {ref_image}")
