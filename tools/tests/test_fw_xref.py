from __future__ import annotations

import struct
from pathlib import Path

import fw_xref


def make_image() -> bytes:
    """name table at 0x40, a pointer table at 0x80 referencing it."""
    img = bytearray(0x100)
    img[0x40:0x4B] = b"\0Edt_Syn_kA\0"
    img[0x4C:0x56] = b"Edt_Syn_kB\0"
    struct.pack_into("<I", img, 0x80, fw_xref.LOAD_BASE + 0x41)
    struct.pack_into("<I", img, 0x84, 0x1234)
    struct.pack_into("<I", img, 0x88, fw_xref.LOAD_BASE + 0x4C)
    struct.pack_into("<I", img, 0x8C, fw_xref.LOAD_BASE + 0x41)
    return bytes(img)


def test_find_string_and_xrefs():
    img = make_image()
    at = fw_xref.find_string(img, "Edt_Syn_kA")
    assert at == 0x41
    assert fw_xref.xrefs(img, at) == [0x80, 0x8C]
    assert fw_xref.find_string(img, "missing") is None


def test_c_string_at_rejects_out_of_range_and_non_text():
    img = make_image()
    assert fw_xref.c_string_at(img, fw_xref.LOAD_BASE + 0x4C) == "Edt_Syn_kB"
    assert fw_xref.c_string_at(img, 0x10) is None
    assert fw_xref.c_string_at(img, fw_xref.LOAD_BASE + 0xFFFF) is None
    assert fw_xref.c_string_at(img, fw_xref.LOAD_BASE + 0x00) is None  # empty string


def test_dump_words_resolves_strings():
    rows = fw_xref.dump_words(make_image(), 0x80, 0x90)
    assert rows[0] == (0x80, fw_xref.LOAD_BASE + 0x41, "Edt_Syn_kA")
    assert rows[1] == (0x84, 0x1234, "")
    assert rows[2][2] == "Edt_Syn_kB"


def test_main_modes(tmp_path: Path, capsys):
    img = tmp_path / "code.bin"
    img.write_bytes(make_image())
    assert fw_xref.main([str(img), "--string", "Edt_Syn_kA"]) == 0
    assert "0x80" in capsys.readouterr().out
    assert fw_xref.main([str(img), "--string", "nope"]) == 1
    assert fw_xref.main([str(img), "--dump", "0x80", "0x84"]) == 0
    assert "Edt_Syn_kA" in capsys.readouterr().out
