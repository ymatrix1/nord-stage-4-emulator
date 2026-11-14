from __future__ import annotations

import struct
from pathlib import Path

import ns4_arp_presets as ap

CSV = Path(__file__).resolve().parents[2] / "docs" / "research" / "arp-preset-table.csv"


def test_digits_and_word_from_round_trip():
    word = ap.word_from("EqQ.", ap.GATE_CHARS)
    assert word == 0b11_10_01_00
    assert ap.digits(word, ap.GATE_CHARS, 4) == "EqQ."
    assert ap.digits(ap.word_from("CLRC", ap.PAN_CHARS), ap.PAN_CHARS, 4) == "CLRC"


def _image() -> bytes:
    """Tiny code image: strings, then one table entry at `STRUCT_BASE` (v = 1)."""
    img = bytearray(0x100)
    strings = {0x00: b"Name\0", 0x10: b"SL_S\0", 0x20: b"S___\0"}
    for off, s in strings.items():
        img[off : off + len(s)] = s
    base = ap.STRUCT_BASE - ap.LOAD_BASE
    padded = bytearray(base + 20)
    padded[: len(img)] = img
    ptr = ap.LOAD_BASE
    struct.pack_into("<IIIII", padded, base, ptr, 1, 4, ptr + 0x10, ptr + 0x20)
    return bytes(padded)


def test_read_struct_decodes_one_entry():
    entry = ap.read_struct(_image(), 1)
    assert entry == {"name": "Name", "flag": 1, "steps": 4, "a": "SL_S", "b": "S___"}


def test_cstring_stops_at_nul():
    assert ap.cstring(_image(), ap.LOAD_BASE + 0x10) == "SL_S"


def test_corpus_matches_requires_gate_pan_and_no_accent():
    rows = [{"preset": 3, "steps": 4, "gate": "EQE.", "pan": "CLRC"}]
    base = {"steps": 4, "gate": "EQE.", "accent": "....", "pan": "CLRC"}
    edited = {**base, "pan": "CCCC"}
    accented = {**base, "accent": ">..."}
    patterns = [("a", "A", base), ("b", "B", edited), ("c", "C", accented)]
    (m,) = ap.corpus_matches(rows, patterns)
    assert m["exact"] == ["a/A"]
    assert m["gate_only"] == ["a/A", "b/B", "c/C"]


def test_rust_rows_formats_the_engine_table():
    rows = [{"gate": "EQ", "pan": "CL"}]
    assert ap.rust_rows(rows) == '    preset("EQ", "CL"),\n'


def test_committed_table_is_well_formed():
    rows = ap.read_csv(CSV)
    assert [r["preset"] for r in rows] == list(range(29))
    assert rows[0]["gate"] == "E" * 8 and rows[0]["pan"] == "C" * 8
    for r in rows:
        assert r["steps"] in (8, 16)
        assert len(r["gate"]) == len(r["pan"]) == r["steps"]
        assert set(r["gate"]) <= set(ap.GATE_CHARS)
        assert set(r["pan"]) <= {"C", "L", "R"}
        assert set(r["gate"]) != {"."}


def test_committed_table_pins_factory_confirmed_presets():
    rows = {r["preset"]: r for r in ap.read_csv(CSV)}
    # Held unedited by factory programs (research note): presets 13 and 25, pan word included.
    assert rows[13]["gate"] == "QEEEEEQEQEEEEEQE" and rows[13]["pan"] == "LLRLLRLLRRLRRLRR"
    assert rows[25]["gate"] == "EE.EEEEEEEEE.EEE" and rows[25]["pan"] == "CLCCRLRCRLRCCLCC"


def test_main_table_and_rust_use_recovered_rows(monkeypatch, capsys):
    row = {
        "preset": 1,
        "steps": 2,
        "gate": "EQ",
        "pan": "CL",
        "accent_word": 0,
        "name": "N",
        "flag": 0,
        "a": "SL",
        "b": "",
    }
    monkeypatch.setattr(ap, "recover", lambda *_a: [row])
    assert ap.main(["rust", "x.bin"]) == 0
    assert capsys.readouterr().out == '    preset("EQ", "CL"),\n'
    assert ap.main(["table", "x.bin"]) == 0
    assert "1,2,EQ,CL,0,N" in capsys.readouterr().out
