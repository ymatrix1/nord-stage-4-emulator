#!/usr/bin/env python3
"""Firmware evidence for the arpeggiator pattern gate lengths and for preset dial value 0.

**Gate lengths.** `GlueArpPtnPreset` (`docs/research/synth-arp-patterns.md`) copies the pattern
words (Dur `0x118`, Pan `0x119`, Acc `0x117`) into the per-layer arp object (table `0xC054CBD0`,
`[obj+0x18/0x1C/0x20]`) and calls the *pattern compiler* at `0xC02424DC`. The compiler turns the
words into one 16-byte record per **slot**, two slots per step (`[obj+0x24] = steps * 2`, set at
`0xC011C520`; the step code of slot `s` is `(Dur >> (s & ~1)) & 3`):

    record = {type, tick, pan, acc}     tick: arp note index (the arpeggio advances once per tick)
    type 0 / 1  note on (1 = on without a preceding note-off, legato)
    type 2      hold (nothing happens)
    type 3      note off

so `E` is on at the first slot and off at the second (**1 slot = half a step**), `q` is on and held
for two slots (**one full step**), `Q` is on and held into the next step, which does not trigger
(**tie**: same tick) and ends with its own gate. This tool runs the real compiler on the ARM
emulator (`tools/arm_emu.py`, `unicorn`) and checks that the simple per-step rule of the engine
(`E` 4, `q` 8, `Q` 8 + tie, `.` 0 eighths of a step) reproduces every note the firmware compiles for
the 28 presets.

**Dial value 0.** The listener `0xC0102F30` and the update method `0xC010FDAC` of the same Glue
object give value 0 a meaning: it is not a table entry but "the saved custom pattern" (restored
into the pattern words), and a gate (`0x118`) edit while a preset is selected snapshots the words
and sets the dial back to 0 (the "preset edited" marker). `dial` replays both routines.

    uv run --with unicorn python tools/ns4_arp_gate.py gates CODE docs/research/arp-preset-table.csv
    uv run --with unicorn python tools/ns4_arp_gate.py dial CODE
    uv run --with unicorn python tools/ns4_arp_gate.py clock CODE

with `CODE` = `target/fw_unpacked/bank0_CODE.bin`.

**Clock (fourth pass).** `clock` replays the Glue rate update `0xC011A51C` and checks the model of
the free-running arp clock (`0xC01E6368`): a phase of `2**19` units per firmware beat at the
`RATE_TABLE` BPM, in a time base of 24000 units per second. See section 8 of the research note for
what this proves about the step and slot duration and what it does not (the record player itself
was not found).

Evidence and limits: `docs/research/synth-arp-patterns.md` section 7.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

import ns4_arp_presets as ap

COMPILE = 0xC02424DC
LISTENER = 0xC0102F30
UPDATE = 0xC010FDAC
OBJ = 0x80040000  # fake arp / glue object
RECORDS = 0x80060000
SLOT_EIGHTHS = 4  # one slot = half a step = 4 eighths of a step
UPDATE_RATE = 0xC011A51C  # Glue rate update: dial value -> clock rate `[clock+0x38]` (double)
RATE_TABLE = 0xC02BBF98 + 0x458  # 128 ints: the rate dial's firmware BPM per raw value 0..127
TIME_BASE_HZ = 24000  # `0x80001DC0` counter: timeouts 2400 / 600 / 4800 = 100 / 25 / 200 ms
PHASE_PER_BEAT = 1 << 19  # `0x41200000` constant of the rate formula (524288.0)
FREE_RUN_GRID = 0xC02BBF98  # step grid of the free-running arp: count 8, entry k = k * 2**18
GRID_ENTRY_PHASE = 1 << 18  # grid spacing = half a firmware beat
SLOTS_PER_STEP = 2
NOTE_ON = (0, 1)
NOTE_OFF = 3
# Engine rule: eighths of a step the note of a step lasts by itself, and whether it ties on.
STEP_RULE = {"E": (4, False), "q": (8, False), "Q": (8, True), ".": (0, False)}


def modelled_notes(gate: str) -> list[tuple[int, int]]:
    """`(start slot, eighths)` of every note the engine rule plays for a gate string.

    A note starts at every played step that is not the continuation of a `Q`; a `Q` ties into the
    next step (not across the pattern end) and the chain ends with the gate of the last step.
    """
    notes = []
    step = 0
    while step < len(gate):
        eighths, tie = STEP_RULE[gate[step]]
        if eighths == 0:
            step += 1
            continue
        start = step
        total = eighths
        while tie and step + 1 < len(gate):
            step += 1
            eighths, tie = STEP_RULE[gate[step]]
            total += eighths
        notes.append((start * SLOTS_PER_STEP, total))
        step += 1
    return notes


def notes_from_records(types: list[int]) -> list[tuple[int, int]]:
    """`(start slot, eighths)` of the notes in a compiled slot record list (type per slot).

    A note ends at the first note-off, at the next note-on, or at the end of the pattern.
    """
    notes = []
    start = None
    for slot, kind in enumerate(types):
        if kind == NOTE_OFF or (kind in NOTE_ON and start is not None):
            if start is not None:
                notes.append((start, (slot - start) * SLOT_EIGHTHS))
            start = None
        if kind in NOTE_ON:
            start = slot
    if start is not None:
        notes.append((start, (len(types) - start) * SLOT_EIGHTHS))
    return notes


def compile_slots(emu, gate: str, pan: str | None = None) -> list[tuple[int, int, int, int]]:
    """Runs the firmware pattern compiler; returns `(type, tick, pan, acc)` per slot."""
    from arm_emu import SENTINEL, STACK_TOP

    pan = pan or "C" * len(gate)
    slots = SLOTS_PER_STEP * len(gate)
    emu.write32(OBJ + 0x18, ap.word_from(gate, ap.GATE_CHARS))
    emu.write32(OBJ + 0x1C, ap.word_from(pan, ap.PAN_CHARS))
    emu.write32(OBJ + 0x20, 0)
    emu.write32(OBJ + 0x24, slots)
    emu.write32(OBJ + 0x2C, slots)
    for k in range(slots + 1):
        emu.write32(OBJ + 0x30 + 4 * k, RECORDS + 16 * k)
    emu.set_reg(0, OBJ)
    emu.set_reg(13, STACK_TOP - 0x1000)
    emu.set_reg(14, SENTINEL)
    emu.run(COMPILE, SENTINEL, 100_000)
    return [
        tuple(emu.read32(RECORDS + 16 * k + 4 * j) for j in range(4))  # type: ignore[misc]
        for k in range(slots)
    ]


def check_gates(emu, rows: list[dict]) -> list[str]:
    """Differences between the engine rule and the firmware compiler (empty when they agree)."""
    problems = []
    for r in rows:
        recs = compile_slots(emu, r["gate"], r["pan"])
        got = notes_from_records([rec[0] for rec in recs])
        want = modelled_notes(r["gate"])
        if got != want:
            problems.append(f"preset {r['preset']} {r['gate']}: firmware {got} rule {want}")
    return problems


def listener(emu, event_id: int, dial: int, saved_dial: int) -> tuple[int, int, int]:
    """Runs the change listener `0xC0102F30`; returns `(snapshot flag, mode, saved dial)`.

    `mode` is the Glue `[obj+0xEC]`: 0 nothing, 1 restore the saved custom words, 2 apply preset.
    """
    from arm_emu import SENTINEL, STACK_TOP

    p, q, r, s = OBJ + 0x1000, OBJ + 0x1100, OBJ + 0x2000, OBJ + 0x2100
    event, ident = OBJ + 0x3000, OBJ + 0x3100
    emu.write32(OBJ + 0x34, p)  # [[[[obj+0x34]] + 0x450]] -> dial value (param 0x114)
    emu.write32(p, q)
    emu.write32(q + 0x450, r)
    emu.write32(r, s)
    emu.write32(s, dial)
    emu.write32(OBJ + 0xF0, saved_dial)
    emu.uc.mem_write(OBJ + 0xE8, b"\0")
    emu.write32(OBJ + 0xEC, 0)
    emu.write32(event + 4, 2)
    emu.write32(ident, event_id)
    emu.set_reg(0, OBJ)
    emu.set_reg(1, event)
    emu.set_reg(2, ident)
    emu.set_reg(13, STACK_TOP - 0x1000)
    emu.set_reg(14, SENTINEL)
    emu.run(LISTENER, SENTINEL, 10_000)
    return emu.uc.mem_read(OBJ + 0xE8, 1)[0], emu.read32(OBJ + 0xEC), emu.read32(OBJ + 0xF0)


def restore(emu, saved: tuple[int, int, int, int]) -> list[tuple[int, int]]:
    """Runs the update method `0xC010FDAC` in restore mode; returns the `(id, value)` writes.

    `saved` = the cached Dur, Pan, Acc and Length words (`[obj+0xF4..0x100]`).
    """
    from arm_emu import SENTINEL, STACK_TOP

    writes: list[tuple[int, int]] = []
    emu.add_stub(0xC00D1948, lambda e: writes.append((e.read32(e.reg(1)), e.reg(2))) or 0)
    for stubbed in (0xC00B1DA0, 0xC00D10FC):
        emu.add_stub(stubbed, lambda _e: 0)
    emu.write32(OBJ + 0xEC, 1)
    emu.uc.mem_write(OBJ + 0xE8, b"\0")
    for offset, value in zip((0xF4, 0xF8, 0xFC, 0x100), saved, strict=True):
        emu.write32(OBJ + offset, value)
    emu.write32(OBJ + 0xE4, 1)
    emu.write32(OBJ + 0xD0, OBJ + 0x2000)
    emu.set_reg(0, OBJ)
    emu.set_reg(13, STACK_TOP - 0x1000)
    emu.set_reg(14, SENTINEL)
    emu.run(UPDATE, SENTINEL, 200_000)
    return writes


def rate_per_unit(table_bpm: int) -> float:
    """Clock phase advance per time-base unit for a rate-table entry (the firmware formula).

    `i2d(entry) / 60.0 * 524288.0 / 24000.0` (the three constants of `0xC011A51C`).
    """
    return table_bpm / 60 * PHASE_PER_BEAT / TIME_BASE_HZ


def display_bpm(table_bpm: int) -> float:
    """Quarter-note BPM the panel shows: the table holds twice that (ns4decode's list: raw 0 is
    "16 bpm" and the table starts at 32; the duplicate entries sit on its "N.A." raw values)."""
    return table_bpm / 2


def grid_seconds(table_bpm: int) -> float:
    """Seconds between two points of the free-running grid (`2**18` phase units)."""
    return GRID_ENTRY_PHASE / (rate_per_unit(table_bpm) * TIME_BASE_HZ)


def read_rate_table(code: bytes, count: int = 128) -> list[int]:
    """The 128 firmware rate entries from the code image (`0xC0000000` load base)."""
    base = RATE_TABLE - 0xC0000000
    return [int.from_bytes(code[base + 4 * i : base + 4 * i + 4], "little") for i in range(count)]


def read_grid(code: bytes, address: int = FREE_RUN_GRID) -> list[int]:
    """Grid phases `{count; 0; entries as 64-bit}` at `address` (`0xC02BBF98`: 0, 2**18, ...)."""
    base = address - 0xC0000000
    count = int.from_bytes(code[base : base + 4], "little")
    return [
        int.from_bytes(code[base + 8 + 8 * k : base + 16 + 8 * k], "little") for k in range(count)
    ]


def clock_rate(emu, dial: int) -> float:
    """Runs the Glue rate update for `dial`; returns the free-running clock rate `[clock+0x38]`."""
    import struct

    from arm_emu import SENTINEL, STACK_TOP

    glue, arp, clock = OBJ, OBJ + 0x10000, OBJ + 0x18000
    emu.write32(0x80001DC0, 100000)  # time base low/high words
    emu.write32(0x80001DC4, 0)
    emu.write32(glue + 0x64, 0)  # layer 0
    emu.write32(glue + 0xCC, 0)  # not Master Clock synced
    emu.write32(glue + 0xC8, dial << 16)  # dial value, 16.16
    emu.write32(0xC054CBD0 + 4, arp)  # arp pattern object of layer 0
    emu.write32(0xC054CC10 + 4, clock)  # clock object of layer 0
    emu.write32(arp + 0xC, 0)  # not triplet
    emu.set_reg(0, glue)
    emu.set_reg(13, STACK_TOP - 0x1000)
    emu.set_reg(14, SENTINEL)
    emu.run(UPDATE_RATE, SENTINEL, 200_000)
    lo, hi = emu.read32(clock + 0x38), emu.read32(clock + 0x3C)
    return struct.unpack("<d", struct.pack("<II", lo, hi))[0]


def check_clock(emu, table: list[int]) -> list[str]:
    """Differences between the formula and the replayed firmware update (empty when equal)."""
    problems = []
    for dial, entry in enumerate(table):
        got, want = clock_rate(emu, dial), rate_per_unit(entry)
        if abs(got - want) > 1e-9 * max(1.0, want):
            problems.append(f"dial {dial}: firmware {got!r} formula {want!r}")
    return problems


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - needs the firmware image
    import arm_emu

    ap_ = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawTextHelpFormatter
    )
    sub = ap_.add_subparsers(dest="cmd", required=True)
    g = sub.add_parser("gates")
    g.add_argument("code", type=Path)
    g.add_argument("csv", type=Path)
    d = sub.add_parser("dial")
    d.add_argument("code", type=Path)
    c = sub.add_parser("clock")
    c.add_argument("code", type=Path)
    ns = ap_.parse_args(argv)
    emu = arm_emu.load_firmware(ns.code)
    if ns.cmd == "clock":
        table = read_rate_table(ns.code.read_bytes())
        problems = check_clock(emu, table)
        print("\n".join(problems) or f"all {len(table)} dial values: formula == firmware update")
        grid = read_grid(ns.code.read_bytes())
        spacing = sorted({b - a for a, b in zip(grid, grid[1:], strict=False)})
        print(f"free-run grid: {len(grid)} points, spacing {spacing}")
        for dial in (0, len(table) - 1):
            print(
                f"dial {dial}: {display_bpm(table[dial])} bpm, "
                f"grid point every {grid_seconds(table[dial]):.5f} s"
            )
        return 1 if problems else 0
    if ns.cmd == "gates":
        rows = ap.read_csv(ns.csv)
        problems = check_gates(emu, rows)
        print("\n".join(problems) or f"all {len(rows)} presets: rule == firmware compiler")
        return 1 if problems else 0
    cases = {
        "dial 0 -> 5": (0x114, 5, 0),
        "dial 5 -> 7": (0x114, 7, 5),
        "dial 5 -> 0": (0x114, 0, 5),
        "gate edit on preset 5": (0x118, 5, 5),
        "gate edit on dial 0": (0x118, 0, 0),
    }
    for name, args in cases.items():
        print(f"{name}: (snapshot, mode, saved dial) = {listener(emu, *args)}")
    print("restore writes (id, value):", [(hex(i), hex(v)) for i, v in restore(emu, (1, 2, 3, 9))])
    return 0


if __name__ == "__main__":
    sys.exit(main())
