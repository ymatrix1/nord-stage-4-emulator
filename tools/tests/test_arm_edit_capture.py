"""Unit tests for tools/arm_edit_capture.py with hand-assembled ARM snippets (no firmware)."""

from __future__ import annotations

import faulthandler
import gc
import struct

import arm_boot as ab
import arm_boot_full as bf
import arm_edit_capture as cap
import arm_emu as ae
import arm_steady as ast
import pytest

pytest.importorskip("unicorn")
faulthandler.disable()

BX_LR = 0xE12FFF1E
B_SELF = 0xEAFFFFFE  # b .
BIG = 0x600000
SPIN = ae.LOAD_BASE + 0x4000


def words(*ws: int) -> bytes:
    return b"".join(struct.pack("<I", w & 0xFFFFFFFF) for w in ws)


# dispatch(store, 0, id, &value): sends the path 1 message (id, [r3]) to comm 0 (FX)
SENDER = words(0xE5930000, 0xE59F1008, 0xE5812000, 0xE5810000, BX_LR, ae.PORT1_BASE)


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


def running_emu() -> ae.ArmEmu:
    emu = emu_with({cap.DISPATCH: SENDER, SPIN: words(B_SELF)})
    emu.set_reg(13, ae.STACK_TOP - 0x1000)
    emu.set_reg(15, SPIN)
    return emu


def build_dispatcher(emu: ae.ArmEmu, kinds: list[int]) -> list[int]:
    """Fake dispatcher with one layer per entry of `kinds`; returns the store addresses."""
    g = ae.SRAM_BASE + 0x1000
    emu.write32(ab.DISPATCHER_GLOBAL, g)
    emu.write32(g + 0x14C, len(kinds))
    vtable = ae.SRAM_BASE + 0x2000
    emu.write32(vtable + 0x20, cap.STORE_SETTERS[0])
    stores = []
    for i, kind in enumerate(kinds):
        layer = ae.SRAM_BASE + 0x10000 + 0x10000 * i
        emu.write32(g + 0x150 + 4 * i, layer + cap.CLIENT_OFFSET)
        emu.write32(layer, kind)
        for k, off in enumerate((0x56C8, 0x56CC)):
            store = ae.SRAM_BASE + 0x4000 + 0x100 * (2 * i + k)
            emu.write32(store, vtable)
            emu.write32(layer + off, store)
            stores.append(store)
    return stores


def build_value_table(emu: ae.ArmEmu, store: int, edit_id: int, initial: int = 0) -> int:
    """Value table behind `store` (parent, table pointer, id array, cells); returns the cell."""
    parent = ae.SRAM_BASE + 0x6000
    table_ptr = ae.SRAM_BASE + 0x6100
    table = ae.SRAM_BASE + 0x7000
    array = ae.SRAM_BASE + 0x8000
    cells = ae.SRAM_BASE + 0x9000
    emu.write32(store + cap.PARENT_OFFSET, parent)
    emu.write32(parent + cap.TABLE_OFFSET, table_ptr)
    emu.write32(table_ptr, table)
    emu.write32(table + 4 * edit_id, array)
    emu.write32(array, cells)
    emu.write32(cells, initial)
    return cells


def test_value_cell_follows_parent_table_array_cells() -> None:
    emu = emu_with({})
    store = ae.SRAM_BASE + 0x5000
    cell = build_value_table(emu, store, 0x166)
    assert cap.value_cell(emu, store, 0x166) == cell
    assert cap.value_cell(emu, store, 0x166, index=2) == cell + 8
    assert cap.value_cell(emu, store, 0x167) is None  # the layer does not own this id
    assert cap.value_cell(emu, 0x1234, 0x166) is None  # not a store


def test_baseline_keys_collects_the_idle_traffic() -> None:
    # a loop that stores one path 1 message (word1 = 5, word2 = 6) then spins
    code = words(0xE59F1010, 0xE3A02005, 0xE5812000, 0xE3A02006, 0xE5812000, B_SELF, ae.PORT1_BASE)
    emu = emu_with({SPIN: code})
    emu.set_reg(13, ae.STACK_TOP - 0x1000)
    emu.set_reg(15, SPIN)
    keys = cap.baseline_keys(emu, windows=2, window=50)
    assert keys == {("FX", 0, 1, 5)}


def test_value_set_covers_a_small_range_and_samples_a_large_one() -> None:
    assert cap.value_set(0, 5) == [0, 1, 2, 3, 4, 5]
    assert cap.value_set(-6, 4) == list(range(-6, 5))
    assert cap.value_set(0, 127) == [0, 31, 63, 96, 127]


