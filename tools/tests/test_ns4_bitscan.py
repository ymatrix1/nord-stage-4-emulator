from __future__ import annotations

import zipfile
from pathlib import Path

import ns4_bitscan as bs
import pytest


def _payload(setbits: dict[int, int]) -> bytes:
    """493-byte payload with the given absolute bit positions set."""
    buf = bytearray(bs.SYNTH_PAYLOAD_LEN)
    for pos in setbits:
        buf[pos // 8] |= 0x80 >> (pos % 8)
    return bytes(buf)


def test_sound_blocks_slice_layers_and_reject_short():
    payload = _payload({1472 + 5: 1, 1856 + 7: 1, 2240 + 383: 1})
    a, b, c = bs.sound_blocks(payload)
    assert (len(a), len(b), len(c)) == (384, 384, 384)
    assert a[5] == "1" and b[7] == "1" and c[383] == "1"
    assert a.count("1") == b.count("1") == c.count("1") == 1
    with pytest.raises(ValueError):
        bs.sound_blocks(b"\x00" * 10)


def test_bit_counts_map_and_histogram():
    blocks = ["10" + "0" * 382, "11" + "0" * 382, "10" + "0" * 382, "10" + "0" * 382]
    counts = bs.bit_counts(blocks)
    assert counts[0] == 4 and counts[1] == 1 and counts[2] == 0
    m = bs.bit_map(blocks)
    assert m[0] == "#" and m[2] == "."
    assert bs.field_histogram(blocks, 0, 2) == [(2, 3), (3, 1)]
    assert bs.fraction_equal(blocks, 0, 2, 2) == 0.75
    assert bs.fraction_equal([], 0, 2, 2) == 0.0
    assert bs.group_histogram(blocks, 0, 1, 2)[0] == [(1, 4)]


def test_loaders_read_dir_and_ns4b(tmp_path: Path):
    payload = _payload({1472 + 1: 1})
    raw = b"\x00" * bs.CBIN_HEADER_SIZE + payload
    d = tmp_path / "presets"
    d.mkdir()
    (d / "A.ns4y").write_bytes(raw)
    assert bs.load_presets(d) == {"A": payload}

    z = tmp_path / "x.ns4b"
    with zipfile.ZipFile(z, "w") as zf:
        zf.writestr("Synth Preset/Bank 1/B.ns4y", raw)
        zf.writestr("Program/A/C.ns4p", b"ignored")
    loaded = bs.load_presets(z)
    assert list(loaded) == ["Synth Preset/Bank 1/B.ns4y"]
    assert len(bs.collect_blocks(loaded)) == 3


def test_main_prints_map_and_hist(tmp_path: Path, capsys):
    d = tmp_path / "p"
    d.mkdir()
    (d / "A.ns4y").write_bytes(b"\x00" * bs.CBIN_HEADER_SIZE + _payload({1472 + 318: 1}))
    assert bs.main([str(d), "--map", "--hist", "318:7:2"]) == 0
    out = capsys.readouterr().out
    assert "3 Sound blocks" in out and "+318 w7" in out and "+325 w7" in out
