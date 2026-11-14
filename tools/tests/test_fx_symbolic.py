"""Tests for tools/fx_symbolic.py (symbolic run of the routine models)."""

import fx_model_resonant
import fx_models
import fx_pe
import fx_symbolic as sy


def test_expression_nodes_are_shared_and_render_with_names():
    a, b = sy.Sym.mk("in", name="a"), sy.Sym.mk("in", name="b")
    s = (a + b) * (a + b)
    assert s.args[0] is s.args[1]  # interned: one node for both factors
    defs, roots = sy.render({"y": s, "z": abs(-(a + b))})
    assert defs == ["t0 = a + b"] and roots == {"y": "(t0 * t0)", "z": "(abs((-t0)))"}
    assert sy.Sym.lift(3).op == "const"


def test_the_resonant_routine_reduces_to_its_documented_recurrence():
    defs, roots = sy.run("resonant")
    text = "\n".join(defs)
    # the integrator is clipped to the limit word c[-0xe] and fed by the smoothed gains
    assert "clip(" in text and "c[-0xe]" in text
    # the stored first-frame left output is x0 plus a feed term: y + F d
    assert roots["x0'"].startswith("(t") and roots["c[-0x1e]'"] == "t1"
    assert len([k for k in roots if k.startswith("c[")]) == 18


def test_delay_and_lfo_keep_integers_concrete_and_loads_opaque():
    defs, roots = sy.run("delay")
    text = "\n".join(defs)
    assert "load(" in text and "fext_p(" in text and "ffix(" in text and "fpack(" in text
    assert "if_ge(" in text and "recips(" in text
    assert "m[0xc02c8]'" in roots
    defs, roots = sy.run("lfo")
    assert any("ffix(" in d for d in defs) and "c[-0x1f]'" in roots
    defs, roots = sy.run("allpass")
    assert "x3'" in roots


def test_the_patches_are_undone_after_a_symbolic_run():
    sy.run("resonant4")
    assert fx_pe.fl(0x3F800000) == 1 and fx_pe.fmul([0x40000000] * 2, [0x40400000] * 2)[0] == 6
    assert fx_model_resonant.fadd is fx_pe.fadd  # re-imported names point at the real functions
    rt = fx_models.routines()["resonant"]
    assert fx_models.digest(rt) == fx_models.digest(rt)
    assert fx_models.main(["digest", "resonant"]) == 0


def test_command_line_prints_definitions_and_stored_words(capsys):
    assert sy.main(["resonant4", "--seed", "2"]) == 0
    out = capsys.readouterr().out
    assert "x7' = " in out and "t0 = " in out
