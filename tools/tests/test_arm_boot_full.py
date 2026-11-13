"""Unit tests for tools/arm_boot_full.py with hand-assembled ARM snippets (no firmware needed)."""

from __future__ import annotations

import faulthandler
import gc
import struct

import arm_boot as ab
import arm_boot_full as bf
import arm_emu as ae
import pytest

pytest.importorskip("unicorn")
faulthandler.disable()

BX_LR = 0xE12FFF1E
B_SELF = 0xEAFFFFFE  # b .
BIG = 0x300000


def words(*ws: int) -> bytes:
    return b"".join(struct.pack("<I", w & 0xFFFFFFFF) for w in ws)


def emu_with(patches: dict[int, bytes]) -> ae.ArmEmu:
    buf = bytearray(BIG)
    for address, code in patches.items():
        buf[address - ae.LOAD_BASE : address - ae.LOAD_BASE + len(code)] = code
    emu = ae.ArmEmu(bytes(buf), ram_size=BIG)
    emu.uc.mem_map(0, ab.LOW_MEMORY)  # `boot_full` re-zeroes low memory between chunks
    return emu


@pytest.fixture(autouse=True)
def _release_emulators():
    yield
    gc.collect()


def test_run_init_array_runs_every_initialiser_in_order() -> None:
    # r4 = 1, then +1, +1, +1: only an in-order run of all four functions ends with 4
    emu = emu_with(
        {
            bf.INIT_ARRAY[0]: words(0xE3A04001, BX_LR),
            bf.INIT_ARRAY[1]: words(0xE2844001, BX_LR),
            bf.INIT_ARRAY[2]: words(0xE2844001, BX_LR),
            bf.INIT_ARRAY[3]: words(0xE2844001, BX_LR),
        }
    )
    bf.run_init_array(emu)
    assert emu.reg(4) == 4


def test_run_init_array_does_not_skip_calls() -> None:
    # fn0 calls a helper (bl) that sets r5; the skip-calls harness would never reach it
    helper = ae.LOAD_BASE + 0x1000
    bl = 0xEB000000 | (((helper - (bf.INIT_ARRAY[0] + 4 + 8)) >> 2) & 0xFFFFFF)
    emu = emu_with(
        {
            bf.INIT_ARRAY[0]: words(0xE92D4000, bl, 0xE8BD8000),  # push {lr}; bl; pop {pc}
            helper: words(0xE3A05007, BX_LR),
            bf.INIT_ARRAY[1]: words(BX_LR),
            bf.INIT_ARRAY[2]: words(BX_LR),
            bf.INIT_ARRAY[3]: words(BX_LR),
        }
    )
    bf.run_init_array(emu)
    assert emu.reg(5) == 7


def test_install_ui_stubs_noop_the_ui_routines_and_map_the_high_page() -> None:
    emu = emu_with({bf.EVENT_INSERT: words(0xE3A00009, BX_LR)})
    bf.install_ui_stubs(emu)
    assert emu.call(bf.EVENT_INSERT) == 0  # the stub replaced `mov r0, #9`
    assert emu.call(bf.TEXT_MEASURE[0]) == 0
    assert emu.read32(0xFFFFFFFC) == 0  # a null font table dereference reads zero


def test_spi_transfer_stub_zeroes_the_result_word() -> None:
    emu = emu_with({bf.SPI_TRANSFER: words(0xE3A00009, BX_LR)})
    bf.install_ui_stubs(emu)
    out = ae.SRAM_BASE + 0x100
    emu.uc.mem_write(out, struct.pack("<I", 0x3F0077))
    # AAPCS: fifth argument on the stack; `call` places it at [sp]
    assert emu.call(bf.SPI_TRANSFER, 0, 0, 0, 1, out) == 0
    assert emu.read32(out) == 0


def test_install_ui_stubs_accepts_extra_addresses() -> None:
    extra = ae.LOAD_BASE + 0x2000
    emu = emu_with({extra: words(0xE3A00009, BX_LR)})
    bf.install_ui_stubs(emu, (extra,))
    assert emu.call(extra) == 0


def test_boot_full_finishes_when_the_thunk_returns() -> None:
    emu = emu_with({ab.APP_THUNK: words(BX_LR)})
    result = bf.boot_full(emu, max_insns=1000, chunk=100)
    assert result.finished
    assert result.error == ""


def test_boot_full_reports_the_spin_address() -> None:
    emu = emu_with({ab.APP_THUNK: words(B_SELF)})
    result = bf.boot_full(emu, max_insns=500, chunk=100)
    assert not result.finished
    assert result.pc == ab.APP_THUNK
    assert result.hot[0] == (ab.APP_THUNK, 5)
    assert result.instructions == 500


def test_boot_full_reports_a_fault() -> None:
    # r0 = 0xFFFFFFFC ; ldr r0, [r0]: the last word of the address space cannot be auto-mapped
    emu = emu_with({ab.APP_THUNK: words(0xE3E00003, 0xE5900000)})
    result = bf.boot_full(emu, max_insns=1000, chunk=100)
    assert result.error
    assert not result.finished


def test_prepare_builds_an_emulator_with_stubs_and_initialisers(tmp_path) -> None:
    buf = bytearray(BIG)
    for fn in bf.INIT_ARRAY:
        buf[fn - ae.LOAD_BASE : fn - ae.LOAD_BASE + 4] = words(BX_LR)
    code = tmp_path / "code.bin"
    code.write_bytes(bytes(buf))
    emu = bf.prepare(code, None)
    assert bf.EVENT_INSERT in emu.stubs
    assert ab.OPERATOR_NEW in emu.stubs
