"""Tests for the SYNC 6 / SYNC 8 stage models (`0x1239BE`, `0x123FE9`; no firmware needed)."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import synth_stage_golden7 as golden7  # noqa: E402
import synth_stage_sync as sync  # noqa: E402
import synth_stage_sync6 as sync6  # noqa: E402
import synth_wave_model as m  # noqa: E402
import synth_wave_oracle as o  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]


def lane(spread: float = 0.5) -> m.Lane:
    one, zero = m.f2u(1.0), m.f2u(0.0)
    return m.Lane(
        pitch=m.f2u(0.006), phase1=0x12345678, phase2=0x9ABCDEF0, shape_a=zero, shape_b=zero,
        ratio1=one, ratio2=one, weights=(zero, one, zero, zero), out_gain=one,
        spread=m.f2u(spread),
    )  # fmt: skip


def test_sync8_setup_has_the_fixed_width_and_unit_k() -> None:
    s = sync6.setup(lane(), sync.table(), 8)
    assert s["c2"] == m.HALF and s["k"] == 1
    assert s["h"] == m.ld(m.st(sync6.lookup_at(m.mul(m.X(1, 2), m.X(3, 4)), sync.table()) * 2))


def test_sync6_setup_width_follows_the_increment() -> None:
    s = sync6.setup(lane(), sync.table(), 6)
    assert 0 < s["c2"] < m.HALF
    assert s["q"] >= 2 and s["q2"] >= 2
    assert s["h"] == m.ld(m.st(sync.lookup(lane().spread, sync.table())))


def test_lookup_at_agrees_with_the_word_lookup_for_the_same_value() -> None:
    words = sync.table()
    v = m.ld(m.f2u(0.37))
    assert sync6.lookup_at(v, words) == sync.lookup(m.f2u(0.37), words)


def test_stages_depend_on_the_spread_word_and_return_both_phases() -> None:
    for stage in (sync6.stage_sync6, sync6.stage_sync8):
        a, p1, p2 = stage([lane(0.25), lane(0.25)])
        b, q1, q2 = stage([lane(0.75), lane(0.75)])
        assert a != b and (p1, p2) == (q1, q2)


def test_variants_differ_and_have_no_high_pass_state() -> None:
    six = sync6.stage_sync6([lane(), lane()])[0]
    eight = sync6.stage_sync8([lane(), lane()])[0]
    assert six != eight
    assert o.STAGE_SYNC6 in o.S5_STAGES and o.STAGE_SYNC8 in o.S5_STAGES
    assert o.STAGE_SYNC6 not in o.HP_STAGES and o.STAGE_SYNC8 not in o.HP_STAGES


def test_golden7_file_is_what_the_generator_prints() -> None:
    path = ROOT / "crates/nord-core/src/synth/tests_fw_stage_golden7.rs"
    assert path.read_text().replace("\r\n", "\n") == golden7.rust()
