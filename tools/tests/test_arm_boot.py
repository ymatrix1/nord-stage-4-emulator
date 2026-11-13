"""Unit tests for tools/arm_boot.py with hand-assembled ARM snippets (no firmware needed)."""

from __future__ import annotations

import faulthandler
import gc
import struct

import arm_boot as ab
import arm_emu as ae
import pytest

pytest.importorskip("unicorn")
faulthandler.disable()

BX_LR = 0xE12FFF1E
BIG = 0x300000  # covers the firmware addresses that the tests patch (up to 0xC025D530)


def words(*ws: int) -> bytes:
    return b"".join(struct.pack("<I", w & 0xFFFFFFFF) for w in ws)


def image_at(offset: int, code: bytes) -> bytes:
    buf = bytearray(max(BIG, offset + len(code)))
    buf[offset : offset + len(code)] = code
    return bytes(buf)


@pytest.fixture(autouse=True)
def _release_emulators():
    yield
    gc.collect()


def emu_for(address: int, code: bytes) -> ae.ArmEmu:
    return ae.ArmEmu(image_at(address - ae.LOAD_BASE, code), ram_size=BIG)


def test_tick_advances_on_every_read() -> None:
    emu = emu_for(ae.LOAD_BASE, words(0xE5900000, BX_LR))  # ldr r0, [r0] ; bx lr
    state = ab.install_tick(emu, step=7)
    assert emu.call(ae.LOAD_BASE, ab.TICK_ADDRESS) == 7
    assert emu.call(ae.LOAD_BASE, ab.TICK_ADDRESS) == 14
    assert state["t"] == 14


def test_tick_is_64_bit() -> None:
    emu = emu_for(ae.LOAD_BASE, words(0xE5900004, BX_LR))  # ldr r0, [r0, #4] (high word)
    ab.install_tick(emu, step=1 << 33)
    # the high word is read through the same 8-byte window, so the hook fires and writes 2^33
    assert emu.call(ae.LOAD_BASE, ab.TICK_ADDRESS) == 2


def test_gpio_input_follows_last_set_or_clear() -> None:
    set_then_read = words(
        0xE3A01001,  # mov r1, #1
        0xE5801008,  # str r1, [r0, #8]   (SET_DATA)
        0xE5900010,  # ldr r0, [r0, #0x10] (IN_DATA)
        BX_LR,
    )
    clear_then_read = words(
        0xE3A01001,  # mov r1, #1
        0xE580100C,  # str r1, [r0, #0xC]  (CLR_DATA)
        0xE5900010,  # ldr r0, [r0, #0x10] (IN_DATA)
        BX_LR,
    )
    emu = emu_for(ae.LOAD_BASE, set_then_read)
    emu.write_bytes(ae.LOAD_BASE + 0x100, clear_then_read)
    state = ab.install_gpio_echo(emu)
    bank0 = ab.GPIO_BASE + 0x10
    assert emu.call(ae.LOAD_BASE, bank0) == 0xFFFFFFFF
    assert emu.call(ae.LOAD_BASE + 0x100, bank0) == 0
    assert state["writes"] == 2


def test_gpio_echo_reaches_every_bank() -> None:
    code = words(0xE3A01001, 0xE5801008, BX_LR)  # mov r1,#1 ; str r1,[r0,#8]
    emu = emu_for(ae.LOAD_BASE, code)
    ab.install_gpio_echo(emu)
    emu.call(ae.LOAD_BASE, ab.GPIO_BASE + 0x10)
    for bank in range(ab.GPIO_BANKS):
        assert emu.read32(ab.GPIO_BASE + 0x10 + ab.GPIO_BANK_STRIDE * bank + 0x10) == 0xFFFFFFFF


def test_boot_finishes_when_the_thunk_returns() -> None:
    emu = emu_for(ab.APP_THUNK, words(0xE3A00007, BX_LR))  # mov r0, #7 ; bx lr
    ab.install_boot_stubs(emu)
    result = ab.boot(emu, max_insns=1000, chunk=100)
    assert result.finished
    assert result.error == ""


