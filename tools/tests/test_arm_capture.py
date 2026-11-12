"""Unit tests for tools/arm_capture.py using stand-in senders assembled at the real addresses."""

from __future__ import annotations

import faulthandler
import gc
import json
import struct

import arm_capture as ac
import arm_emu as ae
import pytest

pytest.importorskip("unicorn")
# Unicorn probes guard pages through a Windows structured exception that faulthandler would print.
faulthandler.disable()

BX_LR = 0xE12FFF1E


def words(*ws: int) -> bytes:
    return b"".join(struct.pack("<I", w & 0xFFFFFFFF) for w in ws)


# Stand-in for the strip setter (r0 = this, r1 = id, r2 = &value): sends
# (0x28800000 + 4 * id, 0, [r2]) to the ORG poke port.
FAKE_STRIP_SETTER = words(
    0xE5923000,  # ldr r3, [r2]
    0xE1A01101,  # mov r1, r1, lsl #2
    0xE59FC018,  # ldr ip, [pc, #24]
    0xE081100C,  # add r1, r1, ip
    0xE59F2014,  # ldr r2, [pc, #20]
    0xE5821000,  # str r1, [r2]
    0xE3A0C000,  # mov ip, #0
    0xE582C000,  # str ip, [r2]
    0xE5823000,  # str r3, [r2]
    BX_LR,
    0x28800000,
    ae.PORT2_BASE + 0x20,
)

# Stand-in for a controller sender (r0 = {[0] index, [8] state}): sends
# (0x28800000 + 4 * index, 0, state) to the ORG poke port.
FAKE_CONTROLLER = words(
    0xE5901000,  # ldr r1, [r0]
    0xE5903008,  # ldr r3, [r0, #8]
    0xE1A01101,  # mov r1, r1, lsl #2
    0xE59FC018,  # ldr ip, [pc, #24]
    0xE081100C,  # add r1, r1, ip
    0xE59F2014,  # ldr r2, [pc, #20]
    0xE5821000,  # str r1, [r2]
    0xE3A0C000,  # mov ip, #0
    0xE582C000,  # str ip, [r2]
    0xE5823000,  # str r3, [r2]
    BX_LR,
    0x28800000,
    ae.PORT2_BASE + 0x20,
)


@pytest.fixture(autouse=True)
def _release_emulators():
    """Unicorn instances sit in reference cycles; free them so many tests fit in memory."""
    yield
    gc.collect()


def fake_emu() -> ae.ArmEmu:
    size = max(ac.STRIP_SETTER, *ac.CONTROLLER_SENDERS) - ae.LOAD_BASE + 0x100
    buf = bytearray(size)
    off = ac.STRIP_SETTER - ae.LOAD_BASE
    buf[off : off + len(FAKE_STRIP_SETTER)] = FAKE_STRIP_SETTER
    for s in ac.CONTROLLER_SENDERS:
        off = s - ae.LOAD_BASE
        buf[off : off + len(FAKE_CONTROLLER)] = FAKE_CONTROLLER
    return ae.ArmEmu(bytes(buf), ram_size=(len(buf) + 0xFFF) & ~0xFFF)


def test_f32_decodes_ieee_single() -> None:
    assert ac.f32(0x3F800000) == 1.0
    assert ac.f32(0xC2000000) == -32.0


def test_strip_call_passes_id_and_value_pointer() -> None:
    emu = fake_emu()
    (m,) = ac.strip_call(emu, 5, 0x20000)
    assert m.word1 == 0x28800000 + 20
    assert m.word2 == 0x20000


def test_strip_call_pokes_strip_index_into_object() -> None:
    emu = fake_emu()
    ac.strip_call(emu, 1, 0, strip=3)
    assert emu.read32(ac.SCRATCH_OBJ + ac.STRIP_INDEX_OFFSET) == 3


