"""Tests for the Mod 2 model tools: tools/fx_listing_gen.py, fx_model_mod2.py (and its generated
fx_model_mod2_gen.py), fx_mod2_rust_gen.py and fx_golden_mod2_rest.py."""

import ast
import random
from pathlib import Path

import fx_golden_mod2_rest as gr
import fx_leslie_gen as lg
import fx_listing_gen as lgen
import fx_mod2_rust_gen as rg
import fx_model_mod2 as mm
import fx_model_mod2_gen as gen
import fx_models
import fx_oracle as fo
from fx_pe import M32

LISTING = """\
00000000  713e 89bf  i3=i1
00000002  0180 1601  r6=r0+r1
00000004  0f8f ffe1  r15=0xffffffe1
00000006  7048 013f  if av r2=r4
00000008  0abe  rts
"""


def test_the_listing_translator_handles_flags_and_conditions(tmp_path, monkeypatch):
    monkeypatch.setattr(lg, "AVAC", True)
    path = tmp_path / "l.txt"
    path.write_text(LISTING, encoding="utf-8")
    lst = lg.parse_listing(path)
    src = "\n".join(lg.gen_routine(lst, 0, 8, "demo"))
    assert "iflags('add'" in src and "cond_av(m" in src
    assert "UNSUPPORTED" not in src
    assert 0x120000 + 8 == 0x120008


def test_the_translator_knows_shifts_max_min_recips_and_exchanges():
    stp = lg.Step()
    for text in (
        "r0=lshift r0 by 0x1",
        "r1=max(r3,r1)",
        "f0=recips f4",
        "if eq r2<->s0",
    ):
        for part in lg.split_top(text):
            lg.parse_part(part, stp)
    assert stp.w[0].startswith("frecips") and "ishl" not in stp.w[0]
    assert stp.w[1].startswith("imax")
    assert set(stp.wx) == {2} and set(stp.wy) == {0}


def test_the_generated_routines_are_all_registered_and_listed():
    assert set(mm.SPECS) <= set(lgen.ROUTINES)
    for name, spec in mm.SPECS.items():
        assert (spec.pc, spec.end) == lgen.ROUTINES[name], name
        assert callable(getattr(gen, name))
    assert set(fx_models.routines()) >= set(mm.SPECS)


def test_the_models_run_on_their_scenarios_and_write_state_back():
    for name, spec in mm.SPECS.items():
        rt = mm.routine(spec)
        case = rt.scenario(random.Random(5))
        mem, written = fo.model_run(rt, case)
        assert written, name
        assert all(a in mem for a in written)
        assert any(mem[a] != case.get(a) for a in written), name


def test_the_vibe_phase_is_an_integer_sum_through_the_self_pointer():
    spec = mm.SPECS["vibe"]
    case = mm.finite_scenario(spec, random.Random(4))
    mem, _ = fo.model_run(mm.routine(spec), dict(case))
    assert mem[mm.CTX - 0x19] == (case[mm.CTX - 0x19] + case[mm.CTX - 0x17]) & M32


def test_scenarios_are_deterministic_and_finite():
    spec = mm.SPECS["phas0"]
    a = mm.finite_scenario(spec, random.Random(9))
    b = mm.finite_scenario(spec, random.Random(9))
    assert a == b


def test_the_generated_model_is_current_when_the_listing_is_available():
    listing = Path("core2.txt")
    if listing.exists():  # the listing is local-only (firmware)
        assert lgen.render(listing) == lgen.OUT.read_text(encoding="utf-8")


def test_the_rust_generator_output_covers_the_routines_and_is_syntax_valid_python_ast():
    produced = rg.files()
    assert set(produced) == {v[0] for v in rg.FILES.values()}
    for name, text in produced.items():
        assert "pub(super) fn run<M: Mem>" in text, name
        assert text.count("m.apply(") > 40, name
    tree = ast.parse(rg.MODEL.read_text(encoding="utf-8"))
    fns = {n.name: n for n in tree.body if isinstance(n, ast.FunctionDef)}
    keep = rg.kept_flag_steps(fns["flang"].body)
    assert len(keep) == 1, "one flag producer feeds the single IF of the flanger"
    assert len(rg.kept_flag_steps(fns["vibe"].body)) == 0


def test_the_committed_rust_files_match_the_generator():
    for name, text in rg.files().items():
        committed = (rg.FX_DIR / name).read_text(encoding="utf-8")
        assert committed.replace("\r\n", "\n") == text, name


def test_the_golden_vectors_are_deterministic_and_complete():
    a, b = gr.golden("vibe"), gr.golden("vibe")
    assert a == b and "ring_diff" in a and "ctx_out" in a
    assert gr.golden("flang") != a
    assert gr.CTX_N == 0x4B
