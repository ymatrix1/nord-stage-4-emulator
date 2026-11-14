"""Tests for tools/fx_mod1_laws.py, tools/fx_golden_mod1.py and the Mod 1 model ops."""

import struct

import fx_golden_mod1 as golden
import fx_leslie_gen as lg
import fx_leslie_ops as ops
import fx_mod1_laws as laws
import fx_model_mod2 as mm


def _image(words: dict[int, int], size: int = 0x100000) -> bytes:
    img = bytearray(4 * size)
    for addr, w in words.items():
        struct.pack_into("<I", img, 4 * addr, w)
    return bytes(img)


def test_a_row_is_the_image_under_the_boot_words():
    base = laws.BASES["pump"]
    image = _image({base - 22: 0x7FFFFFFF, base + 4: 0x7FFFFFFF})
    boot = {base - 22: 0x7F000000, base - 29: base - 31}
    rows = {(n, o): (w, s) for n, o, w, s in laws.row_words(image, boot) if n == "pump"}
    assert rows[("pump", -22)] == (0x7F000000, "boot"), "the boot word wins"
    assert rows[("pump", -29)] == (base - 31, "boot"), "a pointer the image lacks"
    assert rows[("pump", 4)] == (0x7FFFFFFF, "image")


def test_boot_words_keep_the_last_value_per_word(tmp_path):
    script = tmp_path / "s.script"
    script.write_text("0 0x10 0x1\n0 0x10 0x2\n0 0x11 0x3\n", encoding="utf-8")
    assert laws.boot_words(script) == {0x10: 2, 0x11: 3}


def test_every_mod1_type_has_type_words_and_both_laws():
    tw, lw = laws.type_words(), laws.laws()
    for t in laws.TYPES:
        assert tw[t], t
        assert lw[t]["Amt"] or lw[t]["Rate"], t
        for kind in ("Amt", "Rate"):
            for _off, (_flt, words) in lw[t][kind].items():
                assert len(words) == len(laws.m2.SAMPLES)


def test_the_committed_rows_and_rust_are_current():
    assert laws.read_rows(), "the rows CSV is not empty"
    assert laws.main(["rust", "--check"]) == 0
    names = {n for n, *_ in laws.read_rows()}
    assert names == set(laws.BASES)


def test_the_boot_log_supplies_the_self_pointers_and_the_all_pass_gains():
    rows = {(n, o): w for n, o, w, _ in laws.read_rows()}
    assert rows[("rm", -29)] == laws.BASES["rm"] - 31
    assert rows[("wah", -29)] == laws.BASES["wah"] - 31
    assert rows[("pump", -29)] == laws.BASES["pump"] - 31
    assert rows[("ens", -5)] == laws.BASES["ens"] - 8
    assert rows[("spin", -8)] == laws.BASES["spin"] - 10
    assert rows[("phas1", -26)] == 0x3EA00000, "all-pass gain 0.3125"


def test_the_new_routines_have_harness_specs():
    for name in ("rm", "wah", "awah", "pump"):
        spec = mm.SPECS[name]
        assert spec.fn.__name__ == name
        assert name in golden.NAMES


def test_the_translator_knows_ashift_uui_icomp_and_the_float_conditions():
    stp = lg.Step()
    assert lg.compute("ashift r1 by 0xffffffff", "r1", stp) and "iashift" in stp.w[1]
    stp = lg.Step()
    assert lg.compute("r0*r1(uui)", "r4", stp) and stp.w[4] == "imul(r(0),r(1))"
    stp = lg.Step()
    lg.parse_part("comp(r2,r3)", stp)
    assert stp.fl.startswith("icomp")
    stp = lg.Step()
    lg.parse_part("comp(f2,f3)", stp)
    assert stp.fl.startswith("comp")
    for cond in ("gt", "lt", "le"):
        stp = lg.Step()
        lg.parse_part(f"if {cond} r3=m0", stp)
        assert f"cond_{cond}" in stp.w[3]


def test_the_model_ops_follow_the_oracle_semantics():
    m = ops.LMachine({}, {})
    m.an, m.az = [True, False], [False, True]
    old, new = [1, 1], [9, 9]
    assert ops.cond_gt(m, old, new) == [1, 1], "negative or zero is not GT"
    assert ops.cond_lt(m, old, new) == [9, 1], "negative and not zero"
    assert ops.cond_le(m, old, new) == [9, 9]
    assert ops.iashift([[0x80000000, 4]][0], [31, 0xFFFFFFFE]) == [0, 1], "right shift by 2"
    assert ops.iashift([0x80000000, 0], [0xFFFFFFFF, 0]) == [0xC0000000, 0], "sign extends"
    assert ops.imul([0xFFFFFFFF, 3], [2, 5]) == [0xFFFFFFFE, 15], "low 32 bits"
    an, az = ops.icomp([0xFFFFFFFF, 7], [1, 7])
    assert an == [True, False] and az == [False, True], "signed compare"
