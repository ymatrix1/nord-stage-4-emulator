"""Tests for tools/fx_leslie_gen.py, fx_leslie_ops.py, fx_leslie_check.py and fx_model_leslie.py."""

import inspect

import fx_leslie_check as lc
import fx_leslie_gen as g
import fx_leslie_ops as ops
import fx_model_leslie as model


def parse(text):
    stp = g.Step()
    for part in g.split_top(text):
        g.parse_part(part, stp)
    return stp.src(0x120000)


def test_split_top_keeps_commas_inside_parentheses():
    assert g.split_top("f12=f9*f12, dm(0xfffffffc,i3)=r12") == [
        "f12=f9*f12",
        "dm(0xfffffffc,i3)=r12",
    ]


def test_multifunction_reads_old_registers_and_broadcasts_i1_and_i9_loads():
    assert "w={12:fmul(r(0),r(4)),8:fadd(r(8),r(12)),0:ld(I(7))}" in parse(
        "f12=f0*f4, f8=f8+f12, r0=dm(i7,m2)"
    )
    assert "ldb(A(1,-13))" in parse("r1=dm(0xfffffff3,i1)")
    assert "ldb(I(9))" in parse("r4=pm(i9,m9)")
    assert "ld(A(0,32))" in parse("r0=dm(0x20,i0)")


def test_long_word_and_dag_forms():
    assert 'd={"i0":mem[10],"i1":mem[(10)+1]}' in parse("i0=dm(0xa) (LW)")
    assert "ir['i6'],ir['i7']" in parse("dm(0xb485a)=i6 (LW)")
    assert 'd={"i7":modi(7,65)}' in parse("i7=modify (i7,0x41)")
    assert "wx={9:y[8]},wy={8:x[9]}" in parse("r9<->s8")


def test_conditionals_and_flags():
    assert "cond_ne(m,r(10),r(8))" in parse("if ne r10=r8")
    assert "fl=flags_of" in parse("r3=r3 and r9")
    assert "fl=" not in parse("f0=f0+f1")


def test_lazy_mem_reads_the_image_under_the_overlay():
    mem = ops.LazyMem(bytes([1, 0, 0, 0, 2, 0, 0, 0]), {1: 9})
    assert mem[0] == 1 and mem[1] == 9
    assert ops.ftrunc([ops.fx_pe.Fraction(-3, 2)] * 2) == [0xFFFFFFFF, 0xFFFFFFFF]


def test_conditions_select_per_element():
    m = ops.LMachine({}, {})
    m.az = [True, False]
    assert ops.cond_ne(m, [1, 1], [2, 2]) == [1, 2]
    m.an = [True, False]
    assert ops.cond_ge(m, [1, 1], [2, 2]) == [1, 2]


def test_generated_model_covers_the_listing_without_unsupported_forms():
    src = inspect.getsource(model)
    assert "UNSUPPORTED" not in src
    assert src.count("step(0x") > 1400 and "def callee" in src and "def main" in src


def test_check_inputs_are_four_seeded_words_per_call():
    a = lc.input_words(3)
    assert a == lc.input_words(3) and all(len(w) == 4 for w in a)
    assert lc.rotor_words()[lc.LIMIT] == 0x7FFFFFFF
