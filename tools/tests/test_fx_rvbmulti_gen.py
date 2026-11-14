"""Tests for the multi-tap reverb model chain: fx_rvbmulti_gen, _ops, _check, _rust_gen and
the generated fx_model_rvbmulti."""

import ast

import fx_leslie_gen as lg
import fx_model_rvbmulti as model
import fx_rvbmulti_check as chk
import fx_rvbmulti_gen as gen
import fx_rvbmulti_ops as ops
import fx_rvbmulti_rust_gen as rgen


def parse(text):
    stp = lg.Step()
    for part in lg.split_top(text):
        lg.parse_part(part, stp)
    return stp.src(0x120000)


def test_packed_float_and_bit_forms_are_generated():
    assert "fpack_p(r(0))" in parse("r0=fpack f0")
    assert "funpack_p(r(2))" in parse("r2=funpack f2")
    assert "ibtgl(r(4),imm(0))" in parse("r4=btgl r4 by 0")
    assert "fdep_or(r(0),r(2),imm(16),imm(16))" in parse("r0=r0 or fdep r2 by 0x10:0x10")
    assert "fext_p(r(0),imm(16),imm(16),imm(0))" in parse("r2=fext r0 by 0x10:0x10")


def test_if_not_sz_becomes_a_not_zero_condition_and_the_call_becomes_the_core():
    lst = {
        0x10: (2, "r4=btgl r4 by 0"),
        0x12: (2, "if ne r5=dm(0x1,i3)"),
        0x14: (1, "call 0x120d8a"),
    }
    src = "\n".join(gen.gen({k: (n, t) for k, (n, t) in lst.items()}))
    assert "def core(m: Machine)" in src and "def wrapper(m: Machine)" in src
    assert "from fx_rvbmulti_ops import *" in src


def test_ibtgl_flips_one_bit_per_element():
    assert ops.ibtgl([0, 1], [0, 0]) == [1, 0]
    assert ops.ibtgl([0x80000000, 0], [31, 31]) == [0, 0x80000000]


def test_the_generated_model_has_the_two_routines_and_the_loop():
    tree = ast.parse(open(model.__file__, encoding="utf-8").read())
    funcs = {n.name: n for n in tree.body if isinstance(n, ast.FunctionDef)}
    assert set(funcs) == {"core", "wrapper"}
    loops = [n for n in ast.walk(funcs["core"]) if isinstance(n, ast.For)]
    assert len(loops) == 1 and loops[0].iter.args[0].value == 7  # `lcntr = 7`, 0x120F4D
    calls = [
        n
        for n in ast.walk(funcs["wrapper"])
        if isinstance(n, ast.Call) and isinstance(n.func, ast.Name) and n.func.id == "core"
    ]
    assert len(calls) == 1  # `CALL 0x120D8A` at 0x1210B4


def test_the_checker_noise_is_deterministic_and_memory_beyond_the_image_is_zero():
    assert chk.frames_noise(3) == chk.frames_noise(3)
    assert len(chk.frames_noise(2)[0]) == 8
    mem = chk.Mem(b"\x01\x00\x00\x00", {5: 7})
    assert mem[0] == 1 and mem[5] == 7 and mem[0x08020000] == 0


def test_the_committed_rust_files_are_what_the_generator_prints():
    for name, text in rgen.files().items():
        path = rgen.FX_DIR / name
        assert path.read_text(encoding="utf-8") == text, f"stale {name}: fx_rvbmulti_rust_gen.py"


def test_rust_expressions_for_the_packed_forms():
    e = rgen.lrg.Emit()

    def tr(src):
        return e.expr(ast.parse(src, mode="eval").body)

    assert tr("fpack_p(r(0))") == "m.fpack_p(m.r(0))"
    assert tr("fdep_or(r(0), r(2), imm(16), imm(16))").startswith("m.fdep_or(m.r(0), m.r(2)")
    assert tr("ibtgl(r(4), imm(0))") == "m.ibtgl(m.r(4), [V::W(0x0u32); 2])"
