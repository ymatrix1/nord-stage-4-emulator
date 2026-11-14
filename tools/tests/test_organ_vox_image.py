"""Unit tests for organ_vox_image, organ_vox_chord and arm_vox_slots (no firmware)."""

from __future__ import annotations

import arm_vox_slots as av
import organ_vox_chord as vc
import organ_vox_image as vi
import pytest


def test_ladder_ratio_is_one_on_the_closed_form() -> None:
    words = [round(vi.LADDER_BASE * 2 ** (j / 12)) for j in range(36)]
    assert max(abs(r - 1) for r in vi.ladder_ratio(words)) < 1e-8


def test_bucket_map_has_three_groups_per_octave_fold() -> None:
    b = [vi.bucket_of(j) for j in range(36)]
    assert b[:12] == list(range(12)) and b[12:24] == list(range(12))
    assert b[24:32] == list(range(8)) and b[32:] == [0, 1, 2, 3]


def test_rust_file_carries_the_start_state() -> None:
    d = {
        "phases": list(range(84)),
        "lfo": [0x80000000, 0x80000000],
        "sine_ph": [0xFD70A3D8] * 24 + [0x8B851EB9] * 8 + [0x970A3D71] * 4,
        "ladder": list(range(36)),
        "gains": [0x3F800000] * 12 + [0x3E2FE9F0] * 12 + [0x3E29F113] * 8 + [0x3DAC5155] * 4,
        "consts": list(range(12)),
        "base": [0x7572967D, 0x7572967D],
        "pairs": [0] * 36,
    }
    text = vi.rust_file(d)
    assert "LFO_START: u32 = 0x80000000" in text
    assert "NOTE_START: [u32; 84]" in text and "SINE_START: [u32; 3]" in text
    assert "SINE_BASE: u32 = 0x7572967D" in text


def test_chord_weights_follow_the_key_map() -> None:
    w = vc.weights([60])
    assert sorted(w) == [24, 36, 48, 60]  # 16', 8', 4', 2'
    assert w[36] == pytest.approx(2076180 / 2**23)
    two = vc.weights([60, 72])  # C5 16' lands on C4 8'
    assert two[36] == pytest.approx(2 * 2076180 / 2**23)
    assert vc.fold(-1) == 11 and vc.fold(96) == 84


def test_chord_script_has_install_and_one_write_per_row() -> None:
    lines = vc.script([60]).splitlines()
    keys = [ln for ln in lines if ln.endswith("key-down rows")]
    assert len(keys) == 4
    assert keys[0].split()[1] == hex(vc.WEIGHTS0 + 24)
    assert keys[0].split()[2] == "0x3e7d70a0"  # VERIFIED word of the captured key-down


def test_slot_offsets_map_to_level_slots() -> None:
    assert av.slot_of(0xD18) == 0 and av.slot_of(0xD68) == 10 and av.slot_of(0xDB0) == 19
    assert av.case_slots([0xD30, 0xD80, 0xDE4]) == [3, 13]  # 0xDE4 is not a level


def test_level_table_reads_nine_words() -> None:
    import struct

    code = bytearray(av.LEVEL_TABLE - av.LOAD_BASE + 36)
    vals = [0, 629145, 838860, 1048576, 1258291, 1468006, 1677721, 1887436, 2076180]
    for v, x in enumerate(vals):
        struct.pack_into("<i", code, av.LEVEL_TABLE - av.LOAD_BASE + 4 * v, x)
    assert av.levels(bytes(code)) == vals


def test_timeline_script_staggers_the_footages() -> None:
    lines = vc.timeline_script([60]).splitlines()
    rows = {ln.split()[1]: int(ln.split()[0]) for ln in lines if "# tick" in ln}
    assert rows[hex(vc.WEIGHTS0 + 24)] == 65  # 16' at tick 3 = 65 passes
    assert rows[hex(vc.WEIGHTS0 + 36)] == 369  # 8' at tick 17
    assert rows[hex(vc.WEIGHTS0 + 60)] == 1020  # 2' at tick 47
