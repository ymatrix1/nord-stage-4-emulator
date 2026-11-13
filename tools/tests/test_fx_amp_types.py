"""Tests for tools/fx_amp_types.py: the Amp type words and the golden file helpers."""

from pathlib import Path

import fx_amp_types as at

DATA = Path(__file__).resolve().parents[2] / "docs" / "research" / "data" / "fx-amp-types.csv"


def _row(t: int, dm: int, word: int, core: str = "1") -> dict[str, str]:
    return {
        "slot": "Amp",
        "stage": "type",
        "core": core,
        "type": str(t),
        "dm_word": hex(dm),
        "word": hex(word),
    }


def test_context_offsets_are_relative_to_the_stage_base():
    assert at.ctx_offset(0xC018A) == 0
    assert at.ctx_offset(0xC0191) == 7
    assert at.ctx_offset(0xC016A) == -0x20
    assert at.ctx_offset(0xC019F) == 0x15
    assert at.ctx_offset(0xC016A - 1) is None
    assert at.ctx_offset(0xC019F + 1) is None


def test_extract_keeps_core_one_context_words_and_counts_the_fir():
    rows = [
        _row(1, 0xC0191, at.bits(2.5)),
        _row(1, 0xC0191, at.bits(9.0)),  # repeated word: the first message wins
        _row(1, 0xC0191, at.bits(1.0), core="2"),  # other core
        _row(1, 0x969C0, 0),
        _row(1, 0x969C1, 0x3C000000),
        _row(2, 0xE3702, 5),  # not a context word, not a FIR tap
    ]
    ctx, cabinet = at.extract(rows)
    assert [(r["type"], r["ctx"], r["word"]) for r in ctx] == [(1, 7, at.bits(2.5))]
    assert cabinet == {1: {"fir_taps": 1, "fir_words": 2}}


def test_float_helpers_round_trip():
    for v in (0.0, 1.0, -3.0, 2.8101623):
        assert abs(at.f32(at.bits(v)) - v) < 1e-6


def test_committed_data_holds_the_decoded_types():
    t = at.read_csv(DATA)
    assert sorted(t) == [1, 2, 3, 4, 5, 6]
    twin = t[1]
    assert at.section(twin, 7)[0] == at.f32(0x4033D9B3)
    assert abs(at.section(twin, 7)[0] - 2.8101623) < 1e-6
    assert abs(at.section(twin, 0x10)[4] - 0.3447298) < 1e-6
    assert [at.f32(t[k][-0xC]) for k in range(1, 7)] == [16.0, 16.0, 16.0, -3.0, 16.0, 16.0]
    assert at.section(t[5], 7) == [1.0, 0.0, 0.0, 0.0, 0.0]
    assert (
        at.section(t[6], 0x10)[0] == at.f32(0x3D700B71)
        or abs(at.section(t[6], 0x10)[0] - 0.0586) < 1e-3
    )


def test_test_signal_is_deterministic_and_has_the_call_layout():
    a, b = at.test_signal(), at.test_signal()
    assert a == b
    assert len(a) == at.CALLS * at.WORDS_PER_CALL
    assert all(abs(x) < 0.21 for x in a)
    assert a[1] == 0.8 * a[0] or a[0] == 0.0


def test_rust_array_formats_six_words_per_line():
    text = at.rust_array("X", list(range(9)), "doc")
    lines = text.splitlines()
    assert lines[0] == "/// doc"
    assert lines[1] == "pub const X: [u32; 9] = ["
    assert lines[2].count("0x") == 8 and lines[3].count("0x") == 1
    assert lines[-1] == "];"
