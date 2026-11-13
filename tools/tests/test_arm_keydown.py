"""Unit tests for tools/arm_keydown.py with hand-assembled ARM snippets (no firmware needed)."""

from __future__ import annotations

import faulthandler
import gc
import struct

import arm_boot as ab
import arm_emu as ae
import arm_keydown as kd
import arm_steady as ast
import pytest

pytest.importorskip("unicorn")
faulthandler.disable()

BX_LR = 0xE12FFF1E
BIG = 0x600000  # covers the firmware handler table at 0xC055F674 and the board routine
HANDLER = ae.LOAD_BASE + 0x1000
SCRATCH = ae.SRAM_BASE + 0x400


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


def uart_reader() -> bytes:
    """r2 = LSR ; r3 = RBR ; r4 = LSR ; r5 = RBR ; r6 = LSR ; r7 = IIR ; bx lr (r1 = base)."""
    return words(
        0xE59F1018,  # ldr r1, [pc, #0x18] -> literal UART2_BASE
        0xE5912014,  # ldr r2, [r1, #0x14]
        0xE5913000,  # ldr r3, [r1]
        0xE5914014,  # ldr r4, [r1, #0x14]
        0xE5915000,  # ldr r5, [r1]
        0xE5916014,  # ldr r6, [r1, #0x14]
        0xE5917008,  # ldr r7, [r1, #8]
        BX_LR,
        kd.UART2_BASE,
    )


def test_uart2_serves_bytes_and_status_bits_in_order() -> None:
    emu = emu_with({HANDLER: uart_reader()})
    uart = kd.Uart2([0x90, 0x3C]).install(emu)
    emu.call(HANDLER)
    assert emu.reg(2) == kd.LSR_READY
    assert emu.reg(3) == 0x90
    assert emu.reg(4) == kd.LSR_READY
    assert emu.reg(5) == 0x3C
    assert emu.reg(6) == kd.LSR_IDLE
    assert emu.reg(7) == kd.IIR_IDLE
    assert uart.reads == 2


def test_uart2_empty_queue_reports_idle_and_zero_byte() -> None:
    emu = emu_with({HANDLER: uart_reader()})
    uart = kd.Uart2().install(emu)
    emu.call(HANDLER)
    assert (emu.reg(2), emu.reg(3), emu.reg(7)) == (kd.LSR_IDLE, 0, kd.IIR_IDLE)
    assert uart.reads == 2  # RBR is read twice by the snippet; both return 0


def test_uart2_reports_receive_interrupt_while_data_is_queued() -> None:
    emu = emu_with({HANDLER: uart_reader()})
    kd.Uart2([1, 2, 3]).install(emu)
    emu.call(HANDLER)
    assert emu.reg(7) == kd.IIR_RX


def test_push_queues_bytes_and_runs_the_registered_isr() -> None:
    emu = emu_with({HANDLER: uart_reader()})
    emu.write32(ast.IRQ_TABLE + 4 * kd.UART2_IRQ, HANDLER)
    emu.set_reg(13, ae.STACK_TOP - 0x1000)
    uart = kd.Uart2().install(emu)
    assert uart.push(emu, b"\x90\x3c\x64") == HANDLER
    assert uart.queue == [0x64]  # the snippet consumed two of the three bytes
    assert uart.reads == 2


def test_push_without_a_registered_handler_returns_zero() -> None:
    emu = emu_with({})
    uart = kd.Uart2().install(emu)
    assert uart.push(emu, b"\x90") == 0
    assert uart.queue == [0x90]


def test_board_present_sets_the_two_flag_bytes_when_the_attach_routine_runs() -> None:
    emu = emu_with({kd.BOARD_ATTACH: words(BX_LR)})
    kd.install_board_present(emu)
    emu.write_bytes(SCRATCH, bytes(0x20))
    emu.call(kd.BOARD_ATTACH, SCRATCH)
    assert emu.read_bytes(SCRATCH + 0x10, 6) == bytes([0, 0, 1, 1, 0, 0])


