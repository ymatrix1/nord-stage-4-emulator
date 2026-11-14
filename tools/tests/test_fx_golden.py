"""Tests for tools/fx_golden.py: the golden vector files of the Rust ports are in sync."""

from pathlib import Path

import fx_golden as gd
import pytest

FX_DIR = Path(__file__).resolve().parents[2] / "crates" / "nord-core" / "src" / "fx"
FILES = {
    "tests_resonant_golden.rs": ["resonant", "resonant4"],
    "tests_allpass_golden.rs": ["allpass"],
    "tests_apan_golden.rs": ["apan"],
    "tests_lfo_golden.rs": ["lfo"],
    "tests_delay_golden.rs": ["delay"],
}


@pytest.mark.parametrize("name", sorted(FILES))
def test_the_rust_golden_file_is_what_the_generator_prints(name):
    want = gd.rust(FILES[name])
    assert (FX_DIR / name).read_text() == want, (
        f"regenerate: python tools/fx_golden.py ... > {name}"
    )


def test_vectors_are_deterministic_and_carry_the_state_between_calls():
    spec = gd.specs()["resonant"]
    a, b = gd.vectors(spec), gd.vectors(spec)
    assert a == b
    assert len(a["frames_in"]) == len(a["frames_out"]) == 4 * gd.CALLS
    assert a["ctx_in"] != a["ctx_out"] and a["frames_in"] != a["frames_out"]
    # a different seed gives different data
    assert gd.vectors(spec, seed=1)["frames_in"] != a["frames_in"]


def test_delay_vectors_hold_both_lines_the_table_and_the_sends():
    vec = gd.vectors(gd.specs()["delay"])
    assert len(vec["line"]) == len(vec["line_out"]) == 0xE0
    assert len(vec["sends"]) == len(vec["sends_out"]) == 4 and len(vec["shaper"]) == 49
    assert vec["line"] != vec["line_out"] and vec["sends"] != vec["sends_out"]


def test_allpass_vectors_carry_one_counter_pair_per_call_and_lfo_tables():
    ap = gd.vectors(gd.specs()["allpass"])
    assert len(ap["extra"]) == 2 * gd.CALLS
    lf = gd.vectors(gd.specs()["lfo"])
    assert (len(lf["table_a"]), len(lf["table_b"]), len(lf["table_c"])) == (65, 65, 161)


def test_command_line_prints_the_rust_file(capsys):
    assert gd.main(["allpass"]) == 0
    out = capsys.readouterr().out
    assert out.startswith("//! Firmware parity of the allpass") and "AP_CTX_IN" in out
