"""Tests for tools/arm_master_level.py (the float32 law, no firmware needed)."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import arm_master_level as t  # noqa: E402


def test_default_volume_is_unity():
    # the mixer constructor stores taper 1.0; volume 127 gives (127 / 127)^2
    assert t.f32_bits(t.master_level_word(127)) == 0x3F800000


def test_words_match_the_firmware_run():
    # words the firmware sender produced in the settled emulator (arm_master_level.py run)
    assert t.f32_bits(t.master_level_word(100)) == 0x3F1EB866
    assert t.f32_bits(t.master_level_word(64)) == 0x3E820610
    assert t.master_level_word(0) == 0.0


def test_law_is_the_squared_ratio_and_the_taper_multiplies():
    for v in (1, 17, 90, 126):
        assert abs(t.midi_volume_mult(v) - (v / 127) ** 2) < 1e-6
    assert abs(t.master_level_word(127, 0.5) - 0.5) < 1e-9


def test_address_is_the_fx_core1_word():
    # system byte address = 0x28000000 + 4 * DM word (core 1)
    assert t.MASTER_LEVEL_ADDRESS == 0x28000000 + 4 * t.MASTER_LEVEL_DM
