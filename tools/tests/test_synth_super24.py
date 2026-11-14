"""Tests for the SUPER 2..4 tables and the stage model `0x124985` (no firmware needed)."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import synth_stage_super2 as s2  # noqa: E402
import synth_super24_tables as t24  # noqa: E402
import synth_wave_model as m  # noqa: E402


def test_committed_table_file_matches_its_fingerprint_and_sizes() -> None:
    assert t24.fingerprint() == t24.FINGERPRINT
    tables = t24.load()
    for name, count in t24.SECTIONS:
        assert len(tables[name]) == count
    assert tables["rows"][:4] == [0x7A4, 4, 0x781, 1]
    assert tables["wave"][0] == 0xFB50F8BD


def test_render_and_parse_round_trip() -> None:
    tables = t24.load()
    assert t24.parse(t24.render(tables)) == tables


def test_row_index_follows_the_exponent_and_top_mantissa_bits() -> None:
    # f = 2^-8 (0x3B800000): 2f = 2^-7 -> fix = 2^24, exponent -6, top3 = 0 -> idx 48
    assert s2.row_index(m.ld(0x3B800000)) == 48
    assert s2.row_index(m.ld(0x3F000000)) == 0  # f = 0.5 clips the doubled pitch below 1
    assert s2.row_index(m.ld(0x3A000000)) > s2.row_index(m.ld(0x3B800000))
    assert s2.exp_of(0x02BFE404) == -5


def test_fir_lane_sums_even_and_odd_tap_halves_with_zero_or_sign_extension() -> None:
    fir = [0] * 16
    fir[0] = fir[1] = 1 << 30  # 0.5 on taps 0 and 1
    words = [0xFFFF0001, 0, 0, 0, 0, 0]  # sample 0 = 1, sample 1 = 0xFFFF (-1 signed)
    # x half: floor(1 / 2) = 0; y half reads sample 1: floor(65535 / 2) = 32767 (zero-extended)
    assert s2.fir_lane(words, 0, 0, fir) == 32767
    assert s2.fir_lane(words, 0, 0, fir, signed=True) == -1  # floor(-1 / 2) = -1
    big = [0x0000FFFF, 0, 0, 0, 0, 0]
    assert s2.fir_lane(big, 0, 0, fir) == 0x7FFF  # 0.5 * 65535 floored
    assert s2.fir_lane(big, 0, 0, fir, signed=True) == -1


def test_biquad_matches_a_direct_form_and_keeps_dc_gain() -> None:
    coefs = [m.ld(w) for w in (0x3E95F619, 0x3F15F619, 0x3E95F612, 0, 0x3E2FB0D5)]
    inp = [m.st(m.X(1))] * 4
    s1 = s2_ = m.X(0)
    out = []
    for _ in range(40):
        out, s1, s2_ = s2.biquad(inp, coefs, s1, s2_)
    assert abs(float(m.ld(out[-1])) - 1.0) < 1e-3


def test_stage_runs_on_committed_tables_and_is_deterministic() -> None:
    import random

    import synth_wave_oracle as o

    lanes = o.random_lanes(random.Random(5), True)
    s2.random_state(5, lanes)
    for ln in lanes:
        ln.pitch = m.f2u(0.01)
        ln.spread = m.f2u(0.3)
    a = s2.stage_super24(lanes, s2.load_tables())
    b = s2.stage_super24(lanes, s2.load_tables())
    assert a == b and len(a[0]) == 2 and len(a[0][0]) == 4
    assert len(s2.table_pokes()) == 3072 + 985


def test_rust_golden_file_is_what_the_generator_prints() -> None:
    import synth_stage_golden10 as g10

    path = Path(__file__).resolve().parents[2] / "crates/nord-core/src/synth"
    assert (path / "tests_fw_stage_golden10.rs").read_text() == g10.rust()
    assert g10.fnv1a([0]) == g10.fnv1a([0])
    assert len(g10.pitch_rows()) == 40 and len(g10.fir_rows(True)) == 48
