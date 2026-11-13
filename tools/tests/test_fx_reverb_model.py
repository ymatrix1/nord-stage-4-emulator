"""Tests for tools/fx_reverb_model.py: the half-rate wrapper model and its Rust golden file."""

from fractions import Fraction
from pathlib import Path

import fx_biquad_check as fb
import fx_reverb_model as rm

FX_DIR = Path(__file__).resolve().parents[2] / "crates" / "nord-core" / "src" / "fx"
ZERO = (Fraction(0), Fraction(0))


def zero_core(_a, _b):
    return ZERO, ZERO


def squash(text: str) -> str:
    """Whitespace removed: `cargo fmt` re-wraps the generated file."""
    return "".join(text.split()).replace(",]", "]").replace(",)", ")")


def test_the_rust_golden_file_is_what_the_generator_prints():
    text = (FX_DIR / "tests_rvb_wrap_golden.rs").read_text()
    assert squash(text) == squash(rm.golden_rust()), (
        "regenerate: python tools/fx_reverb_model.py golden > tests_rvb_wrap_golden.rs"
    )


def test_golden_vectors_are_deterministic_and_complete():
    a, b = rm.golden_vectors(), rm.golden_vectors()
    assert a == b
    assert len(a["out4"]) == 8 * rm.GOLDEN_CALLS and len(a["out2"]) == 4 * rm.GOLDEN_CALLS
    assert len(a["states"]) == 24 and any(a["states"])


def test_zero_gain_and_zero_core_give_silence_and_the_gain_is_applied_to_the_dry_input():
    frames = [(Fraction(1, 2), Fraction(-1, 4))] * 4
    z = [Fraction(0)] * 12
    out, _ = rm.wrapper4(frames, Fraction(0), list(z), list(z), zero_core)
    assert all(v == 0 for f in out for v in f)
    out, _ = rm.wrapper4(frames, Fraction(1, 2), list(z), list(z), zero_core)
    assert out[0] == (Fraction(1, 4), Fraction(-1, 8))


def test_the_core_gets_frames_0_and_2_of_the_low_passed_block_and_its_output_is_held():
    seen = []

    def core(a, b):
        seen.append((a, b))
        return ZERO, ZERO

    z = [Fraction(0)] * 12
    impulse = [(Fraction(1), Fraction(0))] + [ZERO] * 3
    rm.wrapper4(impulse, Fraction(0), list(z), list(z), core)
    b0 = rm.COEF[0]
    assert abs(seen[0][0][0] - b0**3) < Fraction(1, 10**6)  # impulse through three stages
    one = (Fraction(1), Fraction(1))
    _, zz = rm.wrapper4([ZERO] * 4, Fraction(0), list(z), list(z), lambda *_: (one, ZERO))
    assert zz[1][0] > zz[0][0] > 0  # held pair, then a rising low pass


def test_mix2_adds_the_wet_frames_after_the_gain():
    out = rm.wrapper2(
        [(Fraction(1), Fraction(2)), (Fraction(3), Fraction(4))],
        Fraction(1, 2),
        ((Fraction(10), Fraction(20)), (Fraction(30), Fraction(40))),
    )
    assert out == [(Fraction(21, 2), Fraction(21)), (Fraction(63, 2), Fraction(42))]


def test_underflow_flushes_to_zero_and_the_sign_of_zero_is_not_modelled():
    tiny = Fraction(1, 2**130)
    assert rm.rnd(tiny) == 0 and rm.store(-tiny) == 0
    assert rm.unsign0(0x80000000) == 0 and rm.unsign0(0x3F800000) == 0x3F800000
    assert rm.rnd(Fraction(3, 4)) == Fraction(3, 4)


def test_state_layout_is_four_words_per_stage_and_two_banks():
    assert (rm.ST1, rm.ST2, rm.HOLD, rm.GAIN) == (
        rm.CTX + 0x49,
        rm.CTX + 0x55,
        rm.CTX + 0x41,
        rm.CTX + 0x15,
    )
    assert rm.ST2 - rm.ST1 == 4 * rm.STAGES


def test_static_context_has_the_type_words_and_the_core_pointers():
    mem = rm.static_context()
    assert mem[rm.CTX - 0x12] == 0x15  # core loop count (21 blocks), a type 0 word
    assert mem[rm.CTX + 0x27] == 0xB5000 and len(mem) >= 39
    assert all(isinstance(w, int) for w in mem.values())


def test_pairs_and_flat_round_trip():
    words = [fb.bits(0.5), fb.bits(-0.25), fb.bits(1.0), fb.bits(2.0)]
    assert rm.flat(rm.pairs(words)) == words
