"""Unit tests for tools/arm_steady.py with hand-assembled ARM snippets (no firmware needed)."""

from __future__ import annotations

import faulthandler
import gc
import struct

import arm_boot as ab
import arm_emu as ae
import arm_steady as ast
import pytest

pytest.importorskip("unicorn")
faulthandler.disable()

BX_LR = 0xE12FFF1E
B_SELF = 0xEAFFFFFE  # b .
BIG = 0x600000  # covers the firmware handler table at 0xC055F674
HANDLER = ae.LOAD_BASE + 0x1000
COUNTER = ae.SRAM_BASE + 0x200


def words(*ws: int) -> bytes:
    return b"".join(struct.pack("<I", w & 0xFFFFFFFF) for w in ws)


def emu_with(patches: dict[int, bytes]) -> ae.ArmEmu:
    buf = bytearray(BIG)
    for address, code in patches.items():
        buf[address - ae.LOAD_BASE : address - ae.LOAD_BASE + len(code)] = code
    emu = ae.ArmEmu(bytes(buf), ram_size=BIG)
    emu.uc.mem_map(0, ab.LOW_MEMORY)
    return emu


@pytest.fixture(autouse=True)
def _release_emulators():
    yield
    gc.collect()


def counter_handler() -> bytes:
    """ldr r1, =COUNTER ; ldr r0, [r1] ; add r0, r0, #1 ; str r0, [r1] ; bx lr"""
    return words(0xE59F1014, 0xE5910000, 0xE2800001, 0xE5810000, BX_LR, 0, 0, COUNTER)


def test_snapshot_restore_round_trips_memory_registers_and_model_state() -> None:
    emu = emu_with({})
    ab.install_tick(emu, step=3)
    emu.bump_new(ab.OPERATOR_NEW)
    emu.write32(ae.LOAD_BASE + 0x2000, 0x11111111)
    emu.set_reg(5, 0xABCD)
    emu.tick_state["t"] = 99
    emu.heap_top[0] += 0x40
    blob = ast.snapshot(emu)
    emu.write32(ae.LOAD_BASE + 0x2000, 0x22222222)
    emu.set_reg(5, 1)
    emu.tick_state.update(t=0, step=7)
    emu.heap_top[0] = 0
    ast.restore(emu, blob)
    assert emu.read32(ae.LOAD_BASE + 0x2000) == 0x11111111
    assert emu.reg(5) == 0xABCD
    assert emu.tick_state == {"t": 99, "step": 3}
    assert emu.heap_top[0] == 0x90000040


def test_restore_maps_pages_the_target_does_not_have() -> None:
    src = emu_with({})
    src.uc.mem_map(0xA5000000, ae.PAGE)
    src.write32(0xA5000000, 0xCAFE)
    blob = ast.snapshot(src)
    dst = emu_with({})
    ast.restore(dst, blob)
    assert dst.read32(0xA5000000) == 0xCAFE


def test_run_until_stops_before_the_stop_pc() -> None:
    stop = ae.LOAD_BASE + 0x2000
    # mov r0,#1 ; mov r1,#2 ; (stop) mov r2,#3
    emu = emu_with({ae.LOAD_BASE + 0x2000 - 8: words(0xE3A00001, 0xE3A01002, 0xE3A02003)})
    pc, _n, err = ast.run_until(emu, stop, 1000, pc=stop - 8)
    assert (pc, err) == (stop, "")
    assert emu.reg(1) == 2
    assert emu.reg(2) == 0  # the instruction at the stop pc did not run


def test_run_until_reports_the_spin_pc_when_the_budget_ends() -> None:
    spin = ae.LOAD_BASE + 0x2000
    emu = emu_with({spin: words(B_SELF)})
    pc, n, err = ast.run_until(emu, 0xC0099999, 500, pc=spin, chunk=100)
    assert (pc, n, err) == (spin, 500, "")


def test_run_until_reports_a_fault() -> None:
    # r0 = 0xFFFFFFFC ; ldr r0, [r0]: the last word of the address space cannot be auto-mapped
    start = ae.LOAD_BASE + 0x2000
    emu = emu_with({start: words(0xE3E00003, 0xE5900000)})
    _pc, _n, err = ast.run_until(emu, None, 1000, pc=start)
    assert err


