"""Unit tests for tools/arm_sy_pitch_law.py with a hand-assembled stub of the pitch block."""

from __future__ import annotations

import faulthandler
import math
import struct

import arm_emu as ae
import arm_sy_pitch_law as law
import pytest

pytest.importorskip("unicorn")
faulthandler.disable()


def stub_image() -> bytes:
    """`ldr r0, [r6, #8]` at the block start, then a branch to the block end."""
    img = bytearray(0x1E2000)
    start = law.BLOCK_START - ae.LOAD_BASE
    img[start : start + 8] = struct.pack("<II", 0xE5960008, 0xEA000000 | 0x38)
    return bytes(img)


def test_float_from_bits_decodes_ieee_single() -> None:
    assert law.float_from_bits(0x3F800000) == 1.0
    assert law.float_from_bits(0x34000000) == 2.0**-23


def test_run_block_writes_the_voice_fields_the_listing_reads() -> None:
    emu = ae.ArmEmu(stub_image())
    assert law.run_block(emu, 77) == 77
    assert emu.read32(law.OBJ_BASE + law.OBJ_KEY) == 77
    law.run_block(emu, 60, transpose=5, cents=7, fine=-3, tune=2, fixed_key=40)
    assert emu.read32(law.OBJ_BASE + law.OBJ_TRANSPOSE) == 5
    assert emu.read32(law.OBJ_BASE + law.OBJ_CENTS_A) == 7
    assert emu.read32(law.OBJ_BASE + law.OBJ_FINE) & 0xFFFFFFFF == (-3) & 0xFFFFFFFF
    assert emu.read32(law.LAYER_BASE + law.LAYER_MODE) == 0
    assert emu.read32(law.LAYER_BASE + law.LAYER_FIXED_KEY) == 40
    assert emu.read32(law.LAYER_BASE + law.LAYER_TUNE) == 2


def test_semitone_error_is_zero_for_equal_temperament() -> None:
    values = [2.0 ** ((k - 60) / 12.0) for k in range(128)]
    assert law.semitone_error(values) == pytest.approx(0.0, abs=1e-9)
    values[10] *= 2.0 ** (1.0 / 1200.0)
    assert law.semitone_error(values) == pytest.approx(1.0, abs=1e-6)
    assert math.isfinite(law.semitone_error(values))
