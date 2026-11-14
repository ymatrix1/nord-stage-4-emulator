"""Tests for tools/fx_leslie_rust_gen.py, fx_leslie_words.py and fx_leslie_golden.py."""

import ast

import fx_leslie_golden as lg
import fx_leslie_run as lr
import fx_leslie_rust_gen as rg
import fx_leslie_words as lw


def expr(text):
    return rg.Emit().expr(ast.parse(text, mode="eval").body)


def test_committed_rust_steps_are_not_stale():
    produced = rg.files()
    produced.pop("_main_parts")
    for name, text in produced.items():
        path = rg.FX_DIR / name
        assert path.read_text(encoding="utf-8") == text, f"{name} is stale: rerun the generator"
        assert len(text.splitlines()) < 400, name


def test_expressions_translate_to_the_register_machine():
    assert expr("fmul(r(0), r(4))") == "m.fmul(m.r(0), m.r(4))"
    assert expr("ldb(A(1, -13))") == "m.ldb(m.a(1, -13))"
    assert expr("mem[(740554) + 1]") == "m.rd((0xb4cca_u32).wrapping_add(0x1u32))".replace(
        "0xb4cca_u32", "0xb4cca" + "u32"
    )
    assert expr("modi(7, -2)") == "m.modimm(7, 0xfffffffeu32)"
    assert expr("ir['i6']") == "m.i[6]"
    assert expr("[ir['i0']] * 2") == "[V::W(m.i[0]); 2]"
    assert expr("cond_ne(m, r(10), r(8))") == "m.cond_ne(m.r(10), m.r(8))"


def test_data_windows_are_disjoint_and_split_into_three_files():
    spans = sorted(lw.WINDOWS)
    assert all(a[1] < b[0] for a, b in zip(spans, spans[1:], strict=False))
    assert lw.SPLIT[0][0] == 0 and lw.SPLIT[-1][1] == len(lw.WINDOWS)
    assert lw.fnv([0, 0, 0, 0]) == lw.fnv([0] * 4) != lw.fnv([1, 0, 0, 0])


def test_rust_window_formatting_and_lcg_noise_is_exact():
    text = "\n".join(lw.rust_window("W0", 0, list(range(10))))
    assert "const W0: [u32; 10]" in text and "0x8, 0x9," in text
    rows = lr.lcg_words(3)
    assert rows == lr.lcg_words(3) and all(len(r) == 4 for r in rows)
    assert all(abs(v) <= 0.25 and v * 33554432.0 == int(v * 33554432.0) for r in rows for v in r)


def test_golden_rust_block_has_the_hash_and_first_words():
    words = list(range(4 * lg.KEEP + 8))
    text = lg.rust("SLOW", 1, words)
    assert f"SLOW_FNV: u64 = {lw.fnv(words):#x}" in text
    assert f"[u32; {4 * lg.KEEP}]" in text
