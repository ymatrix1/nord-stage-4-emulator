"""Unit tests for tools/arm_slot_owners.py (pure helpers; no emulator)."""

from __future__ import annotations

import arm_slot_owners as so

IDLE = so.IDLE_ROUTINE


def w(chain: int, slot: int, routine: int, state: int = 1, event: int = 0) -> so.SlotWrite:
    return so.SlotWrite(chain, state, event, 6, 0, slot, routine)


BOOT = [
    w(0xA, 0, 0x122A71, event=2),
    w(0xB, 1, 0x121BBE),
    w(0xC, 2, IDLE),
    w(0xC, 3, IDLE),
    w(0xB, 4, 0x121E2B, state=2),
    w(0xA, 5, 0x122B85, state=2, event=2),
]


def test_final_table_keeps_the_last_write_per_slot() -> None:
    extra = [*BOOT, w(0xB, 2, 0x12272D), w(0xB, 2, 0x122620)]
    assert so.final_table(extra)[2] == 0x122620
    assert so.final_table(BOOT)[1] == 0x121BBE


def test_owners_ignore_idle_writes() -> None:
    own = so.owners(BOOT)
    assert own[1] == 0xB and own[0] == 0xA
    assert 2 not in own and 3 not in own


def test_shared_generators_are_the_non_idle_slots_below_four() -> None:
    table = so.final_table(BOOT)
    assert so.shared_generators(table) == {0: 0x122A71, 1: 0x121BBE}


def test_pairs_follow_the_stream_of_slot_plus_four() -> None:
    table = so.final_table(BOOT)
    got = so.pairs(table)
    assert (1, 0x121BBE, 5, 0x122B85) in got
    assert (0, 0x122A71, 4, 0x121E2B) in got
    assert all(p == g + 4 for g, _, p, _ in got)


def test_census_counts_chip_core_path() -> None:
    c = so.census([("PNO", 1, 2), ("PNO", 1, 2), ("SMP", 0, 1)])
    assert c[("PNO", 1, 2)] == 2 and c[("SMP", 0, 1)] == 1


def test_csv_has_one_row_per_write_and_hex_routines() -> None:
    text = so.writes_csv({"boot": BOOT})
    rows = text.strip().splitlines()
    assert rows[0] == "run,chain,state,event,kind,chan,slot,routine"
    assert len(rows) == 1 + len(BOOT)
    assert rows[1].endswith("0x122a71")


def test_table_text_orders_by_slot() -> None:
    assert so.table_text({4: 0x121E2B, 1: 0x121BBE}) == "1:0x121bbe 4:0x121e2b"


def test_idle_flag() -> None:
    assert w(1, 2, IDLE).idle and not w(1, 2, 0x122620).idle
