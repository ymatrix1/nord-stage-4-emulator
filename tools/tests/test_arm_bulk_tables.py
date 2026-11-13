"""Unit tests for tools/arm_bulk_tables.py (pure helpers; the boot capture needs the firmware)."""

from __future__ import annotations

import json

import arm_bulk_tables as bt
import arm_emu as ae

PNO2 = ae.PORT2_BASE + 0x10  # path 2 port of comm 1 (PNO, pad byte 1)
SMP1 = ae.PORT1_BASE + 0x40  # path 1 port of comm 4 (SMP)


def pno_writes(dm: int, value: int, pc: int = 0x100) -> list[tuple[int, int, int]]:
    """The three stores of one PNO core 1 path 2 message."""
    return [(PNO2, 0x28000000 + 4 * dm, pc), (PNO2, 0, pc), (PNO2, value, pc)]


def test_region_of_names_every_window() -> None:
    code, sram = 0x1000, 0x100
    assert bt.region_of(0xC0000010, code, sram) == "bank0_CODE"
    assert bt.region_of(0xC0001000, code, sram) == "bss_runtime"
    assert bt.region_of(0x80000010, code, sram) == "bank0_SRAM"
    assert bt.region_of(0x80000200, code, sram) == "sram_runtime"
    assert bt.region_of(0x90001000, code, sram) == "heap_runtime"
    assert bt.region_of(0x01C41000, code, sram) == "device"
    assert bt.region_of(0x12345678, code, sram) == "other"


def test_fingerprint_depends_on_words_not_on_python_ints() -> None:
    assert bt.fingerprint([1, 2, 3]) == bt.fingerprint([1, 2, 3])
    assert bt.fingerprint([1, 2, 3]) != bt.fingerprint([1, 2, 4])
    assert len(bt.fingerprint([0])) == 16


def test_group_indexed_returns_last_store_index_and_drops_partial_groups() -> None:
    writes = pno_writes(0x10, 0xAA) + pno_writes(0x11, 0xBB)[:2]
    grouped = bt.group_indexed(writes)
    assert len(grouped) == 1
    message, index = grouped[0]
    assert (message.chip, message.core, message.dm_word, message.word2) == ("PNO", 1, 0x10, 0xAA)
    assert index == 2


def test_group_indexed_keeps_a_message_split_across_a_phase_boundary() -> None:
    writes = pno_writes(0x20, 7)
    phases = [("boot", 1), ("settle 1", len(writes))]
    msgs = bt.phase_messages(writes, phases)
    assert [(p.message.dm_word, p.phase) for p in msgs] == [(0x20, "settle 1")]


def test_group_indexed_path1_two_words() -> None:
    grouped = bt.group_indexed([(SMP1, 0x93936, 1), (SMP1, 0x3F800000, 2)])
    assert grouped[0][0].chip == "SMP"
    assert grouped[0][0].path == 1
    assert grouped[0][0].dm_word == 0x93936


def test_phase_of_clamps_to_the_last_phase() -> None:
    phases = [("init", 3), ("boot", 10)]
    assert bt.phase_of(0, phases) == "init"
    assert bt.phase_of(3, phases) == "boot"
    assert bt.phase_of(99, phases) == "boot"


def test_final_state_last_write_wins_and_fingerprint_is_stable() -> None:
    msgs = [pm.message for pm in bt.phase_messages(pno_writes(5, 1) + pno_writes(5, 2), [("b", 9)])]
    state = bt.final_state(msgs)
    assert state == {("PNO", 1, 2, 5): 2}
    count, sha = bt.state_fingerprint(state, "PNO", 1)
    assert count == 1
    assert sha == bt.fingerprint([5, 2])
    assert bt.state_fingerprint(state, "ORG", 1) == (0, "")