def test_call_preserving_returns_r0_and_restores_every_register() -> None:
    fn = ae.LOAD_BASE + 0x2000
    # add r0, r0, r1 ; mov r4, #0x55 ; bx lr
    emu = emu_with({fn: words(0xE0800001, 0xE3A04055, BX_LR)})
    emu.set_reg(13, ae.STACK_TOP - 0x1000)
    emu.set_reg(4, 0x44)
    emu.set_reg(14, 0x1234)
    emu.set_reg(15, 0xC0000000)
    assert ast.call_preserving(emu, fn, 30, 12) == 42
    assert emu.reg(4) == 0x44
    assert emu.reg(13) == ae.STACK_TOP - 0x1000
    assert emu.reg(14) == 0x1234


def test_inject_irq_calls_the_registered_handler() -> None:
    emu = emu_with({HANDLER: counter_handler()})
    emu.set_reg(13, ae.STACK_TOP - 0x1000)
    emu.write32(ast.IRQ_TABLE + 4 * ast.TIMER_IRQ, HANDLER)
    assert ast.inject_irq(emu) == HANDLER
    assert ast.inject_irq(emu) == HANDLER
    assert emu.read32(COUNTER) == 2


def test_inject_irq_ignores_an_empty_slot() -> None:
    emu = emu_with({})
    assert ast.inject_irq(emu, 5) == 0


def test_run_ticking_injects_one_interrupt_per_period() -> None:
    spin = ae.LOAD_BASE + 0x2000
    emu = emu_with({spin: words(B_SELF), HANDLER: counter_handler()})
    emu.set_reg(13, ae.STACK_TOP - 0x1000)
    emu.write32(ast.IRQ_TABLE + 4 * ast.TIMER_IRQ, HANDLER)
    emu.set_reg(15, spin)
    pc, err = ast.run_ticking(emu, 1000, per_tick=100)
    assert (pc, err) == (spin, "")
    assert emu.read32(COUNTER) == 10


def test_enable_real_timer_stops_the_read_hack() -> None:
    # ldrd/ldr r1, =tick ; ldr r0, [r1] ; bx lr   (a plain read of the tick address)
    fn = ae.LOAD_BASE + 0x2000
    emu = emu_with({fn: words(0xE59F1008, 0xE5910000, BX_LR, 0, ab.TICK_ADDRESS)})
    ab.install_tick(emu, step=5)
    emu.call(fn)
    emu.call(fn)
    assert emu.tick_state["t"] == 10
    ast.enable_real_timer(emu)
    emu.call(fn)
    assert emu.tick_state["t"] == 10  # unchanged: the read no longer advances time
    assert emu.read32(ab.TICK_ADDRESS) == 10


def test_settle_stops_at_the_first_silent_window() -> None:
    spin = ae.LOAD_BASE + 0x2000
    emu = emu_with({spin: words(B_SELF)})
    emu.set_reg(15, spin)
    emu.set_reg(13, ae.STACK_TOP - 0x1000)
    emu.port_writes.append((ae.PORT2_BASE, 1, 0))  # a message that settle must discard
    assert ast.settle(emu, window=300, max_windows=5) == 1
    assert emu.port_writes == []


def test_boot_to_main_loop_runs_the_thunk_up_to_the_loop() -> None:
    # thunk: mov r0, #1 ; b MAIN_LOOP   (MAIN_LOOP: mov r1, #2 would be the first loop insn)
    off = ast.MAIN_LOOP - (ab.APP_THUNK + 4) - 8
    emu = emu_with(
        {
            ab.APP_THUNK: words(0xE3A00001, 0xEA000000 | ((off >> 2) & 0xFFFFFF)),
            ast.MAIN_LOOP: words(0xE3A01002),
        }
    )
    pc, _n, err = ast.boot_to_main_loop(emu, 1000)
    assert (pc, err) == (ast.MAIN_LOOP, "")
    assert emu.reg(0) == 1
    assert emu.reg(1) == 0
