import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import sharc_visa as v  # noqa: E402

# Bytes of bank0_CODE (OS v1.64) at 0x45D630, two bytes after the record header.
RECORD = bytes.fromhex(
    "1a72bf00fe6abf10a40f70003e043fe77e683f10150f0c0040047e688a100010861400002000"
)


def test_classify_known_forms():
    assert v.classify(0x721A, 0x003F) == ("5b", 2)  # 011 1 0 ..., marker
    assert v.classify(0x1900 | 0x01, None) == ("25", 3)
    assert v.classify(0xC123, None) == ("2c", 1)
    assert v.classify(0x0FA4, 0x006C) == ("17b", 2)
    assert v.classify(0x0F15, 0x000C) == ("17a", 3)
    assert v.classify(0x1486, 0x0000) == ("18a", 3)


def test_record_tiles_exactly_only_from_the_right_phase():
    par = v.parcels_le(RECORD)
    insns, exact = v.tile(par)
    assert exact
    assert [n for n, _ in insns][:3] == ["5b", "4b", "17b"]
    header_phase = bytes.fromhex("2000") + RECORD[:-2]  # start at the record header
    _, exact_shifted = v.tile(v.parcels_le(header_phase))
    assert not exact_shifted


def test_ureg_load_reads_immediates():
    par = v.parcels_le(RECORD)
    insns, _ = v.tile(par)
    loads = [v.ureg_load(ps) for n, ps in insns if n in ("17a", "17b")]
    assert (0x24, 0x70) in loads  # M4 = 0x70 (0x74 in v1.62)
    assert (0x15, 0x000C0440) in loads  # I5 = 0xC0440


def test_main_prints_and_reports(tmp_path, capsys):
    f = tmp_path / "x.bin"
    f.write_bytes(RECORD)
    assert v.main([str(f), "0", str(len(RECORD))]) == 0
    out = capsys.readouterr().out
    assert "17b" in out and "tiles exactly" in out
