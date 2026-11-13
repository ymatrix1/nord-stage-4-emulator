"""Unit tests for tools/arm_zone_capture.py with synthetic memory and a fake rig."""

from __future__ import annotations

import faulthandler
import gc
import struct

import arm_boot as ab
import arm_emu as ae
import arm_zone_capture as zc
import arm_zones as az
import pytest

pytest.importorskip("unicorn")
faulthandler.disable()

BX_LR = 0xE12FFF1E
BIG = 0x600000
OBJ = ae.SRAM_BASE + 0x10000


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


def message(dm_word: int, value: int, core: int = 1, chip: str = "PNO") -> ae.Message:
    """A path 2 message to PNO core `core`: `word1` is the system byte address of the DM word."""
    base = 0x28800000 if core == 2 else 0x28000000
    return ae.Message(2, ae.COMM_NAMES.index(chip), base + 4 * dm_word, value)


class FakeRig:
    def __init__(self, emu: ae.ArmEmu) -> None:
        self.emu = emu
        self.log: list[tuple[str, bytes | int]] = []

    def send(self, data: bytes) -> None:
        self.log.append(("send", data))

    def tick(self, count: int = 1) -> list[tuple[int, ae.Message]]:
        self.log.append(("tick", count))
        return []


def test_sweep_points_lists_notes_then_velocities_without_duplicates() -> None:
    pts = zc.sweep_points((36, 60), (32, 100, 127), 60, 100)
    assert pts == [(36, 100), (60, 100), (60, 32), (60, 127)]


def test_engine_flags_reads_rows_and_slots() -> None:
    emu = emu_with({})
    for row, slot in ((0, 0), (7, 1), (95, 19)):
        emu.write_bytes(OBJ + zc.FLAG_OFFSET + zc.FLAG_ROW_STRIDE * row + slot, b"\x01")
    assert zc.engine_flags(emu, OBJ) == [(0, 0), (7, 1), (95, 19)]


def test_engine_levels_reads_every_eighth_byte_word() -> None:
    emu = emu_with({})
    emu.write32(OBJ + zc.LEVEL_OFFSET + 8 * 2, 2076180)
    levels = zc.engine_levels(emu, OBJ)
    assert levels[2] == 2076180 and sum(levels) == 2076180 and len(levels) == zc.FLAG_SLOTS
    assert levels[2] / 2**23 == pytest.approx(0.24748, abs=1e-4)


def test_rows_for_formats_derived_values_only() -> None:
    msgs = [(3, message(0x95E5E, 0x3E7D70A0))]
    assert zc.rows_for("organ", 1, 60, 100, "on", msgs) == [
        "organ,1,60,100,on,3,PNO,1,2,0x95e5e,0x3e7d70a0"
    ]


def test_msg_script_keeps_only_the_requested_core() -> None:
    blocks = [
        (0, "install", [(0, message(0x959AF, 0))]),
        (1, "on", [(26, message(0x95E6A, 0x3E7D70A0)), (26, message(0xC1DCC, 0x0B00E200, core=2))]),
    ]
    text = zc.msg_script(blocks, core=1)
    assert text == "0 0x959af 0x00000000  # install +0 ms\n1 0x95e6a 0x3e7d70a0  # on +26 ms\n"
    assert zc.msg_script(blocks, core=2) == "1 0xc1dcc 0x0b00e200  # on +26 ms\n"


def test_keep_switches_zones_of_the_other_layers_off() -> None:
    emu = emu_with({})
    bases = [OBJ + 0x4000 * i for i in range(7)]
    for i, b in enumerate(bases):
        emu.write32(b + az.SLOT_OFFSET, 0)
        emu.write32(b + 4 * (az.ZONE_TABLE + az.NOTE_CLASS * 12) + 4, OBJ + 0x40000 + 0x10 * i)
    setup = zc.Setup(FakeRig(emu), bases, {})  # type: ignore[arg-type]
    setup.keep({0, 1})
    assert [az.zone_enabled(emu, b) for b in bases] == [True, True] + [False] * 5
    setup.keep({4})
    assert [az.zone_enabled(emu, b) for b in bases] == [False] * 4 + [True, False, False]


def test_key_run_sends_note_on_then_off_and_labels_phases(monkeypatch: pytest.MonkeyPatch) -> None:
    emu = emu_with({})
    rig = FakeRig(emu)
    setup = zc.Setup(rig, [], {})  # type: ignore[arg-type]
    monkeypatch.setattr(setup, "reset", lambda: rig.log.append(("reset", 0)))
    rows, on, off = zc.key_run(setup, "organ", 3, 60, 100, 5, 7, notes=(64,))
    assert rig.log == [
        ("reset", 0),
        ("send", bytes([0x90, 60, 100])),
        ("send", bytes([0x90, 64, 100])),
        ("tick", 5),
        ("send", bytes([0x80, 60, 0])),
        ("send", bytes([0x80, 64, 0])),
        ("tick", 7),
    ]
    assert rows == [] and on == [] and off == []


def test_find_engine_hooks_the_weight_drain(monkeypatch: pytest.MonkeyPatch) -> None:
    emu = emu_with({zc.WEIGHT_DRAIN: struct.pack("<I", BX_LR)})
    rig = FakeRig(emu)
    setup = zc.Setup(rig, [], {})  # type: ignore[arg-type]
    monkeypatch.setattr(setup, "reset", lambda: None)

    def send(data: bytes) -> None:
        emu.set_reg(5, OBJ)
        emu.call(zc.WEIGHT_DRAIN)

    rig.send = send  # type: ignore[method-assign]
    assert zc.find_engine(setup) == OBJ
    rig.send = lambda data: None  # type: ignore[method-assign]
    with pytest.raises(SystemExit):
        zc.find_engine(setup)


def test_flag_table_lists_the_flags_of_each_note(monkeypatch: pytest.MonkeyPatch) -> None:
    emu = emu_with({})
    rig = FakeRig(emu)
    setup = zc.Setup(rig, [], {})  # type: ignore[arg-type]
    current: list[int] = []
    monkeypatch.setattr(setup, "reset", lambda: None)

    def send(data: bytes) -> None:
        current.append(data[1])
        emu.write_bytes(OBJ + zc.FLAG_OFFSET, bytes(zc.FLAG_ROWS * zc.FLAG_ROW_STRIDE))
        row = data[1] - 36
        emu.write_bytes(OBJ + zc.FLAG_OFFSET + zc.FLAG_ROW_STRIDE * row + 2, b"\x01")

    rig.send = send  # type: ignore[method-assign]
    assert zc.flag_table(setup, OBJ, (36, 40)) == ["note,row,slot", "36,0,2", "40,4,2"]
