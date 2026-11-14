"""Tests for tools/fx_mod2_tables.py and tools/fx_mod2_laws.py (derived Mod 2 data)."""

import struct

import fx_mod2_laws as laws
import fx_mod2_tables as tables
import fx_model_mod2 as mm


def _image(words: dict[int, int], size: int = 0x1000) -> bytes:
    img = bytearray(4 * size)
    for addr, w in words.items():
        struct.pack_into("<I", img, 4 * addr, w)
    return bytes(img)


def test_the_committed_tables_have_the_documented_shape():
    t = tables.load()
    assert len(t["ens"]) == 1025 and len(t["phas"]) == 258
    assert t["ens"][1024] == t["ens"][0], "guard copy of the first sample"
    assert t["ens"][255] == 0 and t["phas"][0] == 0x3A46B484 and t["phas"][129] == 0x39C6B484


def test_extract_reads_the_tables_at_their_addresses(monkeypatch):
    monkeypatch.setitem(tables.TABLES, "ens", (0x100, 3))
    monkeypatch.setitem(tables.TABLES, "phas", (0x200, 2))
    got = tables.extract(_image({0x100: 1, 0x102: 3, 0x201: 9}))
    assert got == {"ens": [1, 0, 3], "phas": [0, 9]}


def test_the_rust_tables_are_generated_from_the_csv(tmp_path):
    text = tables.rust(tables.load())
    assert "pub(super) const ENS_TABLE: [u32; 1025]" in text
    assert "pub(super) const PHAS_TABLE: [u32; 258]" in text
    assert tables.main(["rust", "--check"]) == 0, "the committed Rust file is current"
    path = tmp_path / "t.csv"
    tables.write_csv({"ens": [1, 2], "phas": [3]}, path)
    assert tables.load(path) == {"ens": [1, 2], "phas": [3]}


def test_the_instance_rows_are_the_non_zero_words_of_the_image():
    base = laws.BASES["flang"]
    rows = laws.row_words(_image({base - 4: 0xBF800000, base + 2: 2}, 0x100000))
    assert rows == [("flang", -4, 0xBF800000), ("flang", 2, 2)]


def test_the_committed_laws_have_eleven_samples_and_the_rust_is_current():
    lw = laws.laws()
    for t in laws.TYPES:
        for kind in ("Amt", "Rate"):
            assert lw[t][kind], (t, kind)
            for _off, (_flt, words) in lw[t][kind].items():
                assert len(words) == len(laws.SAMPLES)
    assert laws.main(["rust", "--check"]) == 0
    assert laws.read_rows(), "the rows CSV is not empty"


def test_flang_centre_plus_swing_is_minus_one_hundred_at_every_sample():
    lw = laws.laws()[2]
    for kind in ("Amt", "Rate"):
        swing, centre = lw[kind][-16][1], lw[kind][-15][1]
        for a, b in zip(swing, centre, strict=True):
            fa, fb_ = (struct.unpack("<f", struct.pack("<I", w))[0] for w in (a, b))
            assert abs(fa + fb_ + 100.0) < 1e-3


def test_the_ens_and_spin_scenarios_are_registered_and_deterministic():
    import random

    for name in ("ens", "spin"):
        spec = mm.SPECS[name]
        a = mm.finite_scenario(spec, random.Random(3))
        b = mm.finite_scenario(spec, random.Random(3))
        assert a == b
    assert len(mm.ens_table()) == 1025
