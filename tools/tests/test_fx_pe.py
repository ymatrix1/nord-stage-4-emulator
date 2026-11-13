"""Tests for tools/fx_pe.py (pair-register machine and SHARC arithmetic helpers)."""

from fractions import Fraction

import fx_biquad_check as fb
import fx_pe as pe
import pytest

F = fb.bits


def test_float_and_word_views_of_a_register():
    assert pe.fl(F(1.5)) == Fraction(3, 2)
    assert pe.fl(0x00400000) == 0  # zero exponent field flushes to zero
    assert pe.wd(Fraction(3, 2)) == F(1.5)
    assert pe.wd(Fraction(0)) == 0
    # a store keeps the upper 32 bits: a 32-bit mantissa truncates, it does not round
    q = Fraction(1) + Fraction(1, 2**24) + Fraction(1, 2**30)
    assert pe.wd(q) == F(1.0)
    with pytest.raises(ValueError):
        pe.fl(None)
    with pytest.raises(ValueError):
        pe.wd(None)


def test_float_pair_operations_round_like_the_register_file():
    a, b = [F(1.0), F(-2.0)], [F(0.5), F(0.25)]
    assert pe.fadd(a, b) == [Fraction(3, 2), Fraction(-7, 4)]
    assert pe.fsub(a, b) == [Fraction(1, 2), Fraction(-9, 4)]
    assert pe.fmul(a, b) == [Fraction(1, 2), Fraction(-1, 2)]
    assert pe.favg(a, b) == [Fraction(3, 4), Fraction(-7, 8)]
    assert pe.fmax(a, b) == [Fraction(1), Fraction(1, 4)]
    assert pe.fmin(a, b) == [Fraction(1, 2), Fraction(-2)]
    assert pe.fabs(a) == [Fraction(1), Fraction(2)]
    assert pe.fclip([F(3.0), F(-3.0)], [F(2.0), F(2.0)]) == [Fraction(2), Fraction(-2)]
    assert pe.fclip([F(1.0), F(-1.0)], [F(2.0), F(2.0)]) == [Fraction(1), Fraction(-1)]
    # a product keeps 32 mantissa bits (round to nearest even), not the 24 of a single
    x = Fraction(1) + Fraction(1, 2**23)
    assert pe.fmul([x, x], [x, x])[0] == pe.ext40(x * x) != x * x


def test_recips_matches_the_seed_table():
    assert pe.recips_value(F(1.0)) == Fraction(1, 2) * (1 + Fraction(0xFF, 256))
    assert pe.recips_value(F(2.0)) == Fraction(1, 4) * (1 + Fraction(0xFF, 256))
    assert pe.recips_value(F(-1.0)) == -pe.recips_value(F(1.0))
    assert pe.recips_value(F(1.9999)) == Fraction(1, 2)
    assert pe.recips_value(Fraction(2) ** 126) == 0
    # the seed is within 8 bits of the reciprocal over the whole octave
    for k in range(128):
        x = Fraction(128 + k, 128)
        assert abs(pe.recips_value(x) * x - 1) < Fraction(1, 128)
    # two Newton steps `s (2 - x s)` converge to full precision
    x = pe.ext40(Fraction(7, 10))
    s = pe.recips_value(x)
    for _ in range(2):
        s = pe.ext40(s * pe.ext40(2 - pe.ext40(x * s)))
    assert abs(s * x - 1) < Fraction(1, 2**20)


def test_fix_float_and_shift_helpers():
    assert pe.fix_word(Fraction(3, 2), 0) == 2  # ties to even
    assert pe.fix_word(Fraction(5, 2), 0) == 2
    assert pe.fix_word(Fraction(-3, 2), 0) == 0xFFFFFFFE
    assert pe.fix_word(Fraction(1, 2), 8) == 128
    assert pe.fix_word(Fraction(2) ** 40, 0) == 0xFFFFFFFF
    assert pe.float_by(-3, -1) == Fraction(-3, 2)
    assert pe.lshift(1, 4) == 16 and pe.lshift(0x80000000, 1) == 0
    assert pe.lshift(0x100, 0xFFFFFFFC) == 0x10  # negative count shifts right
    assert pe.lshift(1, 40) == 0