def test_note_messages() -> None:
    assert kd.note_on(60, 100) == bytes([0x90, 60, 100])
    assert kd.note_on(0xFF, 0xFF, channel=17) == bytes([0x91, 0x7F, 0x7F])
    assert kd.note_off(60, channel=2) == bytes([0x82, 60, 0])


def test_ring_state_reads_size_head_and_tail() -> None:
    emu = emu_with({})
    for i, v in enumerate((0x90165780, 0x1000, 7, 3)):
        emu.write32(kd.UART_RING + 4 * i, v)
    assert kd.ring_state(emu) == (0x1000, 7, 3)


def test_hooks_are_real_only_when_every_slot_matches() -> None:
    emu = emu_with({})
    table = ae.SRAM_BASE + 0x2000
    emu.write32(kd.HOOK_BASE + kd.HOOK_POINTER, table)
    for slot, fn in kd.REAL_HOOKS.items():
        emu.write32(table + 4 * (slot + 2), fn)
    assert kd.hooks_are_real(emu)
    assert kd.real_hooks(emu)[2] == 0x800015A8
    emu.write32(table + 4 * (2 + 2), 0xC024D1E4)  # the board-absent stub of slot 2
    assert not kd.hooks_are_real(emu)


def test_rig_tick_groups_messages_by_tick(monkeypatch: pytest.MonkeyPatch) -> None:
    emu = emu_with({})
    calls: list[int] = []

    def fake_run(e: ae.ArmEmu, insns: int, per_tick: int = 0) -> tuple[int, str]:
        calls.append(insns)
        # one path 1 message on comm 4 per tick: port stores `(word1, word2)`
        e.port_writes += [
            (ae.PORT1_BASE + 0x10 * 4, 0x92848 + len(calls), 0),
            (ae.PORT1_BASE + 0x10 * 4, len(calls), 0),
        ]
        return 0, ""

    monkeypatch.setattr(ast, "run_ticking", fake_run)
    rig = kd.Rig(emu, kd.Uart2())
    out = rig.tick(3)
    assert calls == [kd.TICK_INSNS] * 3
    assert [(n, m.word2) for n, m in out] == [(0, 1), (1, 2), (2, 3)]
    assert all(m.chip == "SMP" for _, m in out)


def test_message_rows_are_derived_values_only() -> None:
    m = ae.Message(1, 4, 0x92848, 0x3F800000, 0xC00B7B68)
    rows = kd.message_rows("on", 60, 100, [(3, m)])
    assert rows == ["on,60,100,3,SMP,1,0,0x92848,0x3f800000"]
    assert kd.CSV_HEADER.split(",")[:5] == ["phase", "note", "velocity", "tick", "chip"]


def test_count_calls_counts_function_entries() -> None:
    emu = emu_with({HANDLER: words(0xE1A00000, BX_LR)})  # nop ; bx lr
    counts = kd.count_calls(emu, {HANDLER: "fn", HANDLER + 4: "second"})
    emu.call(HANDLER)
    emu.call(HANDLER)
    assert counts == {"fn": 2, "second": 2}


def test_capture_note_returns_on_and_off_rows(monkeypatch: pytest.MonkeyPatch) -> None:
    emu = emu_with({})
    rig = kd.Rig(emu, kd.Uart2())
    sent: list[bytes] = []
    monkeypatch.setattr(rig, "send", sent.append)
    monkeypatch.setattr(
        rig, "tick", lambda n=1: [(0, ae.Message(2, 1, 0x28000004, 0x3F800000))] if n else []
    )
    counts = {"x": 5}
    rows = kd.capture_note(rig, 60, 100, counts, on_ticks=1, off_ticks=1)
    assert sent == [kd.note_on(60, 100), kd.note_off(60)]
    assert counts == {"x": 0}
    assert [r.split(",")[0] for r in rows] == ["on", "off"]
