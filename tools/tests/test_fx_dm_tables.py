"""Tests for tools/fx_dm_tables.py (monotone float run finder)."""

import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import fx_dm_tables as t  # noqa: E402


def image_with(words: dict[int, float]) -> bytes:
    """Flat image whose normal-word addresses hold the given floats."""
    size = 4 * (max(words) + 1)
    data = bytearray(size)
    for a, v in words.items():
        struct.pack_into("<f", data, 4 * a, v)
    return bytes(data)


def test_runs_finds_increasing_and_decreasing_tables():
    values = [0.1 * k for k in range(1, 11)] + [0.0] + [5.0 - 0.2 * k for k in range(10)]
    found = t.runs(values, 0x90000, 5)
    assert found[0][:3] == (0x90000, 10, 1)
    assert any(r[2] == -1 and r[1] >= 9 for r in found)


def test_short_runs_zeros_and_nan_are_skipped():
    values = [0.0, 1.0, 2.0, 0.0, 3.0, float("nan"), 4.0, 5.0]
    assert t.runs(values, 0, 3) == []
    assert not t.usable(0.0) and not t.usable(float("nan")) and not t.usable(1e30)
    assert t.usable(-0.5)


def test_floats_reads_a_flat_image_by_word_address():
    img = image_with({0x90002: 1.5, 0x90003: 2.5})
    v = t.floats(img, 0x90000, 0x90005)
    assert v[2:4] == [1.5, 2.5]
    assert v[0] == 0.0
    assert v[4] != v[4]  # beyond the image: NaN


def test_main_prints_a_table_row(tmp_path, capsys):
    words = {0x90010 + k: 0.01 * (k + 1) for k in range(20)}
    p = tmp_path / "img.bin"
    p.write_bytes(image_with(words))
    assert t.main([str(p), "--min", "10"]) == 0
    out = capsys.readouterr().out
    assert out.startswith("0x90010 20 inc ")