def test_strip_sweep_one_record_per_id_and_value() -> None:
    emu = fake_emu()
    rows = ac.strip_sweep(emu, ids=range(3), values=(1, 2))
    assert len(rows) == 6
    assert rows[0]["scenario"] == "strip"
    assert rows[0]["input_q16"] == "0x00000001"
    assert rows[5]["messages"][0]["dm"] == "0x2"  # type: ignore[index]


def test_strip_table_relative_and_absolute_words() -> None:
    def rec(pid: int, dm: int, value: str) -> dict[str, object]:
        m = {"chip": "ORG", "core": 2, "dm": hex(dm), "w2": value}
        return {"id": pid, "input_q16": "0x00020000", "messages": [m]}

    table = ac.strip_table([rec(0xB, 0xE0020, "0xc2000000"), rec(0x1E, 0xC13A6, "0x0")])
    assert [t["id"] for t in table] == ["0xb", "0x1e"]
    assert table[0]["org2_dm_words"] == "32"
    assert table[1]["org2_dm_words"] == "abs0xc13a6"
    assert table[0]["in_0x20000"] == "0xc2000000"
    assert table[0]["in_0x0"] == ""
    assert table[0]["chips"] == "ORGc2"


def test_controller_call_builds_state_word() -> None:
    emu = fake_emu()
    (m,) = ac.controller_call(emu, ac.CONTROLLER_SENDERS[0], 4, value7=0x55, flags=0x20)
    assert m.dm_word == 4
    assert m.word2 == (0x55 << 8) | 0x20


def test_slot_value_is_seven_bit_and_distinct_per_slot() -> None:
    vals = [ac.slot_value(i) for i in range(ac.CONTROLLER_SLOTS)]
    assert all(0 <= v < 128 for v in vals)
    assert len(set(vals[:16])) == 16


def test_controller_sweep_covers_both_senders_and_all_slots() -> None:
    emu = fake_emu()
    rows = ac.controller_sweep(emu)
    assert len(rows) == 2 * ac.CONTROLLER_SLOTS
    assert rows[0]["sender"] == f"{ac.CONTROLLER_SENDERS[0]:#010x}"
    assert rows[1]["index"] == 1


def test_jsonl_roundtrip_and_filters(tmp_path) -> None:
    emu = fake_emu()
    rows = ac.strip_sweep(emu, ids=range(2), values=(0x10000, 0x20000))
    rows += ac.controller_sweep(emu)[:2]
    path = tmp_path / "cap.jsonl"
    ac.write_jsonl(rows, path)
    assert len(path.read_text().splitlines()) == 6
    assert len(ac.read_messages(path)) == 6
    assert len(ac.read_messages(path, scenario="strip")) == 4
    assert len(ac.read_messages(path, scenario="strip", input_q16="0x00020000")) == 2
    sender = f"{ac.CONTROLLER_SENDERS[0]:#x}"
    assert len(ac.read_messages(path, sender=sender)) == 2
    assert len(ac.read_messages(path, sender="0x1")) == 0


def test_read_messages_accepts_flat_message_lines(tmp_path) -> None:
    path = tmp_path / "flat.jsonl"
    m = ae.Message(2, 2, 0x28B07730, 7)
    path.write_text(m.to_json() + "\n" + json.dumps({"scenario": "x", "messages": []}) + "\n")
    assert ac.read_messages(path) == [m]


def test_flatten_preserves_order() -> None:
    emu = fake_emu()
    rows = ac.strip_sweep(emu, ids=range(2), values=(5,))
    assert [m.word1 for m in ac.flatten(rows)] == [0x28800000, 0x28800004]


def test_new_session_runs_init_and_guards(tmp_path) -> None:
    code = bytearray(0x400)
    code[0x2A4 : 0x2A4 + 4] = words(BX_LR)
    path = tmp_path / "code.bin"
    path.write_bytes(bytes(code))
    emu = ac.new_session(path, None)
    assert emu.read32(ae.PORT_OBJECT_PTR) == ae.FAKE_BASE
    emu.set_reg(14, 0)  # a null call must return instead of raising
    assert emu.null_calls == []
