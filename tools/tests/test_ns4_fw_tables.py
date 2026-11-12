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
    assert ft.main([str(good), "--rust", str(RS)]) == 0
    assert "max err" in capsys.readouterr().out
    values = rust_values()
    values[5] += 1.0
    bad = tmp_path / "bad.bin"
    bad.write_bytes(image_with(values))
    assert ft.main([str(bad), "--rust", str(RS)]) == 1
