"""Tests for tools/fx_amp_cabinet.py: overlay selection and the seeded chain input."""

import csv

import fx_amp_cabinet as ac


def test_chain_input_is_deterministic_stereo_noise_in_range():
    a, b = ac.chain_input(), ac.chain_input()
    assert a == b
    assert len(a) == 2 * ac.CHAIN_FRAMES
    assert all(abs(x) <= 0.1 for x in a)
    assert len(set(a)) > ac.CHAIN_FRAMES  # not a constant or a short cycle


def test_type_overlay_keeps_only_core_one_cabinet_words(tmp_path):
    rows = [
        {"core": "1", "type": "1", "stage": "type", "dm_word": "0xe3702", "word": "0x3f800000"},
        {"core": "1", "type": "1", "stage": "type", "dm_word": "0x969c1", "word": "0x3c000000"},
        {"core": "1", "type": "1", "stage": "type", "dm_word": "0xc0191", "word": "0x1"},
        {"core": "2", "type": "1", "stage": "type", "dm_word": "0xe3703", "word": "0x2"},
        {"core": "1", "type": "2", "stage": "type", "dm_word": "0xe3704", "word": "0x3"},
    ]
    path = tmp_path / "cap.csv"
    with path.open("w", newline="", encoding="utf-8") as fh:
        w = csv.DictWriter(fh, fieldnames=list(rows[0]))
        w.writeheader()
        w.writerows(rows)
    assert ac.type_overlay(path, 1) == {0xE3702: 0x3F800000, 0x969C1: 0x3C000000}


def test_constants_describe_the_three_cabinets():
    assert [k for k, _ in ac.CABINETS] == [1, 3, 5]
    assert ac.CAB_CTX == 0xE376A and ac.TAPS == 400
