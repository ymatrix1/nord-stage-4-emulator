#!/usr/bin/env python3
"""Static map of the ARM side of the piano path (host firmware -> SMP DSP, comm 4).

The piano layer of the host firmware (TI AM1705, `bank0_CODE.bin` loads at `0xC0000000`) drives the
SMP DSP through path 1 messages `(DM word address, value)`. This tool answers, without running the
firmware:

    python tools/arm_piano_path.py fields target/fw_unpacked/bank0_CODE.bin   # literal -> field map
    python tools/arm_piano_path.py pitch  target/fw_unpacked/bank0_CODE.bin   # cents table check

* `fields`: every 32-bit literal of the ARM image that is a DM address inside the SMP voice-record
  array (`REC_BASE + REC_STRIDE * k + field`, field `-32..21`) or the voice list, with the
  enclosing function, so the writers of each record field can be read from the disassembly.
* `pitch`: the 1200-entry cents table that the step writer `0xC00B7B68` indexes
  (`PITCH_TABLE`, entry `n` = `2^24 * 2^(-n/1200)`), its maximum error, and `step_frac`, a model
  of the value that writer sends for a given pitch offset in cents.

Also here: `fixed_key_note` (the `Edt_Sys_kPianoFixedKey_DEV` mapping), `piano_stores` (the piano
layer stores among those `arm_edit_capture.find_stores` returns) and `stack_return_candidates`
(a stack-scan backtrace used to attribute a captured DSP message to its caller chain).

Evidence and limits: `docs/research/piano-dsp-path.md`.
"""

from __future__ import annotations

import argparse
import bisect
import struct
import sys
from collections.abc import Callable, Iterable
from pathlib import Path

LOAD_BASE = 0xC0000000
# VERIFIED (disassembly of 0xC00B6CC0 and the writers 0xC00AF3D4, 0xC00B7B68: DM address =
# literal + 54 * k, `add r3, r3, r3, lsl #1; add r3, r3, r3, lsl #3; lsl #1` = 27 * 2 * k):
# the SMP voice records are 54 words apart from DM 0x92848, k = 0..113 (`cmp r5, #0x72`).
REC_BASE = 0x92848
REC_STRIDE = 54
REC_COUNT = 114
# VERIFIED (disassembly of 0xC00B6CC0): list entry `v` of group `g` is DM `0x92534 + 114 g + v`
# and holds the address of a record (`0x92848 + 54 k`).
LIST_BASE = 0x92534
LIST_STRIDE = 114
# VERIFIED (step writer 0xC00B7B68 writes DM 0x92829 + 54 k = REC_BASE - 31 + 54 k): the record
# pointer `I3` of the SMP voice loop (docs/research/smp-voice.md) is the record base, so field
# `f` of that note (offset from I3, -32..+21 for 54 words) is DM `REC_BASE + 54 k + f`.
FIELD_LO, FIELD_HI = -32, 21
SMP_COMM = 4
# VERIFIED (`pitch_table_error`): 1201 words `2^24 * 2^(-n/1200)` (n = 0..1200), then 0.
PITCH_TABLE = 0xC02A6168
PITCH_LEN = 1200
CODE_END = 0x279000
# Q24 ratio cap before the `<< 8` of the step writer (`mvn r1, #0xff000000`).
STEP_CAP = 0x00FFFFFF
# EMPIRICAL (disassembly of 0xC011A87C, the piano layer's handler of ids 0x42..0x45): the fixed
# key dev id maps 1..89 to the note range of the 88-key keybed, kept in the globals
# `0xC059FF5C + 0x208 / 0x210`.
FIXED_KEY_LO, FIXED_KEY_HI = 0x15, 0x6C


def words(code: bytes) -> list[int]:
    n = len(code) // 4
    return list(struct.unpack(f"<{n}I", code[: n * 4]))


def function_starts(ws: list[int]) -> list[int]:
    """Addresses of `push {..., lr}` instructions (ARM mode), a heuristic for function entries."""
    return [LOAD_BASE + 4 * i for i, w in enumerate(ws) if (w & 0xFFFF4000) == 0xE92D4000]


def enclosing(starts: list[int], address: int) -> int:
    k = bisect.bisect_right(starts, address) - 1
    return starts[k] if k >= 0 else 0


def field_of(address: int) -> int | None:
    """Record field (offset from `I3`) of a DM literal that denotes record 0, else None."""
    f = address - REC_BASE
    return f if FIELD_LO <= f <= FIELD_HI else None


