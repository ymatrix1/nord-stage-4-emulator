"""Unit tests for tools/organ_bass_state.py (state rules; no ARM harness)."""

from __future__ import annotations

from pathlib import Path

import organ_bass_state as bs


def test_phi_is_continuous_piecewise_linear_and_invertible() -> None:
    assert bs.phi(0.0) == 0.0
    assert abs(bs.phi(0.5) - 0.5) < 1e-12
    assert abs(bs.phi(1.0) - 0.75) < 1e-12
    assert abs(bs.phi(1.5) - 0.875) < 1e-12
    for s in (0.1, 0.49, 0.75, 1.2, 1.6):
        assert abs(bs.phi_inv(bs.phi(s)) - s) < 1e-9


def test_window_index_maps_both_windows_and_the_gap() -> None:
    assert bs.window_index(0x951F1) == (1, 0)
    assert bs.window_index(0x95228) == (1, 55)
    assert bs.window_index(0x9522C) is None
    assert bs.window_index(0x95241) == (2, 0)
    assert bs.window_index(0x95285) is None


def test_key_rows_are_increments_over_the_previous_state() -> None:
    state: dict[tuple[int, int], float] = {(2, 3): 0.5}
    words = [(0x95244, bs.f32_bits(0.75)), (0xB44C9, bs.f32_bits(0.25)), (0x95116, 1)]
    rows, step, g = bs.key_rows(60, words, state, 0.0)
    assert rows == [(60, 2, 3, 0.25)]
    assert state[(2, 3)] == 0.75
    assert abs(step - 0.25) < 1e-6 and abs(g - 0.25) < 1e-6


def test_committed_csv_replays_the_oracle_g_words() -> None:
    csv = Path(__file__).resolve().parents[2] / "docs/research/data/bass-state.csv"
    steps = {}
    for line in csv.read_text().splitlines()[1:]:
        key, win, _idx, bits = line.split(",")
        if win == "0":
            steps[int(key)] = bs.f32(int(bits, 16))
    assert abs(bs.phi(steps[60]) - 0.026199) < 1e-6
    assert abs(bs.phi(steps[60] + steps[64] + steps[67]) - 0.078143) < 1e-6
    assert abs(bs.phi(sum(steps.values())) - 0.95344) < 1e-4
    assert bs.csv_lines([(1, 1, 2, 0.5)], {1: 0.25})[1:] == [
        "1,1,2,0x3f000000",
        "1,0,0,0x3e800000",
    ]
