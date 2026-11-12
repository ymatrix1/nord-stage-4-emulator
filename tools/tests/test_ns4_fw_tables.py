from __future__ import annotations

import struct
from pathlib import Path

import ns4_fw_tables as ft
import pytest

RS = Path(__file__).resolve().parents[2] / "crates/nord-core/src/synth_tables.rs"


def image_with(values: list[float]) -> bytes:
    img = bytearray(ft.LFO_PITCH_OFFSET + 4 * ft.TABLE_LEN + 16)
    struct.pack_into("<128f", img, ft.LFO_PITCH_OFFSET, *values)
    return bytes(img)


def rust_values() -> list[float]:
    return ft.parse_rust_table(RS.read_text(encoding="utf-8"), "LFO_PITCH_SEMITONES")


def test_parse_rust_table_reads_the_repo_table():
    values = rust_values()
    assert len(values) == 128 and values[0] == 0.0 and values[-1] == 48.0
    with pytest.raises(ValueError):
        ft.parse_rust_table("pub const OTHER: [f32; 2] = [1.0, 2.0];", "MISSING")
    with pytest.raises(ValueError):
        ft.parse_rust_table("pub const T: [f32; 3] = [1.0, 2.0];", "T")


def test_check_lfo_pitch_matches_when_image_holds_the_same_table():
    err, mono = ft.check_lfo_pitch(image_with(rust_values()), RS.read_text(encoding="utf-8"))
    assert err < 1e-6 and mono


def test_check_lfo_pitch_flags_a_changed_or_non_monotonic_table():
    values = rust_values()
    bad = values.copy()
    bad[64] += 0.5
    err, mono = ft.check_lfo_pitch(image_with(bad), RS.read_text(encoding="utf-8"))
    assert err == pytest.approx(0.5, abs=1e-5)
    assert not mono  # the bump also breaks monotonicity
    swapped = values.copy()
    swapped[10], swapped[11] = swapped[11], swapped[10]
    _, mono = ft.check_lfo_pitch(image_with(swapped), RS.read_text(encoding="utf-8"))
    assert not mono


def test_read_table_bounds_and_length_mismatch():
    with pytest.raises(ValueError):
        ft.read_f32_table(b"\0" * 8, 0)
    with pytest.raises(ValueError):
        ft.max_abs_error([1.0], [1.0, 2.0])


def test_main_exit_codes(tmp_path: Path, capsys):
    good = tmp_path / "fw.bin"
    good.write_bytes(image_with(rust_values()))
    assert ft.main([str(good), "--rust", str(RS), "--lfo-only"]) == 0
    assert "max err" in capsys.readouterr().out
    values = rust_values()
    values[5] += 1.0
    bad = tmp_path / "bad.bin"
    bad.write_bytes(image_with(values))
    assert ft.main([str(bad), "--rust", str(RS), "--lfo-only"]) == 1


PRESETS = Path(__file__).resolve().parents[2] / "crates/nord-core/src/cbin/presets.rs"


def display_image(texts: list[str], offset: int = 0x400, strings_at: int = 0x1000) -> bytes:
    """Image with a 128-entry pointer table at `offset` whose strings start at `strings_at`."""
    img = bytearray(strings_at + 128 * 8 + 64)
    for i, text in enumerate(texts):
        at = strings_at + i * 8
        img[at : at + len(text)] = text.encode("latin1")
        struct.pack_into("<I", img, offset + 4 * i, ft.LOAD_BASE + at)
    return bytes(img)


def test_parse_display_values_and_tolerances():
    assert ft.parse_display("748 ms") == (748.0, 0.5)
    assert ft.parse_display("0.03 Hz") == pytest.approx((0.03, 0.005))
    assert ft.parse_display("43 s") == (43000.0, 500.0)
    assert ft.parse_display("32 kHz") == (32000.0, 500.0)
    assert ft.parse_display("Inf")[0] == float("inf")
    with pytest.raises(ValueError):
        ft.parse_display("loud")


def test_display_string_bounds():
    img = display_image(["1 ms"] * 128)
    assert ft.display_string(img, ft.LOAD_BASE + 0x1000) == "1 ms"
    with pytest.raises(ValueError):
        ft.display_string(img, 0x10)


def test_check_display_table_accepts_rounding_and_flags_real_differences():
    ours = [float(i + 1) for i in range(128)]
    texts = [f"{i + 1} ms" for i in range(128)]
    assert ft.check_display_table(display_image(texts), 0x400, ours) == []
    # 0.4 ms off is inside the +/-0.5 display rounding, 2 ms is not.
    near = ours.copy()
    near[3] += 0.4
    assert ft.check_display_table(display_image(texts), 0x400, near) == []
    far = ours.copy()
    far[3] += 2.0
    bad = ft.check_display_table(display_image(texts), 0x400, far)
    assert [b[0] for b in bad] == [3]


def test_check_display_table_inf_means_hold():
    ours = [10.0] * 127 + [45_000.0]
    texts = ["10 ms"] * 127 + ["Inf"]
    assert ft.check_display_table(display_image(texts), 0x400, ours) == []
    assert ft.check_display_table(display_image(texts), 0x400, [10.0] * 128)[0][1] == "Inf"


def test_display_tables_cover_five_repo_tables():
    assert [name for name, _, _ in ft.DISPLAY_TABLES][:2] == ["env attack", "env decay"]
    src = PRESETS.read_text(encoding="utf-8")
    for _, _, rust_name in ft.DISPLAY_TABLES:
        assert len(ft.parse_rust_table(src, rust_name)) == 128


def test_parse_rust_table_resolves_aliases():
    src = "pub const A: [f32; 2] = [1.0, 2.0];" + chr(10) + "pub const B: [f32; 2] = A;"
    assert ft.parse_rust_table(src, "B") == [1.0, 2.0]
