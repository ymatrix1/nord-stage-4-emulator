"""Unit tests for tools/arm_model_paths.py with hand-assembled ARM snippets (no firmware)."""

from __future__ import annotations

import faulthandler
import gc
import struct

import arm_boot as ab
import arm_edit_capture as aec
import arm_emu as ae
import arm_model_paths as mp
import arm_steady as ast
import pytest

pytest.importorskip("unicorn")
faulthandler.disable()

BX_LR = 0xE12FFF1E
BIG = 0x600000
STORE = ae.SRAM_BASE + 0x1000
CHAIN = ae.SRAM_BASE + 0x2000
ROUTINE_BASE = 0x120000


def words(*ws: int) -> bytes:
    return b"".join(struct.pack("<I", w & 0xFFFFFFFF) for w in ws)


def branch(source: int, target: int) -> int:
    return 0xEA000000 | (((target - (source + 8)) >> 2) & 0xFFFFFF)


# handler(store, 0, &id, model): chain = [store + 0x98]; event = model; state = 0
HANDLER = words(0xE590C098, 0xE58C3090, 0xE3A01000, 0xE58C10B0, BX_LR)
# step(chain): state += 1; slot 2 gets routine ROUTINE_BASE + event (tail call of the setter)
STEP = words(
    0xE59010B0,  # ldr r1, [r0, #0xB0]
    0xE2811001,  # add r1, r1, #1
    0xE58010B0,  # str r1, [r0, #0xB0]
    0xE5901090,  # ldr r1, [r0, #0x90]
    0xE3A00002,  # mov r0, #2
    0xE2811812,  # add r1, r1, #0x120000
    branch(mp.CHAIN_STEP + 0x18, mp.SLOT_SETTER),
)


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


def fake_firmware() -> ae.ArmEmu:
    emu = emu_with(
        {
            mp.MODEL_HANDLER: HANDLER,
            mp.CHAIN_STEP: STEP,
            mp.SLOT_SETTER: words(BX_LR),
        }
    )
    emu.set_reg(13, ae.STACK_TOP - 0x1000)
    ab.install_tick(emu, step=3)
    emu.write32(STORE + mp.CHAIN_FIELD, CHAIN)
    emu.write32(CHAIN + mp.FIELD_STATE, mp.STATE_DONE)
    return emu


def test_install_model_records_the_slot_writes_of_the_three_steps() -> None:
    emu = fake_firmware()
    out = mp.install_model(emu, aec.Store(0, "organ", STORE), 3, "Pipe1")
    assert out.event == 3 and out.name == "Pipe1" and out.layer == 0
    assert out.writes == [mp.SlotWrite(s, 2, ROUTINE_BASE + 3) for s in (0, 1, 2)]
    assert out.routine(1) == out.routine(2) == ROUTINE_BASE + 3
    assert out.slot(2) == 2
    assert emu.read32(CHAIN + mp.FIELD_STATE) == mp.STATE_DONE


def test_install_model_runs_the_main_loop_first_when_ticks_are_asked(monkeypatch) -> None:
    emu = fake_firmware()
    seen: list[int] = []
    monkeypatch.setattr(ast, "run_ticking", lambda _e, insns: seen.append(insns) or (0, ""))
    out = mp.install_model(emu, aec.Store(1, "organ", STORE), 1, ticks=1234)
    assert seen == [1234] and out.event == 1 and len(out.writes) == 3


def test_a_chain_that_never_finishes_is_bounded() -> None:
    emu = emu_with(
        {mp.CHAIN_STEP: words(BX_LR), mp.MODEL_HANDLER: HANDLER, mp.SLOT_SETTER: words(BX_LR)}
    )
    emu.set_reg(13, ae.STACK_TOP - 0x1000)
    emu.write32(CHAIN + mp.FIELD_STATE, 0)
    assert mp.step_chain(emu, CHAIN) == mp.MAX_STEPS


def test_idle_slots_are_not_reported_as_routines() -> None:
    row = mp.ModelInstall(0, "B3", 0, 0)
    row.writes = [
        mp.SlotWrite(0, 4, mp.IDLE_ROUTINE),
        mp.SlotWrite(1, 2, mp.IDLE_ROUTINE),
        mp.SlotWrite(1, 3, mp.IDLE_ROUTINE),
        mp.SlotWrite(2, 4, 0x121E2B),
    ]
    assert row.routine(1) == 0 and row.slot(1) == 3  # last (idle) write decides the slot
    assert row.routine(2) == 0x121E2B and row.slot(2) == 4
    assert row.routine(7) == 0 and row.slot(7) == -1


