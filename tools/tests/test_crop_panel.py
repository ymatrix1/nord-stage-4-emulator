from crop_panel import calculate_keyboard_bounds, crop_panel_and_keyboard
from PIL import Image


def test_calculate_keyboard_bounds_synthetic():
    # 100x100 white image with a 40x20 dark rectangle in the center
    img = Image.new("RGB", (100, 100), color=(255, 255, 255))
    for y in range(30, 50):
        for x in range(20, 60):
            img.putpixel((x, y), (142, 27, 31))

    xmin, ymin, xmax, ymax = calculate_keyboard_bounds(img, threshold=240)
    assert xmin == 20
    assert ymin == 30
    assert xmax == 60
    assert ymax == 50


def test_calculate_keyboard_bounds_all_white():
    img = Image.new("RGB", (50, 50), color=(255, 255, 255))
    xmin, ymin, xmax, ymax = calculate_keyboard_bounds(img, threshold=240)
    assert (xmin, ymin, xmax, ymax) == (0, 0, 50, 50)


def test_crop_panel_and_keyboard_synthetic(tmp_path):
    img = Image.new("RGB", (200, 100), color=(255, 255, 255))
    for y in range(20, 80):
        for x in range(20, 180):
            img.putpixel((x, y), (142, 27, 31))

    src_path = tmp_path / "synthetic_stage4.jpg"
    img.save(src_path)

    out_dir = tmp_path / "out_assets"
    kb_path, panel_path = crop_panel_and_keyboard(
        str(src_path),
        output_dir=str(out_dir),
        panel_fraction=0.5,
    )

    assert (out_dir / "keyboard_full.jpg").exists()
    assert (out_dir / "panel_texture.jpg").exists()
    assert kb_path.endswith("keyboard_full.jpg")
    assert panel_path.endswith("panel_texture.jpg")
