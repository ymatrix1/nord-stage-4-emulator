"""Unit tests for tools/arm_pass_script.py (script formatting and a hand-assembled chain step)."""

from __future__ import annotations

import faulthandler
import gc

import arm_edit_capture as aec
import arm_emu as ae
import arm_pass_script as aps
import pytest

pytest.importorskip("unicorn")
faulthandler.disable()

PNO_PORT2 = ae.PORT2_BASE + 0x10  # comm 1
CORE1_BASE = 0x28000000


def msg(dm_word: int, value: int, comm: int = 1, path: int = 2) -> ae.Message:
    word1 = CORE1_BASE + 4 * dm_word if path == 2 else dm_word
    return ae.Message(path=path, comm=comm, word1=word1, word2=value)


def test_script_line_formats_address_value_and_note() -> None:
    assert aps.script_line(msg(0xC01DE, 0x12272D), "x") == "0 0xc01de 0x0012272d  # x"
    assert aps.script_line(msg(0x95FC6, 0x3F800000)) == "0 0x95fc6 0x3f800000"


def test_only_pno_core_1_path_2_messages_are_kept() -> None:
    other_core = ae.Message(path=2, comm=1, word1=0x28800000 + 4 * 5, word2=1)
    smp = msg(0x9403E, 1, comm=4, path=1)
    kept = aps.core1_words([msg(7, 1), other_core, smp, msg(9, 2)])
    assert [m.dm_word for m in kept] == [7, 9]


def test_script_text_shifts_the_pass_number() -> None:
    text = aps.script_text([msg(0x10, 5), msg(0x11, 6)], "n", shift=3)
    assert text == "3 0x10 0x00000005  # n\n3 0x11 0x00000006  # n\n"
    assert aps.script_text([], "n") == ""


def test_dedup_keeps_the_last_value_of_each_address_in_order() -> None:
    kept = aps.dedup_last([msg(1, 10), msg(2, 20), msg(1, 30)])
    assert [(m.dm_word, m.word2) for m in kept] == [(2, 20), (1, 30)]


def test_step_chain_calls_the_step_until_the_state_is_done() -> None:
    gc.collect()
    emu = ae.ArmEmu(bytes(0x1000))
    chain = 0x80010000
    # `ldr r1, [r0, #0xB0]; add r1, r1, #1; str r1, [r0, #0xB0]; bx lr` at the step address
    for k, word in enumerate((0xE59010B0, 0xE2811001, 0xE58010B0, 0xE12FFF1E)):
        emu.write32(aps.CHAIN_STEP + 4 * k, word)
    emu.write32(0x70000800, 0)  # maps the stack page
    emu.set_reg(13, 0x70000F00)
    emu.write32(chain + aps.FIELD_STATE, 0)
    assert aps.step_chain(emu, chain) == aps.STATE_DONE
    assert emu.read32(chain + aps.FIELD_STATE) == aps.STATE_DONE


def test_the_default_scratch_is_the_edit_capture_scratch() -> None:
    assert aps.SCRATCH == aec.SCRATCH
