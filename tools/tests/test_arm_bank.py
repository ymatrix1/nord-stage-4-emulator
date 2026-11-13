"""Unit tests for tools/arm_bank.py with hand-assembled ARM snippets (no firmware needed)."""

from __future__ import annotations

import faulthandler
import gc
import struct

import arm_bank as bank
import arm_emu as ae
import pytest

pytest.importorskip("unicorn")
faulthandler.disable()

BIG = 0x300000


def words(*ws: int) -> bytes:
    return b"".join(struct.pack("<I", w & 0xFFFFFFFF) for w in ws)


@pytest.fixture(autouse=True)
def _release_emulators():
    yield
    gc.collect()


def emu_with(address: int, code: bytes) -> ae.ArmEmu:
    buf = bytearray(BIG)
    buf[address - ae.LOAD_BASE : address - ae.LOAD_BASE + len(code)] = code
    return ae.ArmEmu(bytes(buf), ram_size=BIG)


# fake bank routine: setter = [[this + 0x10]] slot 6 ; setter(this', 0, 0, 5) with 0xAA on the stack
FAKE_ROUTINE = words(
    0xE92D4000,  # push {lr}
    0xE24DD008,  # sub sp, sp, #8
    0xE5900010,  # ldr r0, [r0, #0x10]
    0xE5902000,  # ldr r2, [r0]
    0xE5922018,  # ldr r2, [r2, #0x18]
    0xE3A03005,  # mov r3, #5
    0xE3A0C0AA,  # mov ip, #0xAA
    0xE58DC000,  # str ip, [sp]
    0xE12FFF32,  # blx r2
    0xE28DD008,  # add sp, sp, #8
    0xE8BD8000,  # pop {pc}
)


def test_program_bank_logs_setter_calls() -> None:
    emu = emu_with(bank.BANK_PROGRAM, FAKE_ROUTINE)
    first = bank.program_bank(emu, level=1)
    second = bank.program_bank(emu, level=2)
    assert first.gains == [(5, 0xAA)]
    assert second.gains == [(5, 0xAA)]  # one entry per call: the stub is installed once
    assert first.level == 1 and second.level == 2


def test_program_bank_flag_is_written_to_the_object() -> None:
    emu = emu_with(bank.BANK_PROGRAM, FAKE_ROUTINE)
    bank.program_bank(emu, level=0, flag=1)
    assert emu.read_bytes(bank.FAKE + 0x1000 + 0x4E, 1) == b"\x01"


def test_bank_script_lines() -> None:
    prog = bank.BankProgram(1, 0, [], [(0x955B6, 0x635000), (0x955B7, 0x633E00)])
    assert bank.bank_script(prog, call=2) == "2 0x955b6 0x00635000\n2 0x955b7 0x00633e00\n"
    assert bank.bank_script(bank.BankProgram(0, 0, [], [])) == ""


def test_drawbar_curve_reads_nine_words_from_the_image() -> None:
    off = bank.DRAWBAR_CURVE_TABLE - ae.LOAD_BASE
    code = bytearray(off + 64)
    code[off : off + 36] = words(0, 3, 6, 13, 22, 39, 62, 87, 127)
    assert bank.drawbar_curve(bytes(code)) == [0, 3, 6, 13, 22, 39, 62, 87, 127]


def test_curve_db_is_relative_to_the_top_entry() -> None:
    db = bank.curve_db([0, 64, 128])
    assert db[0] is None
    assert db[2] == pytest.approx(0.0)
    assert db[1] == pytest.approx(-6.0206, abs=1e-3)
