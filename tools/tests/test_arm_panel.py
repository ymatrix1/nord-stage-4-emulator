"""Unit tests for tools/arm_panel.py: the pure helpers and the hooks on a tiny emulator."""

from __future__ import annotations

import faulthandler
import struct

import arm_emu as ae
import arm_oled as ao
import arm_panel as ap
import pytest

pytest.importorskip("unicorn")
faulthandler.disable()

BIG = 0x300000
BX_LR = 0xE12FFF1E


def test_panel_answers_only_inside_the_detection_routine() -> None:
    inside = ap.DETECT_RANGE[0] + 8
    assert ap.panel_answer(inside, 0x60000010) == 0x80
    assert ap.panel_answer(inside, 0x60000020) == 0x80
    assert ap.panel_answer(inside, 0x60000030) == 0
    assert ap.panel_answer(ap.DETECT_RANGE[1], 0x60000010) == 0  # the key scan reads it too
    assert ap.panel_answer(0xC0100000, 0x60000010) == 0


def test_wait_loop_range() -> None:
    assert ap.in_wait_loop(0xC0075D40)
    assert not ap.in_wait_loop(0xC0075D0C)


def test_install_panel_link_captures_cs2_stores_into_the_controllers() -> None:
    emu = ae.ArmEmu(bytes(BIG))
    link = ap.install_panel_link(emu)
    code = struct.pack(
        "<7I",
        0xE3A020B0,  # mov r2, #0xB0 (page 0 command)
        0xE59F1010,  # ldr r1, [pc, #0x10] -> port
        0xE5C12000,  # strb r2, [r1]
        0xE3A02004,  # mov r2, #4 (latch: display 0 data)
        0xE59F1008,  # ldr r1, [pc, #8] -> latch
        0xE5C12000,  # strb r2, [r1]
        BX_LR,
    ) + struct.pack("<2I", 0x60000017, ao.LATCH_ADDRESS)
    emu.uc.mem_write(ae.LOAD_BASE + 0x1000, code)
    emu.call(ae.LOAD_BASE + 0x1000)
    assert link.panels[0].commands == [0xB0]
    assert link.latch == 0x04


def test_firmware_draw_runs_the_routine_and_reads_back_the_plane() -> None:
    # A fake DRAW_TEXT that stores its x argument (first stack word) into the plane.
    code = struct.pack("<3I", 0xE59D4000, 0xE5C24000, BX_LR)  # ldr r4,[sp]; strb r4,[r2]; bx lr
    buf = bytearray(BIG)
    buf[ao.DRAW_TEXT - ae.LOAD_BASE : ao.DRAW_TEXT - ae.LOAD_BASE + len(code)] = code
    emu = ae.ArmEmu(bytes(buf))
    frame = ap.firmware_draw(emu, 0xC0000000, "hi", x=42, y=3)
    assert len(frame) == ao.FRAME_BYTES and frame[0] == 42
    assert bytes(emu.read_bytes(ap.TEXT_BUF, 3)) == b"hi\0"


def test_firmware_width_calls_the_measure_routine() -> None:
    code = struct.pack("<2I", 0xE3A0002A, BX_LR)  # mov r0, #42; bx lr
    buf = bytearray(BIG)
    buf[ao.TEXT_MEASURE - ae.LOAD_BASE : ao.TEXT_MEASURE - ae.LOAD_BASE + 8] = code
    emu = ae.ArmEmu(bytes(buf))
    assert ap.firmware_width(emu, 0xC0000000, "x") == 42


def test_panel_block_answers_present_in_detection_idle_high_otherwise() -> None:
    import arm_keys as ak

    keys = ak.KeyMatrix()
    keys.press(0x04, 0, 2)
    keys.write(ak.STROBE_ADDRESS, 0x04)
    inside = ap.DETECT_RANGE[0] + 8
    assert ap.panel_block(inside, 0x60000010, keys) == bytes([0x80, 0, 0, 0])
    assert ap.panel_block(0xC0100000, 0x60000010, keys) == bytes([0xFF & ~4, 0xFF, 0xFF, 0xFF])
    assert ap.panel_block(0xC0100000, 0x60000020, keys)[0] == 0xFF
    assert ap.panel_block(0xC0100000, 0x60000010, None) == bytes(4)


def test_install_panel_link_with_keys_answers_the_matrix_and_tracks_the_strobe() -> None:
    import arm_keys as ak

    emu = ae.ArmEmu(bytes(BIG))
    keys = ak.KeyMatrix()
    keys.press(0x08, 1, 0)
    ap.install_panel_link(emu, keys)
    code = struct.pack(
        "<6I",
        0xE3A02008,  # mov r2, #8 (strobe column 3)
        0xE59F100C,  # ldr r1, [pc, #0xc] -> strobe register
        0xE5C12000,  # strb r2, [r1]
        0xE59F3008,  # ldr r3, [pc, #8] -> port 1
        0xE5D30000,  # ldrb r0, [r3]
        BX_LR,
    ) + struct.pack("<2I", ak.STROBE_ADDRESS, 0x60000020)
    emu.uc.mem_write(ae.LOAD_BASE + 0x1000, code)
    assert emu.call(ae.LOAD_BASE + 0x1000) == 0xFE
    assert keys.strobe == 8