def test_boot_stops_at_the_budget_and_reports_the_pc() -> None:
    emu = emu_for(ab.APP_THUNK, words(0xEAFFFFFE))  # b .
    ab.install_boot_stubs(emu)
    result = ab.boot(emu, max_insns=300, chunk=100)
    assert not result.finished
    assert result.pc == ab.APP_THUNK
    assert result.instructions == 300


def test_boot_rezeroes_low_memory_between_chunks() -> None:
    code = words(
        0xE3A01005,  # mov r1, #5
        0xE3A00000,  # mov r0, #0
        0xE5801000,  # str r1, [r0]
        0xEAFFFFFE,  # b .
    )
    emu = emu_for(ab.APP_THUNK, code)
    ab.install_boot_stubs(emu)
    ab.boot(emu, max_insns=200, chunk=50)
    assert emu.read32(0) == 0


def test_boot_stubs_replace_logger_and_board_config() -> None:
    emu = emu_for(ae.LOAD_BASE, words(BX_LR))
    ab.install_boot_stubs(emu)
    # the board configuration query writes 0 through r2 and reports success
    emu.write32(0x70000100, 0xFFFFFFFF)
    assert emu.call(ab.BOARD_CONFIG_QUERY, 0, 0, 0x70000100) == 1
    assert emu.read32(0x70000100) == 0
    assert emu.call(ab.LOGGER, 1, 2) == 0


def make_registry(emu: ae.ArmEmu, ctx: int) -> None:
    emu.uc.mem_map(ctx, 0x10000)
    emu.write32(ctx, 2)
    entries = (ctx + 0x1000, ctx + 0x2000)
    for i, (e, name, ident, lo, hi, dflt) in enumerate(
        [
            (entries[0], "Edt_Test_kA", 0x16D, 0, 8, 8),
            (entries[1], "Edt_Test_kPan", 5, -64, 63, 0),
        ]
    ):
        emu.write32(ctx + 4 + 4 * i, e)
        emu.write_bytes(e + 0x400, name.encode() + b"\0")
        emu.write32(e + ab.ENTRY_ID, ident)
        emu.write32(e + ab.ENTRY_DOMAIN, 0xC)
        emu.write32(e + ab.ENTRY_NAME_PTR, e + 0x400)
        emu.write32(e + ab.ENTRY_MIN, lo)
        emu.write32(e + ab.ENTRY_MAX, hi)
        emu.write32(e + ab.ENTRY_DEFAULT, dflt)
        emu.write32(e + ab.ENTRY_WIDTHS, 4 if i == 0 else 1)
        emu.write32(e + ab.ENTRY_WIDTHS + 4, 4 if i == 0 else 7)


def test_read_edit_registry_decodes_descriptors() -> None:
    emu = emu_for(ae.LOAD_BASE, words(BX_LR))
    make_registry(emu, 0x98000000)
    params = ab.read_edit_registry(emu, 0x98000000)
    assert [p.name for p in params] == ["Edt_Test_kA", "Edt_Test_kPan"]
    assert params[0].edit_id == 0x16D
    assert (params[0].minimum, params[0].maximum, params[0].default) == (0, 8, 8)
    assert params[0].widths == (4, 4, 0)
    assert (params[1].minimum, params[1].maximum) == (-64, 63)  # signed words


def test_registry_csv_rows() -> None:
    emu = emu_for(ae.LOAD_BASE, words(BX_LR))
    make_registry(emu, 0x98000000)
    text = ab.registry_csv(ab.read_edit_registry(emu, 0x98000000))
    lines = text.splitlines()
    assert lines[0] == ab.CSV_HEADER
    assert lines[1] == "0,0x16d,0xc,Edt_Test_kA,0,8,8,4;4;0"
    assert lines[2] == "1,0x5,0xc,Edt_Test_kPan,-64,63,0,1;7;0"