def test_field_extract_and_deposit():
    # `r1 = fext r0 by 24:8` is the top byte; `r0 = fdep r0 by 7:24` deposits 24 bits at bit 7
    assert pe.extract(0xAB123456, 24, 8, False) == 0xAB
    assert pe.extract(0xAB123456, 24, 8, True) == 0xFFFFFFAB
    assert pe.extract(0x00200000, 22, 10, True) == 0
    assert pe.extract(0x80000000, 22, 10, True) == 0xFFFFFE00
    assert pe.deposit(0x00FFFFFF, 7, 24) == 0x7FFFFF80
    assert pe.deposit(0x12345678, 16, 16) == 0x56780000
    # register form: bits 5:0 of the control word are the position, 11:6 the length
    assert pe.fdep_r([0xFFFF, 0xFFFF], [(22 << 6) | 3] * 2) == [0xFFFF << 3] * 2
    assert pe.fext_r([0x12345678, 0x12345678], [(8 << 6) | 4] * 2) == [0x67] * 2


def test_short_float_packing():
    assert pe.fpack(F(1.0)) == 0x3800  # programming reference example
    assert pe.funpack(0x3800) == F(1.0)
    assert pe.fpack(F(1.0e10)) == 0x7FFF
    assert pe.fpack(F(-1.0e10)) == 0xFFFF
    assert pe.fpack(F(1.0e-9)) == 0
    for v in (0.02, 0.5, 1.0, -1.7, 100.0, -0.0623):
        back = pe.fl(pe.funpack(pe.fpack(F(v))))
        assert abs(back - Fraction(v)) <= abs(Fraction(v)) * Fraction(1, 2**11)
    # the smallest short float is a denormal that unpacks to the normal single 2^-17
    assert pe.funpack(1) == 110 << 23 and pe.fpack(110 << 23) == 1


def test_swap_comp_and_conditional_move():
    assert pe.swap([1, 2]) == [2, 1]
    assert pe.imm(7) == [7, 7]
    flags = pe.comp([F(1.0), F(3.0)], [F(2.0), F(3.0)])
    assert flags == ([True, False], [False, True])
    assert pe.cond_ge(flags, ["new", "new"], ["old", "old"]) == ["old", "new"]


def test_machine_commits_a_step_in_parallel_and_stores_pairs():
    mem = {0x100: F(1.0), 0x101: F(2.0)}
    m = pe.Machine(mem, {"i1": 0x100})
    m.step(0, w={0: m.ld(m.a("i1"))})
    assert m.r(0) == [F(1.0), F(2.0)]
    # r1 = r0 + r0 and a store of r0 in the same step: both read the old register
    m.step(1, w={0: pe.fadd(m.r(0), m.r(0))}, st=[(0x110, m.r(0))])
    assert mem[0x110] == F(1.0) and mem[0x111] == F(2.0)
    assert m.r(0) == [Fraction(2), Fraction(4)]
    m.step(2, wx={1: 5}, wy={1: 6}, d={"i1": 0x105})
    assert m.r(1) == [5, 6] and m.ir["i1"] == 0x105
    assert m.ldb(0x100) == [F(1.0), F(1.0)]
    assert m.written == {0x110, 0x111}


def test_machine_compares_against_a_trace():
    row = {"pc": 7, "r": [0] * 16, "s": [0] * 16, "i": [0] * 16, "m": [0] * 16}
    row["r"][2], row["i"][3] = 9, 0x40
    ok = pe.Machine({}, {"i3": 0x40}, [row])
    ok.x[2] = 9
    ok.step(7)
    bad_reg = pe.Machine({}, {"i3": 0x40}, [row])
    bad_reg.x[2] = 8
    with pytest.raises(AssertionError, match="R2"):
        bad_reg.step(7)
    bad_pc = pe.Machine({}, {}, [row])
    with pytest.raises(AssertionError, match="oracle pc"):
        bad_pc.step(8)
    bad_dag = pe.Machine({}, {"i3": 0x41}, [row])
    with pytest.raises(AssertionError, match="i3"):
        bad_dag.step(7)


def test_missing_memory_reports_unset_reads():
    mem = pe.Missing({1: 5})
    assert mem[1] == 5
    with pytest.raises(KeyError, match="0x2"):
        mem[2]
    assert mem.reads == [2]
    assert pe.Missing(default=9)[3] == 9


def test_parse_trace_splits_calls():
    words = " ".join(f"{k:x}" for k in range(96))
    text = (
        f"trace 1206b0 {words}\ntrace 1206b2 {words}\ncall 0: 2 steps\ntrace 5 {words}\ncall 1: 1\n"
    )
    first, second = pe.parse_trace(text, 0), pe.parse_trace(text, 1)
    assert [r["pc"] for r in first] == [0x1206B0, 0x1206B2]
    assert second[0]["pc"] == 5
    assert first[0]["r"][0] == 0 and first[0]["s"][0] == 16 and first[0]["b"][15] == 95
