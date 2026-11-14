"""Unit tests for tools/arm_organ_weights.py (pure helpers and hand-assembled ARM snippets)."""

from __future__ import annotations

import faulthandler
import gc
import struct

import arm_boot as ab
import arm_emu as ae
import arm_organ_weights as ow
import arm_steady as ast
import pytest

pytest.importorskip("unicorn")
faulthandler.disable()

BX_LR = 0xE12FFF1E
B_SELF = 0xEAFFFFFE
BIG = 0x600000
SPIN = ae.LOAD_BASE + 0x4000
PNO_PORT2 = ae.PORT2_BASE + 0x10  # comm 1


def words(*ws: int) -> bytes:
    return b"".join(struct.pack("<I", w & 0xFFFFFFFF) for w in ws)


def f2u(x: float) -> int:
    return struct.unpack("<I", struct.pack("<f", x))[0]


def wb_message(wheel: int, bits: int, set_index: int = 0) -> ae.Message:
    dm = ow.dm_address("wB", set_index, wheel)
    return ae.Message(2, 1, 0x28000000 + 4 * dm, bits)


@pytest.fixture(autouse=True)
def _release_emulators():
    yield
    gc.collect()


def test_classify_and_dm_address_are_inverse_and_respect_the_strides() -> None:
    for kind in ("a", "b", "wB"):
        for s in range(ow.SETS):
            for wheel in (0, 45, ow.WHEELS - 1):
                assert ow.classify(ow.dm_address(kind, s, wheel)) == (kind, s, wheel)
    assert ow.dm_address("a", 1, 0) == 0x9594F + 0x65
    assert ow.dm_address("b", 2, 3) == 0x95AE2 + 2 * 0x64 + 3
    assert ow.dm_address("wB", 3, 0) == 0x957DE + 3 * 0x5C
    assert ow.classify(0x9566E) is None  # wA is never sent
    assert ow.classify(0x957DE - 1) is None


def test_decode_message_only_accepts_pno_core1_weight_words() -> None:
    w = ow.decode_message(wb_message(36, f2u(0.5)))
    assert (w.kind, w.set, w.wheel, w.value) == ("wB", 0, 36, 0.5)
    assert ow.decode_message(ae.Message(1, 1, ow.WB_BASE, 0)) is None  # path 1
    assert ow.decode_message(ae.Message(2, 0, 0x28000000 + 4 * ow.WB_BASE, 0)) is None  # FX
    assert ow.decode_message(ae.Message(2, 1, 0x28000000, 0)) is None  # not a weight word


def test_fixed_to_float_is_the_firmware_sender_scale() -> None:
    # 127 * ((127 * 127) >> 4): full drawbar, velocity 127 (measured 0x3C7ABE.. in the run)
    n = 127 * ((127 * 127) >> 4)
    assert n == 128016
    assert ow.fixed_to_float(n) == pytest.approx(0.015260696, rel=1e-7)
    assert ow.fixed_to_float(1) == 2.0**-23


def test_fold_and_expected_wheels_match_the_engine_tables() -> None:
    # values read from the engine item tables of the booted firmware (keymap command)
    assert ow.expected_wheels(60) == (24, 43, 36, 48, 55, 60, 64, 67, 72)
    assert ow.expected_wheels(0) == (0, 7, 0, 0, 7, 0, 4, 7, 12)
    assert ow.expected_wheels(127) == (79, 86, 79, 79, 86, 79, 83, 86, 79)
    assert ow.fold_wheel(-1) == 11
    assert ow.fold_wheel(91) == 79
    assert ow.fold_wheel(90) == 90


def test_model_wb_ints_adds_keys_and_drawbars_on_a_shared_wheel() -> None:
    levels = tuple(ow.DRAWBAR_7BIT[i] for i in (8, 0, 0, 0, 0, 0, 0, 0, 0))
    one = ow.model_wb_ints([(60, 127)], levels)
    assert one == {24: 127 * ((127 * 127) >> 4)}
    # notes 60 and 72: the 16' wheel of 72 is wheel 36, which is not the 16' wheel of 60
    two = ow.model_wb_ints([(60, 127), (72, 127)], levels)
    assert sorted(two) == [24, 36]
    # velocity scales through the integer shift
    assert ow.model_wb_ints([(60, 1)], levels) == {24: 127 * (127 >> 4)}
    # a drawbar at 0 contributes nothing
    assert ow.model_wb_ints([(60, 127)], (0,) * 9) == {}


