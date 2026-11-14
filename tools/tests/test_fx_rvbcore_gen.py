"""Tests for tools/fx_rvbcore_gen.py: the generated Rust of the reverb core and its words."""

import ast

import fx_rvbcore_gen as gen


def test_the_committed_rust_files_are_what_the_generator_prints():
    for path, text in gen.outputs().items():
        assert path.read_text() == text, f"stale {path.name}: python tools/fx_rvbcore_gen.py"


def test_the_instruction_counts_match_the_model_source():
    _, n_run, n_pass = gen.steps_rust()
    tree = ast.parse(gen.MODEL.read_text())
    funcs = {n.name: n for n in tree.body if isinstance(n, ast.FunctionDef)}

    def count(fn):
        return sum(
            1
            for n in ast.walk(funcs[fn])
            if isinstance(n, ast.Call) and isinstance(n.func, ast.Name) and n.func.id == "step"
        )

    assert n_run == count("run") and n_pass == count("_body") == 16


def test_expressions_translate_to_the_machine_api():
    e = gen.Emit()

    def tr(src):
        return e.expr(ast.parse(src, mode="eval").body)

    assert tr("fmul(r(0), r(4))") == "m.fmul(m.r(0), m.r(4))"
    assert tr("ld(A(3, -26))") == "m.ld(m.a(3, -26))"
    assert tr("[I(2)]*2") == "[V::W(m.i[2]); 2]"
    assert tr("wd(x[10])") == "m.rg[10][0].wd()"
    assert tr("y[3]") == "m.rg[3][1]"
    assert tr("mem[A(1, -13)]") == "m.rd(m.a(1, -13))"
    assert tr("mod(6, 2)") == "m.modi(6, 2)"


def test_a_step_commits_after_evaluating_every_right_hand_side():
    call = ast.parse(
        'step(0x1, w={3: fadd(r(1), r(2))}, st=[(A(3, 4), r(3))], d={"b2": 0x6})', mode="eval"
    ).body
    line = gen.Emit().step(call)
    assert line.index("let w3") < line.index("let (sa0, sv0)") < line.index("m.rg[3] = w3;")
    assert "m.b[2] = d_b2; m.i[2] = d_b2;" in line, "a write to B also loads I"
    assert line.endswith("m.tick(0x1); }")


def test_the_pointer_words_become_regions_and_the_taps_are_the_type_words():
    text = gen.words_rust()
    assert "0x0802f000" not in text and "INSTANCE_PTRS: [(i8, u8); 11]" in text
    assert "(43, 0xfffff59e)" in text and "(48, 0xfffff01c)" in text
