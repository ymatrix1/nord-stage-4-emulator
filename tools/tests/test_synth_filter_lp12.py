"""Tests for the 12 dB low pass model `0x1253BD` (no firmware needed)."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import synth_filter_lp12 as l12  # noqa: E402
import synth_filter_model as f  # noqa: E402

RUST = Path(__file__).resolve().parents[2] / "crates/nord-core/src/synth/fw_filter_table_d.rs"


def run(r: float, w28: int, xs: list[float]) -> list[int]:
    a, d = l12.read_tables()
    coefs = f.coefficients(w28, l12.f2u(r), a, d)
    return l12.process(f.Lane(), coefs, [l12.f2u(x) for x in xs])


def test_table_d_has_129_entries_and_starts_at_two() -> None:
    _, d = l12.read_tables()
    assert len(d) == l12.TABLE_LEN and d[0] == 0x40000000


def test_rust_table_file_is_what_the_generator_prints() -> None:
    assert RUST.read_text() == l12.rust_table_d()


def test_impulse_words_match_the_recorded_oracle_run_words() -> None:
    # model words equal to the oracle (1280 of 1280); the Rust test uses the same words
    assert run(0.3, 0x40000000, [1.0] + [0.0] * 7) == [
        0x0, 0x3D9328A0, 0x3E7F2779, 0x3EC426A6, 0x3EBF2FF5, 0x3E83EB28, 0x3DC2C2A8, 0xBD644E6E,
    ]  # fmt: skip
    sig = [0.5, -0.25, 0.75, 0.0, 0.125, -0.5, 0.25, 1.0]
    assert run(0.1, 0x20800000, sig) == [
        0x0, 0x3B921C95, 0x3C6E9513, 0x3CEC2802, 0x3D4E5AF5, 0x3D96DA13, 0x3DB31CAE, 0x3DB7F4CC,
    ]  # fmt: skip


def test_single_section_has_half_the_slope_of_the_two_section_stage() -> None:
    a, d = l12.read_tables()
    ca = f.coefficients(0x40000000, l12.f2u(0.1), a, d)
    # one section: the second section of the 4-pole stage is the only difference in the model
    xs = [l12.f2u(1.0)] + [0] * 63
    one = l12.process(f.Lane(), ca, xs)
    two = f.process(f.Lane(), ca, xs)
    assert one != two and len(one) == len(two) == 64
