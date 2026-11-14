"""Unit tests for tools/organ_bass_keys.py (window decoding; no ARM harness)."""

from __future__ import annotations

import organ_bass_keys as bk


def test_window_of_maps_addresses_to_windows() -> None:
    assert bk.window_of(0x951F1) == (1, 0)
    assert bk.window_of(0x95265) == (2, 36)
    assert bk.window_of(0x95116) is None and bk.window_of(0x95285) is None


def test_weight_rows_and_first_values_keep_the_first_write() -> None:
    rows = bk.weight_rows(36, [(0x951F1, 5), (0x95116, 7), (0x951F1, 6), (0x9524D, 9)])
    assert rows == [(36, 1, 0, 5), (36, 1, 0, 6), (36, 2, 12, 9)]
    assert bk.first_values(rows, 1) == {36: {0: 5}}
    assert bk.first_values(rows, 2) == {36: {12: 9}}