def scan_literals(ws: list[int], lo: int, hi: int) -> dict[int, list[int]]:
    """Image addresses of every word with `lo <= word < hi` below `CODE_END`."""
    out: dict[int, list[int]] = {}
    for i, w in enumerate(ws):
        a = LOAD_BASE + 4 * i
        if lo <= w < hi and a < LOAD_BASE + CODE_END:
            out.setdefault(w, []).append(a)
    return out


def field_table(code: bytes) -> list[tuple[int, int, int]]:
    """`(field, literal image address, enclosing function)` for every record-field literal."""
    ws = words(code)
    starts = function_starts(ws)
    rows = []
    for lit, sites in scan_literals(ws, REC_BASE + FIELD_LO, REC_BASE + FIELD_HI + 1).items():
        rows += [(lit - REC_BASE, a, enclosing(starts, a)) for a in sites]
    return sorted(rows)


def pitch_table(code: bytes) -> list[int]:
    """The `PITCH_LEN + 2` words at `PITCH_TABLE`."""
    off = PITCH_TABLE - LOAD_BASE
    return list(struct.unpack_from(f"<{PITCH_LEN + 2}I", code, off))


def pitch_table_error(table: list[int]) -> float:
    """Largest `|table[n] - 2^24 * 2^(-n/1200)|` over `n = 0..1200` (Q24 units)."""
    return max(abs(table[n] - 2**24 * 2 ** (-n / PITCH_LEN)) for n in range(PITCH_LEN + 1))


def synthetic_pitch_table() -> list[int]:
    """The table as the rounding of its formula (for tests without firmware)."""
    return [round(2**24 * 2 ** (-n / PITCH_LEN)) for n in range(PITCH_LEN + 1)] + [0]


def step_frac(cents: int, table: list[int]) -> int:
    """`step_frac` word (record field -31) that `0xC00B7B68` sends for a pitch offset `cents <= 0`.

    EMPIRICAL (disassembly 0xC00B7B68..0xC00B7C7C): `n = -cents`; while `n > 1200` subtract 1200
    and count an octave; `ratio_q24 = table[n] >> octaves`; word = `min(ratio_q24, 0xFFFFFF) << 8`
    (a Q32 fraction of one decoded sample per output frame). Positive offsets index before the
    table: the firmware assumes the key map keeps the offset at or below zero.
    """
    if cents > 0:
        raise ValueError("the writer indexes the table with -cents: cents must be <= 0")
    n, octaves = -cents, 0
    while n > PITCH_LEN:
        n -= PITCH_LEN
        octaves += 1
    return (min(table[n] >> octaves, STEP_CAP) << 8) & 0xFFFFFFFF


def fixed_key_note(value: int) -> int:
    """MIDI note played by `Edt_Sys_kPianoFixedKey_DEV = value` (0 disables the override)."""
    if value <= 0:
        raise ValueError("0 means no override")
    return min(FIXED_KEY_LO - 1 + value, FIXED_KEY_HI)


def piano_stores(stores: Iterable) -> list:
    """The stores of the piano layers (`kind == "piano"`), one per `(layer, address)`."""
    return [s for s in stores if getattr(s, "kind", "") == "piano"]


def is_return_address(code: bytes, address: int) -> bool:
    """True when `address` follows a `bl` / `blx reg` in the ARM image (a plausible `lr`)."""
    if not (LOAD_BASE + 4 <= address < LOAD_BASE + CODE_END) or address & 3:
        return False
    (w,) = struct.unpack_from("<I", code, address - 4 - LOAD_BASE)
    return (w & 0x0F000000) == 0x0B000000 or (w & 0x0FFFFFF0) == 0x012FFF30


def stack_return_candidates(
    code: bytes, read32: Callable[[int], int], sp: int, depth: int = 0x400
) -> list[tuple[int, int]]:
    """`(sp offset, call site)` for every stack word that looks like a return address."""
    out = []
    for k in range(0, depth, 4):
        v = read32(sp + k)
        if is_return_address(code, v):
            out.append((k, v - 4))
    return out


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("what", choices=["fields", "pitch"])
    ap.add_argument("image", type=Path)
    ns = ap.parse_args(argv)
    code = ns.image.read_bytes()
    if ns.what == "fields":
        print("field,literal_at,function")
        for f, a, fn in field_table(code):
            print(f"{f},{a:#x},{fn:#x}")
        return 0
    table = pitch_table(code)
    print(f"entries {len(table)}, max |error| {pitch_table_error(table):.4f} (Q24 units)")
    print(f"table[1200] = {table[PITCH_LEN]:#x}, table[1201] = {table[PITCH_LEN + 1]:#x}")
    for cents in (0, -100, -600, -1200, -2400):
        print(f"cents {cents:6d}: step_frac {step_frac(cents, table):#010x}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
