"""Unit tests for tools/arm_sy_kernel_rule.py with a hand-assembled compare chain (no firmware)."""

from __future__ import annotations

import faulthandler
import struct

import arm_emu as ae
import arm_sy_kernel_rule as rule
import pytest

pytest.importorskip("unicorn")
faulthandler.disable()

BASE = ae.LOAD_BASE


def words(*ws: int) -> bytes:
    return b"".join(struct.pack("<I", w & 0xFFFFFFFF) for w in ws)


def chain_image() -> bytes:
    """`cmp r3,#0x5F; movgt r2,#5; movle r2,#0` at 0x100."""
    code = words(0xE3530000 | 0x5F, 0xC3A02005, 0xD3A02000)
    img = bytearray(0x400)
    img[0x100 : 0x100 + len(code)] = code
    return bytes(img)


def test_kernel_index_maps_the_six_bases_and_rejects_others() -> None:
    assert [rule.kernel_index(0xE3400 + 0xC00 * k) for k in range(6)] == list(range(6))
    for bad in (0xE3401, 0xE3400 + 6 * 0xC00, 0xE3400 - 0xC00):
        with pytest.raises(ValueError):
            rule.kernel_index(bad)


def test_boundaries_collapses_runs() -> None:
    assert rule.boundaries([0, 0, 1, 1, 1, 0]) == [(0, 0), (2, 1), (5, 0)]
    assert rule.boundaries([]) == []


def test_chain_instructions_execute_for_a_key() -> None:
    emu = ae.ArmEmu(chain_image())
    for key, expected in ((0x5F, 0), (0x60, 5), (127, 5), (0, 0)):
        emu.set_reg(2, 99)
        emu.set_reg(3, key)
        emu.set_reg(13, ae.STACK_TOP - 0x1000)
        emu.run(BASE + 0x100, BASE + 0x10C, max_insns=8)
        assert emu.reg(2) == expected, key
