"""Unit tests for tools/arm_keys.py: the idle-high key matrix of the front panel."""

from __future__ import annotations

import arm_keys as ak
import pytest


def test_idle_matrix_reads_all_ones() -> None:
    keys = ak.KeyMatrix()
    keys.write(ak.STROBE_ADDRESS, 0x04)
    assert keys.read(0x60000010) == 0xFF
    assert keys.read(0x60000020) == 0xFF


def test_a_held_key_clears_its_bit_only_while_its_column_is_strobed() -> None:
    keys = ak.KeyMatrix()
    keys.press(0x04, 1, 3)
    keys.write(ak.STROBE_ADDRESS, 0x04)
    assert keys.read(0x60000020) == 0xFF & ~(1 << 3)
    assert keys.read(0x60000010) == 0xFF
    keys.write(ak.STROBE_ADDRESS, 0x08)
    assert keys.read(0x60000020) == 0xFF
    keys.release(0x04, 1, 3)
    keys.write(ak.STROBE_ADDRESS, 0x04)
    assert keys.read(0x60000020) == 0xFF


def test_release_all_and_upper_port_bytes() -> None:
    keys = ak.KeyMatrix()
    keys.press(0x01, 0, 0)
    keys.press(0x01, 0, 5)
    keys.write(ak.STROBE_ADDRESS, 0x01)
    assert keys.read(0x60000010) == 0xFF & ~0x21
    assert keys.read(0x60000011) == 0xFF  # the byte next to the port is not part of the mask
    assert keys.read(0x60000030) is None
    keys.release_all()
    assert keys.read(0x60000010) == 0xFF


def test_press_rejects_a_bit_outside_the_scan_mask() -> None:
    keys = ak.KeyMatrix()
    with pytest.raises(ValueError):
        keys.press(0x01, 0, 7)  # strobe 1, port 0 has 6 bits
    with pytest.raises(ValueError):
        keys.press(0x40, 1, 0)  # strobe 0x40 has no port 1 bits
    with pytest.raises(ValueError):
        keys.press(0x03, 0, 0)  # not a column strobe


def test_all_keys_counts_the_masks_of_the_scan_table() -> None:
    keys = ak.all_keys()
    assert len(keys) == 13 + 15 + 16 + 16 + 16 + 14 + 8
    assert keys[0] == (0x01, 0, 0)
    assert len(set(keys)) == len(keys)
    assert all(ak.USABLE_BITS[(s, p)] >> b & 1 for s, p, b in keys)


def test_only_the_strobe_register_changes_the_column() -> None:
    keys = ak.KeyMatrix()
    keys.write(0x60000025, 0x04)
    assert keys.strobe == 0
    keys.write(ak.STROBE_ADDRESS, 0x1FF)
    assert keys.strobe == 0xFF
