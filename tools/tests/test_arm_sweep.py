"""Unit tests for tools/arm_sweep.py with hand-assembled ARM snippets (no firmware needed)."""

from __future__ import annotations

import faulthandler
import gc
import struct

import arm_emu as ae
import arm_sweep as sw
import pytest

pytest.importorskip("unicorn")
faulthandler.disable()

BX_LR = 0xE12FFF1E
BIG = 0x300000


def words(*ws: int) -> bytes:
    return b"".join(struct.pack("<I", w & 0xFFFFFFFF) for w in ws)


@pytest.fixture(autouse=True)
def _release_emulators():
    yield
    gc.collect()


def emu_with_setter(code: bytes) -> ae.ArmEmu:
    buf = bytearray(BIG)
    off = sw.STORE_SETTER - ae.LOAD_BASE
    buf[off : off + len(code)] = code
    return ae.ArmEmu(bytes(buf), ram_size=BIG)


# setter(store, 0, &{id, 1}, value): one path 2 message to comm 1: (0x28000100, pad 0, r3)
SETTER = words(
    0xE59FC014,  # ldr ip, [pc, #0x14]  (port)
    0xE59F0014,  # ldr r0, [pc, #0x14]  (address)
    0xE58C0000,  # str r0, [ip]
    0xE3A00000,  # mov r0, #0
    0xE58C0000,  # str r0, [ip]
    0xE58C3000,  # str r3, [ip]
    BX_LR,
    ae.PORT2_BASE + 0x10,
    0x28000100,
)


def test_load_ids_filters_by_name(tmp_path) -> None:
    csv = tmp_path / "p.csv"
    csv.write_text(
        "index,id,domain,name,min,max,default,w\n"
        "0,0x1,0x1,Fnc_Volume,0,127,127,1\n"
        "1,0x16d,0x1,Edt_Org_Preset_kDb1,0,8,8,1\n"
        "2,0x14d,0x1,Edt_Pno_kType,0,3,0,1\n",
        encoding="utf-8",
    )
    assert sw.load_ids(csv) == [(0x16D, "Edt_Org_Preset_kDb1"), (0x14D, "Edt_Pno_kType")]
    assert sw.load_ids(csv, "Volume") == [(1, "Fnc_Volume")]


def test_sweep_records_the_message_and_passes_id_and_value() -> None:
    emu = emu_with_setter(SETTER)
    scratch = ae.SRAM_BASE + 0x200
    rows = sw.sweep(emu, 1, scratch, [(0x16D, "Edt_Org_Preset_kDb1")], (3, 4))
    assert [r.value for r in rows] == [3, 4]
    assert [(m.comm, m.core, m.dm_word, m.word2) for m in rows[0].messages] == [(1, 1, 0x40, 3)]
    assert rows[1].messages[0].word2 == 4
    assert emu.read32(scratch) == 0x16D
    assert emu.read32(scratch + 4) == 1


def test_sweep_keeps_silent_ids_and_reports_faults() -> None:
    emu = emu_with_setter(words(BX_LR))
    rows = sw.sweep(emu, 1, ae.SRAM_BASE + 0x200, [(5, "Edt_Org_x")], (1,))
    assert rows[0].messages == []
    assert rows[0].error == ""
    emu = emu_with_setter(words(0xE3E00003, 0xE5900000, BX_LR))  # ldr from 0xFFFFFFFC
    rows = sw.sweep(emu, 1, ae.SRAM_BASE + 0x200, [(5, "Edt_Org_x")], (1,))
    assert rows[0].error


def test_rows_csv_has_one_line_per_message_and_one_per_silent_set() -> None:
    emu = emu_with_setter(SETTER)
    rows = sw.sweep(emu, 1, ae.SRAM_BASE + 0x200, [(0x16D, "Edt_Org_Preset_kDb1")], (3,))
    rows.append(sw.SweepRow(0x16C, "Edt_Org_kVib", 1))
    lines = sw.rows_csv(rows).splitlines()
    assert lines[0].startswith("id,name,value,chip")
    assert lines[1].startswith("0x16d,Edt_Org_Preset_kDb1,3,PNO,1,1,2,0x40,0x00000003,")
    assert lines[2].startswith("0x16c,Edt_Org_kVib,1,,")
    assert len(lines) == 3
