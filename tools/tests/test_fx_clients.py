"""Unit tests for tools/fx_clients.py on a synthetic heap (no firmware)."""

from __future__ import annotations

import faulthandler
import gc

import arm_edit_capture as cap
import arm_emu as ae
import fx_clients as fxc
import pytest

pytest.importorskip("unicorn")
faulthandler.disable()

HEAP = 0x90100000
MARKER_ID = 0x1F0


@pytest.fixture(autouse=True)
def _release_emulators():
    yield
    gc.collect()


def make_emu() -> ae.ArmEmu:
    return ae.ArmEmu(bytes(0x1000), ram_size=0x1000)


def put_unit(emu: ae.ArmEmu, address: int, index: int, part: int) -> None:
    emu.write32(address, fxc.UNIT_VTABLE)
    emu.write32(address + fxc.UNIT_INDEX_OFFSET, index)
    emu.write32(address + fxc.UNIT_PART_OFFSET, part)


def put_marker_table(emu: ae.ArmEmu, unit: int, edit_id: int) -> None:
    """Value table behind `unit` so that `value_cell(unit, edit_id)` is not None."""
    parent, table_ptr, table = HEAP + 0x8000, HEAP + 0x8100, HEAP + 0x8200
    array, cells = HEAP + 0x8300, HEAP + 0x8400
    emu.write32(unit + cap.PARENT_OFFSET, parent)
    emu.write32(parent + cap.TABLE_OFFSET, table_ptr)
    emu.write32(table_ptr, table)
    emu.write32(table + 4 * edit_id, array)
    emu.write32(array, cells)


def test_scan_word_finds_aligned_hits_in_range() -> None:
    emu = make_emu()
    emu.write32(HEAP + 0x10, 0xDEADBEEF)
    emu.write32(HEAP + 0x40, 0xDEADBEEF)
    emu.write32(0x91F00000, 0xDEADBEEF)
    assert fxc.scan_word(emu, 0xDEADBEEF, HEAP, HEAP + 0x1000) == [HEAP + 0x10, HEAP + 0x40]
    assert fxc.scan_word(emu, 0x12345678, HEAP, HEAP + 0x1000) == []


def test_find_units_sorts_by_index_and_drops_stray_vtable_copies() -> None:
    emu = make_emu()
    put_unit(emu, HEAP + 0x1000, 7, HEAP + 0x100)
    put_unit(emu, HEAP + 0x2000, 3, HEAP + 0x200)
    put_unit(emu, HEAP + 0x3000, 0x40, HEAP + 0x300)  # a copy of the vtable pointer, not a unit
    units = fxc.find_units(emu)
    assert [(u.index, u.address, u.part) for u in units] == [
        (3, HEAP + 0x2000, HEAP + 0x200),
        (7, HEAP + 0x1000, HEAP + 0x100),
    ]
    assert {u.kind for u in units} == {"other"}


def test_unit_kind_uses_the_marker_ids() -> None:
    emu = make_emu()
    put_unit(emu, HEAP + 0x1000, 0, HEAP + 0x100)
    markers = {"Edt_PnoCom_kEffectsSectionModePno_private": MARKER_ID}
    assert fxc.unit_kind(emu, HEAP + 0x1000, markers) == "other"
    put_marker_table(emu, HEAP + 0x1000, MARKER_ID)
    assert fxc.unit_kind(emu, HEAP + 0x1000, markers) == "piano"
    assert fxc.unit_kind(emu, HEAP + 0x1000, {}) == "other"


def test_module_objects_and_rows() -> None:
    emu = make_emu()
    unit = HEAP + 0x1000
    put_unit(emu, unit, 1, 0)
    for k, off in enumerate((0x208, 0x20C, 0x210)):
        obj = HEAP + 0x4000 + 0x100 * k
        emu.write32(unit + off, obj)
        emu.write32(obj, 0xC0000000 + k)
        emu.write32(obj + 4, 10 + k)
        emu.write32(obj + 0x40, 5 + k)
    emu.write32(unit + 0x214, 0x1234)  # not a heap object: skipped
    mods = fxc.module_objects(emu, unit)
    assert [(m[0], m[2], m[3]) for m in mods] == [
        (0x208, 0xC0000000, 5),
        (0x20C, 0xC0000001, 6),
        (0x210, 0xC0000002, 7),
    ]
    assert fxc.module_rows(emu, unit) == {0x208: 10, 0x20C: 11, 0x210: 12}


def test_stores_of_filters_by_index() -> None:
    units = [fxc.Unit(0, 0x10, 0, "synth"), fxc.Unit(3, 0x20, 0, "organ")]
    assert [s.address for s in fxc.stores_of(units)] == [0x10, 0x20]
    assert fxc.stores_of(units, {3}) == [cap.Store(3, "organ", 0x20)]


def test_value_grid_small_range_is_complete() -> None:
    assert fxc.value_grid(0, 11) == list(range(12))
    assert fxc.value_grid(0, 1) == [0, 1]


def test_value_grid_large_range_has_ends_and_is_sorted() -> None:
    g = fxc.value_grid(0, 127)
    assert g == sorted(set(g)) and g[0] == 0 and g[-1] == 127 and 1 in g and 63 in g
    s = fxc.value_grid(-60, 60)
    assert {-60, -1, 0, 1, 60} <= set(s) and s == sorted(s)
    w = fxc.value_grid(0, 16383)
    assert w[0] == 0 and w[-1] == 16383 and len(w) < 20


def test_apply_series_tags_messages_with_value_and_stage() -> None:
    m1, m2 = ae.Message(2, 1, 0x28000000, 5), ae.Message(2, 1, 0x28000004, 6)

    def fake(v: int):
        return ([m1] if v == 1 else []), ([m2] if v == 2 else []), ""

    got = fxc.apply_series(fake, [0, 1, 2])
    assert got == [(1, "call", m1), (2, "tick", m2)]


def test_rows_drop_noise_and_format_words() -> None:
    unit = fxc.Unit(3, 0x100, 0, "organ")
    slot = fxc.SLOTS[0]
    keep = ae.Message(2, 1, 0x28000000 + 4 * 0xE1000, 0x3F800000, 0xC0000000)
    noisy = ae.Message(2, 1, 0x28000000 + 4 * 0xE2000, 1)
    key = (noisy.chip, noisy.core, noisy.path, noisy.dm_word)
    rows = fxc._rows(unit, slot, 4, "id", "param", [(7, "call", keep), (7, "call", noisy)], {key})
    assert len(rows) == 1
    row = rows[0]
    assert (row["unit"], row["kind"], row["slot"], row["type"]) == (3, "organ", "Fx1", 4)
    assert (row["value"], row["when"], row["core"]) == (7, "call", 1)
    assert (row["dm_word"], row["word"], row["pc"]) == ("0xe1000", "0x3f800000", "0xc0000000")


def test_write_capture_roundtrip(tmp_path) -> None:
    rows = [dict.fromkeys(fxc.CAPTURE_FIELDS, 1)]
    out = tmp_path / "c.csv"
    fxc.write_capture(out, rows)
    lines = out.read_text(encoding="utf-8").splitlines()
    assert lines[0] == ",".join(fxc.CAPTURE_FIELDS)
    assert lines[1] == ",".join(["1"] * len(fxc.CAPTURE_FIELDS))


def test_slots_name_distinct_ids() -> None:
    names = [s.name for s in fxc.SLOTS]
    assert len(names) == len(set(names))
    ids = [i for s in fxc.SLOTS for i in s.params]
    assert len(ids) == len(set(ids))