def test_value_set_dense_and_cell_word_encodings() -> None:
    assert cap.value_set(0, 127, dense=True) == list(range(128))
    assert cap.value_set(0, 4095, dense=True) == list(range(4096))
    assert cap.value_set(0, 100000, dense=True) == cap.value_set(0, 100000)
    knob = cap.Param(0x134, "Edt_Syn_kFltFreq", 0, 127)
    enum = cap.Param(0x133, "Edt_Syn_kFltType", 0, 5)
    assert cap.cell_word(knob, 64) == 64 << 16  # auto: Q16.16 for ranges of 12 steps or more
    assert cap.cell_word(enum, 3) == 3  # auto: plain integer for a small range
    assert cap.cell_word(enum, 3, "q16") == 3 << 16
    assert cap.cell_word(knob, 64, "raw") == 64
    assert cap.cell_word(cap.Param(0x1, "x", -60, 60), -1) == 0xFFFF0000  # negative stays 32 bit


def test_load_params_filters_by_name(tmp_path) -> None:
    f = tmp_path / "p.csv"
    f.write_text(
        "index,id,domain,name,min,max,default,w\n"
        "0,0x16c,0xd,Edt_Org_Preset_kVibratoEnable,0,1,0,1\n"
        "1,0x1,0x1,Fnc_Volume,0,127,127,1\n",
        encoding="utf-8",
    )
    got = cap.load_params(f, r"Edt_Org")
    assert got == [cap.Param(0x16C, "Edt_Org_Preset_kVibratoEnable", 0, 1)]


def test_find_stores_returns_both_stores_per_layer_and_names_the_kind() -> None:
    emu = emu_with({})
    addrs = build_dispatcher(emu, [0xC027AD8C, 0xC027ACA0, 0x1234])
    found = cap.find_stores(emu)
    assert [s.address for s in found] == addrs
    assert [s.kind for s in found] == ["organ", "organ", "piano", "piano", "other", "other"]
    assert [s.layer for s in found] == [0, 0, 1, 1, 2, 2]


def test_find_stores_skips_pointers_that_are_not_stores() -> None:
    emu = emu_with({})
    build_dispatcher(emu, [0xC027AD8C])
    layer = ae.SRAM_BASE + 0x10000
    emu.write32(layer + 0x56CC, 7)  # a plain integer in the second slot
    assert len(cap.find_stores(emu)) == 1


def test_first_store_per_layer_keeps_one_store_per_layer() -> None:
    stores = [cap.Store(0, "organ", 1), cap.Store(0, "organ", 2), cap.Store(1, "piano", 3)]
    assert [s.address for s in cap.first_store_per_layer(stores)] == [1, 3]


def test_apply_value_captures_the_call_messages() -> None:
    emu = running_emu()
    emu.uc.mem_map(cap.SCRATCH, ae.PAGE)
    store = ae.SRAM_BASE + 0x5000
    cell = build_value_table(emu, store, 0x16C)
    during, later, err = cap.apply_value(emu, store=store, edit_id=0x16C, value=1, window=1000)
    assert emu.read32(cell) == 1  # the new value was written to the cell before the dispatch
    assert err == ""
    assert [(m.path, m.chip, m.word1, m.word2) for m in during] == [(1, "FX", 0x16C, 1)]
    assert later == []
    # the interrupted program resumed unchanged
    assert emu.reg(15) == SPIN


def test_apply_value_reports_a_fault_in_the_dispatch() -> None:
    # r0 = 0xFFFFFFFC ; ldr r0, [r0]: the last word of the address space cannot be auto-mapped
    emu = emu_with({cap.DISPATCH: words(0xE3E00003, 0xE5900000), SPIN: words(B_SELF)})
    emu.set_reg(13, ae.STACK_TOP - 0x1000)
    emu.set_reg(15, SPIN)
    emu.uc.mem_map(cap.SCRATCH, ae.PAGE)
    _during, _later, err = cap.apply_value(emu, 0, 1, 0, window=100)
    assert err


def test_sweep_logs_messages_and_restores_after_a_store_that_spoke() -> None:
    emu = running_emu()
    param = cap.Param(0x16C, "Edt_Org_Preset_kVibratoEnable", 0, 1)
    stores = [
        cap.Store(0, "organ", ae.SRAM_BASE + 0x5000),
        cap.Store(1, "piano", ae.SRAM_BASE + 0x5100),
        cap.Store(2, "fx", ae.SRAM_BASE + 0x5200),
    ]
    build_value_table(emu, stores[0].address, 0x16C)
    build_value_table(emu, stores[1].address, 0x16C)
    # stores[2] owns no value cell for the id: it must be skipped
    snapshot = ast.snapshot(emu)
    rows = cap.sweep(emu, snapshot, stores, [param], window=500)
    # two values x two stores, one message each, all at the call stage
    assert len(rows) == 4
    assert {(r.value, r.store.kind, r.stage) for r in rows} == {
        (0, "organ", "call"),
        (1, "organ", "call"),
        (0, "piano", "call"),
        (1, "piano", "call"),
    }
    assert all(r.message.word1 == 0x16C for r in rows)
    noisy = cap.sweep(emu, snapshot, stores, [param], window=500, noise={("FX", 0, 1, 0x16C)})
    assert noisy == []  # the idle-traffic key filters every message