def test_summarise_groups_settle_windows_together() -> None:
    writes = pno_writes(1, 0) + pno_writes(2, 9) + pno_writes(3, 4)
    phases = [("boot", 3), ("settle 1", 6), ("settle 2", 9)]
    rows = bt.summarise(bt.phase_messages(writes, phases))
    by = {r["phase"]: r for r in rows}
    assert by["boot"]["messages"] == 1
    assert by["boot"]["zero_values"] == 1
    assert by["settle"]["messages"] == 2
    assert by["settle"]["distinct_words"] == 2


def test_bulk_and_transfer_rows() -> None:
    call = bt.BulkCall(0x95471, 0xC027C10C, 32, "bank0_CODE", "abc", 0xC00228F8)
    row = bt.bulk_rows([call])[0]
    assert row["dm_word"] == "0x95471"
    assert row["region"] == "bank0_CODE"
    t = bt.Transfer("spi_transfer", 0xC0001234, (1, 2, 3, 4), "settle 3", (0xC0440000, 8))
    rows = bt.transfer_rows([t, t, bt.Transfer("dsp_image_download", 0xC0241C58, (0,) * 4, "boot")])
    assert len(rows) == 2
    spi = next(r for r in rows if r["stub"] == "spi_transfer")
    assert (spi["caller_lr"], spi["phase"], spi["calls"]) == ("0xc0001234", "settle", 2)
    assert spi["first_args"] == ["0x1", "0x2", "0x3", "0x4"]
    assert spi["first_obj"] == ["0xc0440000", "0x8"]


def test_replay_script_selects_core_and_path() -> None:
    msgs = [pm.message for pm in bt.phase_messages(pno_writes(0x30, 0xDEAD), [("b", 3)])]
    assert bt.replay_script(msgs, "PNO", 1) == "0 0x30 0x0000dead\n"
    assert bt.replay_script(msgs, "PNO", 2) == ""
    assert bt.replay_script(msgs, "ORG", 1) == ""


def test_messages_jsonl_round_trips_through_arm_emu() -> None:
    pm = bt.phase_messages(pno_writes(0x44, 0x1234), [("boot", 3)])
    text = bt.messages_jsonl(pm)
    assert json.loads(text.splitlines()[0])["phase"] == "boot"
    back = ae.messages_from_jsonl(text)
    assert (back[0].dm_word, back[0].word2) == (0x44, 0x1234)


def l2_writes(byte_address: int, value: int) -> list[tuple[int, int, int]]:
    """One L2 block store: `(byte address, pad 0, value)` on the PNO path 2 port."""
    return [(PNO2, byte_address, 0x200), (PNO2, 0, 0x200), (PNO2, value, 0x200)]


def test_l2_stores_are_separate_from_dm_words() -> None:
    writes = l2_writes(0x200C0000, 0xFB50F8BD) + l2_writes(0x200C0004, 0xFDBE) + pno_writes(3, 1)
    msgs = [pm.message for pm in bt.phase_messages(writes, [("boot", 99)])]
    assert [bt.is_l2_word(m) for m in msgs] == [True, True, False]
    assert [bt.is_dsp_word(m) for m in msgs] == [False, False, True]
    assert bt.final_state(msgs) == {("PNO", 1, 2, 3): 1}
    count, sha = bt.l2_fingerprint(msgs, "PNO")
    assert count == 2
    assert sha == bt.fingerprint([0x200C0000, 0xFB50F8BD, 0x200C0004, 0xFDBE])
    assert bt.l2_fingerprint(msgs, "ORG") == (0, "")
    assert bt.replay_script(msgs, "PNO", 1) == "0 0x3 0x00000001\n"


def test_summarise_counts_l2_and_other_words() -> None:
    writes = l2_writes(0x200C0000, 5) + [(PNO2, 0x90000000, 1), (PNO2, 0, 1), (PNO2, 7, 1)]
    rows = bt.summarise(bt.phase_messages(writes, [("boot", 99)]))
    assert sum(r["messages"] for r in rows) == 2
    assert sum(r["l2_words"] for r in rows) == 1
    assert sum(r["other_words"] for r in rows) == 1
