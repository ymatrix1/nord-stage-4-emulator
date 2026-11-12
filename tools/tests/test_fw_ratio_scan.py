from __future__ import annotations

import struct
from pathlib import Path

import fw_ratio_scan as fr
import pytest


def image_with(cents: list[float], at: int = 0x40, size: int = 0x200) -> bytes:
    img = bytearray(size)
    for i, c in enumerate(cents):
        struct.pack_into("<f", img, at + 4 * i, fr.cents_to_ratio(c))
    return bytes(img)


def test_ratio_cent_conversions_roundtrip():
    assert fr.ratio_to_cents(2.0) == pytest.approx(1200.0)
    assert fr.ratio_to_cents(fr.cents_to_ratio(7.5)) == pytest.approx(7.5)
    assert fr.cents_to_ratio(0.0) == 1.0


def test_find_cent_constants_picks_round_cents_only():
    img = bytearray(image_with([-5, 5, 15, -15]))
    struct.pack_into("<f", img, 0x80, 1.0)  # unity ignored
    struct.pack_into("<f", img, 0x84, 1.01234567)  # not a round cent value
    struct.pack_into("<f", img, 0x88, 1.5)  # out of range
    hits = fr.find_cent_constants(bytes(img))
    assert [c for _, c in hits] == [-5.0, 5.0, 15.0, -15.0]
    assert [o for o, _ in hits] == [0x40, 0x44, 0x48, 0x4C]


def test_clusters_need_enough_close_hits():
    hits = fr.find_cent_constants(image_with([5, -5, 15, -15, 25]))
    groups = fr.clusters(hits)
    assert len(groups) == 1 and len(groups[0]) == 5
    assert fr.clusters(fr.find_cent_constants(image_with([5, -5]))) == []
    far = fr.find_cent_constants(image_with([5], at=0x10) + image_with([-5], at=0x10))
    assert fr.clusters(far, gap=16, min_size=2) == []


def test_cli_prints_cluster(tmp_path: Path, capsys):
    img = tmp_path / "code.bin"
    img.write_bytes(image_with([-5, 5, 15, -15]))
    assert fr.main([str(img)]) == 0
    out = capsys.readouterr().out
    assert "0x40..0x4c (4)" in out and "-5, +5, +15, -15" in out
