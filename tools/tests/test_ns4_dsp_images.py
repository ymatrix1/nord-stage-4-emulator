import os
import struct
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import ns4_dsp_images as m  # noqa: E402

BASE = m.LOAD_BASE


def synthetic(chain, pool_at=0x100, size=0x20000):
    """Image with a literal pool of (pointer, length) pairs, like bank0_CODE@0x241DAC."""
    data = bytearray(size)
    words = []
    for off, length in chain:
        words += [BASE + off, length, 0x12345678]  # pointer, length, an unrelated word
    struct.pack_into(f"<{len(words)}I", data, pool_at, *words)
    return bytes(data)


CHAIN = [(0x4000, 0x2000), (0x6000, 0x1800), (0x7800, 0x800), (0x8000, 0x4000), (0xC000, 0x1000)]


def test_find_chain_recovers_all_images():
    assert m.find_chain(synthetic(CHAIN)) == CHAIN


def test_find_chain_ignores_unrelated_pairs():
    data = bytearray(synthetic(CHAIN))
    # a stray pointer/length pair that does not chain to anything else
    struct.pack_into("<2I", data, 0x400, BASE + 0x15000, 0x500)
    assert m.find_chain(bytes(data)) == CHAIN


def test_find_chain_empty_when_no_pairs():
    assert m.find_chain(bytes(0x4000)) == []


def test_export_writes_images(tmp_path):
    data = synthetic(CHAIN)
    paths = m.export(data, CHAIN, tmp_path / "out")
    assert [p.name for p in paths][:3] == [
        "dsp_image_0_FX.bin",
        "dsp_image_1_PNO.bin",
        "dsp_image_2_ORG.bin",
    ]
    assert paths[3].read_bytes() == data[0x8000:0xC000]


def test_main_reports_and_exports(tmp_path, capsys):
    f = tmp_path / "bank0_CODE.bin"
    f.write_bytes(synthetic(CHAIN))
    assert m.main([str(f), "--out", str(tmp_path / "o")]) == 0
    out = capsys.readouterr().out
    assert "idx0 FX" in out and "5 images, contiguous" in out
    empty = tmp_path / "e.bin"
    empty.write_bytes(bytes(0x1000))
    assert m.main([str(empty)]) == 1


def test_chain_stops_before_a_string_table():
    data = bytearray(synthetic(CHAIN + [(0xD000, 0x800)]))
    data[0xD000:0xD040] = b"Layer Level Adjust Piano Acoustic Piano Electric Organ/Acc".ljust(0x40)
    assert m.find_chain(bytes(data)) == CHAIN
    assert m._looks_like_text(b"Layer Level Adjust")
    assert not m._looks_like_text(bytes(32))
    assert not m._looks_like_text(b"")
