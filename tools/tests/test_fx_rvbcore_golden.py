"""Tests for tools/fx_rvbcore_golden.py (no image needed): inputs, hash and the Rust table."""

import struct

import fx_rvbcore_golden as g


def test_noise_is_deterministic_exact_float32_and_bounded():
    a, b = g.noise_words(3), g.noise_words(3)
    assert a == b and len(a) == 3 and all(len(r) == 8 for r in a)
    vals = [struct.unpack("<f", struct.pack("<I", w))[0] for r in a for w in r]
    assert all(-0.25 <= v < 0.25 for v in vals) and len(set(vals)) > 20


def test_impulse_is_one_frame_of_ones_at_call_zero():
    rows = g.impulse_words(4)
    assert rows[0][:2] == [0x3F800000] * 2 and not any(rows[0][2:]) and not any(rows[1])


def test_fnv_ignores_the_sign_of_zero_and_is_order_sensitive():
    assert g.fnv([0x80000000, 1]) == g.fnv([0, 1])
    assert g.fnv([1, 2]) != g.fnv([2, 1])
    assert g.fnv([]) == g.FNV_OFFSET


def test_amount_words_are_the_captured_pair():
    assert g.amount_words(127) == (0x3F800000, 0)
    assert g.amount_words(64)[0] == 0x3EC00000


def test_the_rust_table_lists_every_case():
    case = {
        "type": 0, "send": 1, "dry": 2, "kind": "noise", "hash": 3, "nonzero": 4, "first": 0,
        "samples": [(0, [0] * 8)],
    }  # fmt: skip
    text = g.rust([case])
    assert "noise: true," in text and "pub(super) const GOLDEN: [Golden; 1]" in text
