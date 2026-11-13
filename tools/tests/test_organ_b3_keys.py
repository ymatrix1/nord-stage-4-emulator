"""Unit tests for tools/organ_b3_keys.py (synthetic key-down rows; no firmware)."""

from __future__ import annotations

import struct

import organ_b3_keys as k
import pytest


def hexf(v: float) -> str:
    return f"0x{struct.unpack('<I', struct.pack('<f', v))[0]:08x}"


def row(note: int, word: int, value: float) -> dict[str, str]:
    return {
        "model": "0",
        "velocity": "100",
        "phase": "on",
        "note": str(note),
        "dm_word": hex(word),
        "value": hexf(value),
    }


def key_rows(note: int, weights: dict[int, float]) -> list[dict[str, str]]:
    """Burst of a key whose four items write cumulative weights (5 1/3', 8', 4', 16')."""
    rows, last = [], {}
    for off in k.ITEM_OFFSETS:
        partial = k.fold_wheel(note + off - 24) + 24
        word = k.W_BASE + partial - k.W_RING_FIRST_NOTE
        last[word] = last.get(word, 0.0) + weights[off]
        rows.append(row(note, word, last[word]))
    return rows


def test_fold_reaches_wheels_12_to_88() -> None:
    assert k.fold_wheel(-12) == 12
    assert k.fold_wheel(0) == 12
    assert k.fold_wheel(93) == 81
    assert k.fold_wheel(50) == 50


def test_derive_splits_cumulative_writes_into_footage_weights() -> None:
    w = {-12: 0.009, 7: 0.02, 0: 0.016, 12: 0.022}
    out = k.derive(key_rows(24, w))  # 16', 8' and 4' fold onto wheel 12
    assert len(out) == 1
    for off, col in k.FOOTAGE_COLUMN.items():
        assert out[0][col] == pytest.approx(w[off], abs=1e-6)


def test_other_models_velocities_and_phases_are_ignored() -> None:
    rows = key_rows(60, {-12: 0.03, 7: 0.036, 0: 0.033, 12: 0.032})
    extra = [{**r, "model": "1"} for r in rows]
    extra += [{**r, "phase": "off"} for r in rows]
    extra += [{**r, "velocity": "32"} for r in rows]
    assert len(k.derive(rows + extra)) == 1


def test_word_index_mismatch_and_short_bursts_are_rejected() -> None:
    rows = key_rows(60, {-12: 0.03, 7: 0.036, 0: 0.033, 12: 0.032})
    bad = [dict(r) for r in rows]
    bad[0]["dm_word"] = hex(k.W_BASE + 5)
    with pytest.raises(ValueError):
        k.derive(bad)
    with pytest.raises(ValueError):
        k.derive(rows[:3])


def test_format_and_check_round_trip() -> None:
    derived = k.derive(key_rows(60, {-12: 0.03, 7: 0.036, 0: 0.033, 12: 0.032}))
    text = k.format_rows(derived)
    assert text.splitlines()[0] == k.HEADER
    table = [dict(zip(k.COLUMNS, line.split(","))) for line in text.splitlines()[1:]]
    assert k.check(derived, table) == []
    table[0]["w8"] = "0.5"
    assert k.check(derived, table)