def test_record_pointer_is_the_word_after_the_slot_entry() -> None:
    row = mp.ModelInstall(1, "Vox", 0, 1)
    row.writes = [mp.SlotWrite(2, 4, 0x1229B3)]
    row.words = {mp.SLOT_TABLE_DM + 4 * 4 + 2: 0xE2D42}
    assert row.record() == 0xE2D42
    assert mp.ModelInstall(0, "", 0, 0).record() is None


def test_parse_names_skips_padding_and_stops_at_count() -> None:
    pool = (
        b"Cat\x88\x00\x00\x00\x00B3\x00\x00Vox\x00Farfisa\x00Pipe1\x00\x00\x00"
        b"Pipe2\x00\x00\x00B3 Bass\x00<empty>\x00"
    )
    code = bytes(0x40) + pool
    start = ae.LOAD_BASE + 0x40 + pool.index(b"B3\x00")
    assert mp.parse_names(code, start, 6) == ["B3", "Vox", "Farfisa", "Pipe1", "Pipe2", "B3 Bass"]
    assert mp.parse_names(code, start, 2) == ["B3", "Vox"]


def test_ranges_merge_consecutive_words() -> None:
    assert mp.ranges([5, 3, 4, 9, 11, 10, 3]) == [(3, 5), (9, 11)]
    assert mp.ranges([]) == []


def test_pno_words_keep_only_core_1_path_2_messages() -> None:
    ok = ae.Message(2, 1, 0x28000000 + 4 * 0xC01E8, 0xE2D42)
    core2 = ae.Message(2, 1, 0x28800000 + 4 * 0x10, 7)
    other_chip = ae.Message(2, 0, 0x28000000 + 4 * 0x20, 8)
    path1 = ae.Message(1, 1, 0x30, 9)
    assert mp.pno_words([ok, core2, other_chip, path1]) == {0xC01E8: 0xE2D42}


def test_extra_words_list_values_that_differ_from_the_baseline() -> None:
    base = mp.ModelInstall(0, "B3", 0, 0)
    base.words = {1: 5, 2: 6}
    other = mp.ModelInstall(3, "Pipe1", 0, 3)
    other.words = {1: 5, 2: 7, 9: 1}
    assert mp.extra_words(other, base) == [2, 9]


def test_table_rows_and_csv() -> None:
    a = mp.ModelInstall(0, "B3", 0, 0)
    a.writes = [mp.SlotWrite(1, 3, mp.IDLE_ROUTINE), mp.SlotWrite(2, 4, 0x121E2B)]
    a.words = {mp.SLOT_TABLE_DM + 4 * 4 + 2: 0xB472C}
    b = mp.ModelInstall(1, "Vox", 0, 1)
    b.writes = [mp.SlotWrite(1, 2, 0x12272D), mp.SlotWrite(2, 4, 0x1229B3)]
    b.words = {mp.SLOT_TABLE_DM + 4 * 4 + 2: 0xE2D42, 0xE20E3: 0, 0x95FC6: 1, 0x95FC7: 2}
    rows = mp.table_rows([b, a])
    assert {r["model"] for r in rows} == {"0", "1"}
    by = {r["model"]: r for r in rows}
    assert by["0"]["generator"] == "idle" and by["0"]["processor"] == "0x121e2b"
    assert by["0"]["record_i1"] == "0xb472c" and by["0"]["extra_dm"] == ""
    assert by["1"]["generator"] == "0x12272d" and by["1"]["generator_slot"] == "2"
    assert by["1"]["record_i1"] == "0xe2d42"
    assert by["1"]["extra_dm"] == "0x95fc6-0x95fc7;0xc01e8;0xe20e3"
    text = mp.table_csv(rows)
    assert text.splitlines()[0].startswith("layer,model,name,event,generator_slot")
    assert len(text.splitlines()) == 3


def test_routine_names_fall_back_to_the_address() -> None:
    assert mp.routine_name(0) == "idle"
    assert mp.routine_name(0x12272D).startswith("oscillator bank")
    assert mp.routine_name(0x123456) == "0x123456"


def test_sweep_restores_the_settled_state_for_every_model() -> None:
    emu = fake_firmware()
    blob = ast.snapshot(emu)
    stores = [aec.Store(0, "organ", STORE), aec.Store(2, "piano", STORE)]
    rows = mp.sweep(emu, blob, stores, [3, 4], ["a", "b", "c", "d", "e"])
    assert [(r.layer, r.model, r.name) for r in rows] == [(0, 3, "d"), (0, 4, "e")]
    assert [r.routine(2) for r in rows] == [ROUTINE_BASE + 3, ROUTINE_BASE + 4]