def test_build_edit_registry_runs_the_registration_routine() -> None:
    # fake routine: ctx[0] = 3 ; registry_object[0x1C] = ctx ; return
    code = words(
        0xE3A01003,  # mov r1, #3
        0xE5801000,  # str r1, [r0]
        0xE582001C,  # str r0, [r2, #0x1c]
        BX_LR,
    )
    emu = emu_for(ab.REGISTER_EDIT_PARAMS, code)
    ctx = ab.build_edit_registry(emu, ctx=0x98000000, layers=6)
    assert emu.read32(ctx) == 3
    assert emu.read32(ab.REGISTRY_GLOBAL + 0x1C) == ctx
    # the configuration struct handed to the routine starts with the layer count
    assert emu.read32(ctx + ab.EDIT_CTX_SIZE // 2) == 6


def test_construct_organ_layer_and_subscribed_ids() -> None:
    # fake constructor sets bitmap word 0 = bit 4; the test adds word 11 (ids 0x16D..0x175)
    code = words(
        0xE3A01010,  # mov r1, #0x10
        0xE5801064,  # str r1, [r0, #0x64]
        BX_LR,
    )
    emu = emu_for(ab.ORGAN_LAYER_CTOR, code)
    layer = ab.construct_organ_layer(emu, layer=0x99000000)
    emu.write32(layer + 0x64 + 4 * 11, 0x3FE000)
    ids = ab.subscribed_ids(emu, layer)
    assert ids[0] == 4
    assert [i for i in ids if 0x16D <= i <= 0x175] == list(range(0x16D, 0x176))
    assert emu.read32(ab.DISPATCHER_GLOBAL) != 0


def test_boot_result_finished_flag() -> None:
    assert ab.BootResult(ae.SENTINEL, 0, 10).finished
    assert not ab.BootResult(0xC0000000, 0, 10).finished


def test_boot_until_stops_at_the_target_instruction() -> None:
    code = words(0xE3A00001, 0xE3A00002, 0xE3A00003, BX_LR)  # mov r0,#1..#3 ; bx lr
    emu = emu_for(ab.APP_THUNK, code)
    ab.install_boot_stubs(emu)
    result = ab.boot_until(emu, ab.APP_THUNK + 8, max_insns=1000, chunk=100)
    assert result.history == [ab.APP_THUNK + 8]
    assert result.pc == ab.APP_THUNK + 8
    assert emu.reg(0) == 2


def test_dispatcher_and_program_objects_read_the_firmware_tables() -> None:
    emu = emu_for(ae.LOAD_BASE, words(BX_LR))
    g = 0x98100000
    emu.uc.mem_map(g, 0x1000)
    emu.write32(ab.DISPATCHER_GLOBAL, g)
    emu.write32(g + 0x17C, 0x1111)
    emu.write32(g + 0x17C + 8, 0x3333)
    assert ab.dispatcher(emu) == g
    assert ab.program_objects(emu, 4) == [0x1111, 0, 0x3333, 0]


def test_set_drawbar_passes_q16_and_edit_id() -> None:
    # fake handler: [r0] = r2 (edit id) ; [r0 + 4] = [r3] (q16 value) ; return
    code = words(0xE5802000, 0xE5931000, 0xE5801004, BX_LR)
    emu = emu_for(ab.DRAWBAR_HANDLER, code)
    emu.uc.mem_map(0x98200000, 0x1000)
    ab.set_drawbar(emu, 0x98200000, 0, 3, 5, 0x98200800)
    assert emu.read32(0x98200000) == 0x16D + 3
    assert emu.read32(0x98200004) == 5 << 16


def test_read_drawbar_record_and_update_sets_busy_byte() -> None:
    emu = emu_for(ab.RECORD_UPDATE, words(BX_LR))
    rec = 0x98300000
    emu.uc.mem_map(rec, 0x1000)
    for k in range(9):
        emu.write32(rec + ab.RECORD_DRAWBARS + 4 * k, k)
    assert ab.read_drawbar_record(emu, rec) == list(range(9))
    assert ab.run_record_update(emu, rec) == []
    assert emu.read_bytes(rec + ab.RECORD_BUSY, 1) == b"\x01"
