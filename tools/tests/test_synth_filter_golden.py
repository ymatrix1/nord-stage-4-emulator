"""Tests for tools/synth_filter_golden.py: the Rust data files hold what the generator prints."""

import os
import re
import struct
import sys
from pathlib import Path

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import sharc40 as s  # noqa: E402
import synth_filter_golden as g  # noqa: E402
import synth_filter_model as m  # noqa: E402

SYNTH = Path(__file__).resolve().parents[2] / "crates/nord-core/src/synth"
HEX = re.compile(r"0x[0-9a-fA-F]{8}")


def as_f32(word: int) -> float:
    """The float32 whose bits are `word`."""
    return struct.unpack("<f", struct.pack("<I", word))[0]


def words(text: str) -> list[int]:
    """All 32-bit hex words of a Rust source (rustfmt may reflow the layout, not the words)."""
    return [int(w, 16) for w in HEX.findall(text)]


def const_words(text: str, name: str) -> list[int]:
    """Hex words of the `const NAME: ... = [..];` item."""
    start = text.index(f"const {name}:")
    return words(text[start : text.index("];", start)])


def test_rust_files_hold_the_generated_words():
    # `cargo fmt` reflows the generated text, so compare the words, not the layout
    for name, emit in (
        ("fw_filter_tables.rs", g.emit_tables),
        ("fw_env_rates.rs", g.emit_rates),
        ("tests_fw_filter_golden.rs", g.emit_golden),
    ):
        want = emit()
        got = (SYNTH / name).read_text(encoding="utf-8")
        for const in re.findall(r"pub\(super\) const (\w+): \[", want):
            assert const_words(got, const) == const_words(want, const), (
                f"{name}: {const} differs; regenerate with tools/synth_filter_golden.py"
            )


def test_tables_have_129_entries_and_the_documented_anchors():
    a, c, b = g.filter_tables()
    assert len(a) == len(b) == len(c) == 129
    assert s.ld(a[64]) == s.ld(0x3F7D70A4)  # 0.99, the oracle row of the model tests
    assert s.ld(b[0]) == 2
    att, dec = g.rate_tables()
    assert len(att) == len(dec) == 129


def test_generation_is_deterministic():
    assert g.emit_golden() == g.emit_golden()
    assert g.input_words() == g.input_words()


def test_golden_rows_use_the_bit_exact_model():
    a, c, b = g.filter_tables()
    w28, r = g.ROWS[0]
    res = g.run_row(w28, r, a, b, c)
    assert res["lp"][0] == m.coefficients(w28, r, a, b)
    assert res["bp"][0] == m.coefficients(w28, r, c, b)
    assert res["lp"][1][0] == 0  # no feed through, like the oracle
    assert len(res["hp"][1]) == g.SAMPLES


def test_closed_form_response_is_cin_far_below_the_cutoff():
    a, _, b = g.filter_tables()
    w28, r = g.ROWS[0]
    low = g.response(w28, r, a, b)
    assert len(low) == len(g.RESPONSE_FREQS)
    # 0.005 cycles per sample is far below the 0.0928 cutoff: the gain is about cin = 0.99
    assert abs(as_f32(low[0]) - 0.99) < 0.02