def test_key_table_reads_wheels_and_taper_through_the_item_pointers() -> None:
    mem: dict[int, int] = {}
    engine, sub, items = 0x1000, 0x2000, 0x3000
    mem[engine + 0x14] = sub
    mem[sub + 0x28] = items
    for n in range(2):
        item = 0x4000 + 0x100 * n
        mem[items + 4 * n] = item
        mem[item] = 0x5000 + 0x40 * n
        mem[item + 4] = 0x6000 + 0x40 * n
        for j in range(9):
            mem[0x5000 + 0x40 * n + 4 * j] = n + j
            mem[0x6000 + 0x40 * n + 4 * j] = 127
    table = ow.key_table(mem.__getitem__, engine, notes=2)
    assert table[1] == ow.KeyEntry(1, tuple(1 + j for j in range(9)), (127,) * 9)
    csv_text = ow.keymap_csv(table)
    lines = csv_text.strip().split("\n")
    assert lines[0].startswith("note,wheel_16,wheel_5_1o3,wheel_8")
    assert lines[1].split(",")[:3] == ["0", "0", "1"]
    assert len(lines[0].split(",")) == 19


def test_script_lines_keeps_weight_words_of_one_set() -> None:
    msgs = [
        wb_message(3, 0x3F800000),
        wb_message(4, 0x40000000, set_index=1),
        ae.Message(1, 1, 5, 6),
    ]
    text = ow.script_lines(msgs, call=2, only_set=0)
    assert text == f"2 {ow.WB_BASE + 3:#x} 0x3f800000  # wB[3] set 0\n"
    assert ow.script_lines(msgs, call=0).count("\n") == 2
    assert ow.script_lines([], call=1) == ""


# note-on / update stand-ins that send one PNO comm 1 message (address, pad 0, value); the note-on
# one sends the note read through the event pointer (r2), the update one a fixed word.
SEND_HEAD = words(0xE59FC018, 0xE59F1018, 0xE58C1000, 0xE3A01000, 0xE58C1000)
NOTE_ON_CODE = (
    SEND_HEAD + words(0xE5921000, 0xE58C1000, BX_LR) + words(PNO_PORT2, 0x28000000 + 4 * ow.WB_BASE)
)
UPDATE_CODE = (
    SEND_HEAD
    + words(0xE59F100C, 0xE58C1000, BX_LR)
    + words(PNO_PORT2, 0x28000000 + 4 * (ow.WB_BASE + 5))
    + words(0x3F800000)
)


def driver() -> ow.BankDriver:
    buf = bytearray(BIG)
    for address, code in (
        (ow.NOTE_ON, NOTE_ON_CODE),
        (ow.UPDATE, UPDATE_CODE),
        (SPIN, words(B_SELF)),
    ):
        buf[address - ae.LOAD_BASE : address - ae.LOAD_BASE + len(code)] = code
    emu = ae.ArmEmu(bytes(buf), ram_size=BIG)
    emu.uc.mem_map(0, ab.LOW_MEMORY)
    emu.install_dsp_ports()
    emu.set_reg(13, ae.STACK_TOP - 0x1000)
    emu.set_reg(15, SPIN)
    for s in range(ow.SETS):
        emu.write32(ow.BANK_TABLE + 4 * (s + 1), ae.SRAM_BASE + 0x1000 * (s + 1))
    return ow.BankDriver(emu, ast.snapshot(emu))


def test_driver_note_on_update_and_scenario_capture_the_sent_words() -> None:
    d = driver()
    assert d.engine(2) == ae.SRAM_BASE + 0x3000
    d.reset()
    msgs = d.note_on(0, 60, 100)
    assert [(m.dm_word, m.word2) for m in msgs] == [(ow.WB_BASE, 60)]
    d.set_drawbars(1, (8, 7, 6, 5, 4, 3, 2, 1, 0))
    eng = d.engine(1)
    assert [d.emu.read32(eng + ow.DRAWBAR_OFFSET + 4 * j) for j in range(9)] == [
        8,
        7,
        6,
        5,
        4,
        3,
        2,
        1,
        0,
    ]
    assert d.emu.read_bytes(eng + ow.FLAG_OFFSET, 2) == bytes([1, 1])
    on, up = d.scenario([(61, 127), (62, 127)], (8,) * 9, set_index=0)
    assert [w.bits for w in on.weights] == [61, 62]
    assert up.wb(0) == {5: 0x3F800000}
    assert on.wb(1) == {}


def test_verify_reports_a_mismatch_with_the_model() -> None:
    d = driver()
    bad = ow.verify(d, [60])
    assert len(bad) == 1 and bad[0].startswith("note 60")


def test_curve_csv_formats_bits_and_value() -> None:
    text = ow.curve_csv([("drawbar", 8, 36, 0x3C7ABE00, 0x3C7ABE00, 127)])
    head, row = text.strip().split("\n")
    assert head == "sweep,value,wheel,wB_bits,model_bits,d7,wB"
    assert row.startswith("drawbar,8,36,0x3c7abe00,0x3c7abe00,127,0.0")


def test_curve_rows_compare_the_driver_output_with_the_model() -> None:
    rows = ow.curve_rows(driver(), note=60, drawbar=2)
    assert len(rows) == 14  # 9 drawbar indices + 5 velocities
    assert {r[0] for r in rows} == {"drawbar", "velocity"}
    assert all(r[2] == 36 for r in rows)  # 8' of note 60 sounds wheel 36
    assert [r[5] for r in rows[:9]] == list(ow.DRAWBAR_7BIT)
