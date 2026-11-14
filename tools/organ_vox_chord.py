#!/usr/bin/env python3
"""Oracle replay script of an arbitrary Vox chord: model install + the key-down weight words.

The install burst (`Edt_Org_kModel = 1`, pass 0 of `docs/research/data/keydown-vox-c4e4g4.script`)
is reused; the key-down messages are rebuilt from the verified key map
(`docs/research/keydown-zones.md` section 5): a key `n` flags the rows
`fold(n - 36 + I[slot])` of the footage slots with a non-zero level and sends
`DM(0x95E46 + row) = float(sum of the levels of the flagged slots)`.

    python tools/organ_vox_chord.py 45,52,57,61 > chord.script
    python tools/pno_full_pass.py run SRAM/core1_sram.bin --exe target/release/sharc-pass \
        --script chord.script --passes 12000 --out chord
    cargo run --release -p nord-tools --bin organ_ref -- --mode vox --rate 43402.34375 \
        --frames 12000 --notes 45,52,57,61 --out rust_chord
    python tools/organ_partials.py vox chord.f32.npy rust_chord.csv --keys 45,52,57,61 --skip 3000
"""

from __future__ import annotations

import argparse
import struct
import sys
from pathlib import Path

DATA = Path(__file__).resolve().parents[1] / "docs" / "research" / "data"
INSTALL = DATA / "keydown-vox-c4e4g4.script"
SLOT_OFFSETS = (0, 7, 12, 19, 19, 24, 24, 28, 31, 31, 36, 36, 36, 40, 40, 43, 48, 48, 52, 60)
# VERIFIED (keydown-zones.md section 5): default program, 16', 8', 4', 2' at 2076180 / 2^23.
DEFAULT_LEVELS = {0: 2076180, 2: 2076180, 5: 2076180, 10: 2076180}
WEIGHTS0, ROWS = 0x95E46, 96


def fold(row: int) -> int:
    while row < 0:
        row += 12
    while row >= ROWS:
        row -= 12
    return row


def weights(keys: list[int], levels: dict[int, int] | None = None) -> dict[int, float]:
    """Row to weight of the held `keys` (levels are 23 bit fixed point per slot)."""
    levels = DEFAULT_LEVELS if levels is None else levels
    rows: dict[int, float] = {}
    for key in keys:
        for slot, level in levels.items():
            r = fold(key - 36 + SLOT_OFFSETS[slot])
            rows[r] = rows.get(r, 0.0) + level / 2**23
    return rows


def f32_bits(x: float) -> int:
    return struct.unpack("<I", struct.pack("<f", x))[0]


def script(keys: list[int]) -> str:
    """Install burst (pass 0 of the capture) followed by one weight write per row at pass 0."""
    lines = [ln for ln in INSTALL.read_text().splitlines() if ln.split()[0] == "0"]
    for row, w in sorted(weights(keys).items()):
        lines.append(f"0 {WEIGHTS0 + row:#x} {f32_bits(w):#010x}  # key-down rows")
    return "\n".join(lines) + "\n"


# VERIFIED (keydown-organ-keys.csv, model 1, 24 notes): ticks after key-down at which the slots
# 0, 2, 5, 10 are sent; ASSUMED grouping for the other slots (vox_timeline.rs).
ON_TICKS = {0: 3, 1: 3, 2: 17, 3: 17, 4: 17, 5: 17}
FRAME_RATE, TICK_SECONDS = 43402.34375, 0.001


def on_tick(slot: int) -> int:
    return ON_TICKS.get(slot, 47)


def timeline_script(keys: list[int]) -> str:
    """Install burst plus the key-down weights at the real tick of each footage slot.

    One pass is two frames; a tick (ASSUMED 1 ms) is `TICK_SECONDS * FRAME_RATE / 2` passes.
    """
    lines = [ln for ln in INSTALL.read_text().splitlines() if ln.split()[0] == "0"]
    ticks = sorted({on_tick(s) for s in DEFAULT_LEVELS})
    prev: dict[int, float] = {}
    for tick in ticks:
        levels = {s: v for s, v in DEFAULT_LEVELS.items() if on_tick(s) <= tick}
        now = weights(keys, levels)
        pas = round(tick * TICK_SECONDS * FRAME_RATE / 2)
        for row, w in sorted(now.items()):
            if prev.get(row) != w:
                lines.append(f"{pas} {WEIGHTS0 + row:#x} {f32_bits(w):#010x}  # tick {tick}")
        prev = now
    return "\n".join(lines) + "\n"


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("keys", help="comma separated MIDI keys")
    ap.add_argument("--timeline", action="store_true", help="staggered key-down ticks")
    ns = ap.parse_args(argv)
    keys = [int(k) for k in ns.keys.split(",")]
    sys.stdout.write(timeline_script(keys) if ns.timeline else script(keys))
    return 0


if __name__ == "__main__":
    sys.exit(main())