def test_rows_csv_has_a_header_and_one_line_per_message() -> None:
    param = cap.Param(0x166, "Edt_Org_kModel", 0, 5)
    row = cap.CaptureRow(
        param,
        3,
        cap.Store(0, "organ", 0x10),
        "tick",
        ae.Message(2, 1, 0x28000000 + 4 * 0x10, 0xAB, 0xC0001234),
    )
    text = cap.rows_csv([row])
    head, line = text.strip().split("\n")
    assert head.startswith("id,name,value,layer,kind,stage,chip,comm,core,path,dm_word")
    assert line == "0x166,Edt_Org_kModel,3,0,organ,tick,PNO,1,1,2,0x10,0x000000ab,0xc0001234,"


def test_settled_emulator_boots_settles_and_caches(tmp_path) -> None:
    buf = bytearray(BIG)
    for fn in bf.INIT_ARRAY:
        buf[fn - ae.LOAD_BASE : fn - ae.LOAD_BASE + 4] = words(BX_LR)
    # thunk: b MAIN_LOOP ; MAIN_LOOP: b .
    off = ast.MAIN_LOOP - (ab.APP_THUNK + 8)
    thunk = ab.APP_THUNK - ae.LOAD_BASE
    buf[thunk : thunk + 4] = words(0xEA000000 | ((off >> 2) & 0xFFFFFF))
    main = ast.MAIN_LOOP - ae.LOAD_BASE
    buf[main : main + 4] = words(B_SELF)
    code = tmp_path / "code.bin"
    code.write_bytes(bytes(buf))
    cache = tmp_path / "settled.snap"
    emu, blob = cap.settled_emulator(code, None, cache)
    assert cache.exists()
    assert emu.tick_state["step"] == 0
    assert isinstance(blob, bytes)
    emu2, _ = cap.settled_emulator(code, None, cache)  # second call loads the cache
    assert emu2.tick_state["step"] == 0


def sample_rows() -> list[cap.CaptureRow]:
    param = cap.Param(0x166, "Edt_Org_kModel", 0, 5)
    organ = cap.Store(0, "organ", 0x10)
    other = cap.Store(1, "piano", 0x20)
    pno = ae.Message(2, 1, 0x28000000 + 4 * 0x955B6, 0x635300, 0xC0001234)
    smp = ae.Message(1, 4, 0x9403E, 0x3F800000, 0xC0001238)
    return [
        cap.CaptureRow(param, 3, organ, "call", pno),
        cap.CaptureRow(param, 3, organ, "tick", ae.Message(2, 1, 0x28000000 + 4 * 0x955B7, 0)),
        cap.CaptureRow(param, 3, other, "call", pno),
        cap.CaptureRow(param, 4, organ, "call", smp),
        cap.CaptureRow(param, 5, organ, "error", ae.Message(0, -1, 0, 0), "boom"),
    ]


def test_summary_csv_groups_by_id_kind_and_value() -> None:
    lines = cap.summary_csv(sample_rows()).strip().split("\n")
    assert lines[0].startswith("id,name,kind,value,calls,ticks,destinations")
    assert lines[1:] == [
        "0x166,Edt_Org_kModel,organ,3,1,1,PNO/1/2,0x955b6,0x955b7,2,1",
        "0x166,Edt_Org_kModel,organ,4,1,0,SMP/0/1,0x9403e,0x9403e,1,1",
        "0x166,Edt_Org_kModel,piano,3,1,0,PNO/1/2,0x955b6,0x955b6,1,1",
    ]  # the error row has no message and is left out


def test_bank_script_keeps_only_pno_core1_path2_words() -> None:
    rows = sample_rows()
    text = cap.bank_script(rows, "Edt_Org_kModel", 3, call=2)
    assert text.splitlines() == [
        "2 0x955b6 0x00635300  # Edt_Org_kModel=3",
        "2 0x955b7 0x00000000  # Edt_Org_kModel=3",
        "2 0x955b6 0x00635300  # Edt_Org_kModel=3",
    ]
    only_organ = cap.bank_script(rows, "Edt_Org_kModel", 3, call=1, layer=0)
    assert only_organ.count("\n") == 2
    assert cap.bank_script(rows, "Edt_Org_kModel", 4, call=1) == ""  # SMP is not PNO
    assert cap.bank_script(rows, "nope", 3, call=1) == ""
