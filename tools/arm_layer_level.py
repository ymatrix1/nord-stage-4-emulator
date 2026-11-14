#!/usr/bin/env python3
"""Run the layer level handlers of the firmware (organ `0xC011AB8C`, piano `0xC011A7A4`) live.

The handlers turn the q16 edit value `v << 16` of the layer level ids into the gain word that the
ARM stores for the DSP (`docs/research/organ-output-law.md` s3, `piano-note-velocity.md` s4:
listing law `(v / 127)^2`, mute sentinel -0.05). The booted-ARM capture never reached them (the
handler returns early without a per-layer controller), so this tool calls them directly with a
fabricated controller object and event, on the plain firmware image (the code is soft-float
arithmetic and needs no boot state), and reads back the stored word.

    uv run --with unicorn python tools/arm_layer_level.py target/fw_unpacked/bank0_CODE.bin

Fabricated fields (read from the listing, VERIFIED by the run): `[this + 0x64]` slot selector,
`[this + 0x8C]` layer index, `[this + 0x98]` table base, `[event + 0xC]` event kind, `r3` = pointer
to the q16 value, store at `table[7 * layer + slot_word + 0x34]` where `slot_word =
[0xC0 + 4 * [this + 0x64] + 0x658 literal]`, then the tail call `0xC023195C` (stubbed to record its
arguments).
"""

from __future__ import annotations

import argparse
import struct
import sys
from dataclasses import dataclass
from pathlib import Path

PIANO_HANDLER = 0xC011A7A4
ORGAN_HANDLER = 0xC011AB8C
SENDER = 0xC023195C
# VERIFIED (run): (handler, id) -> (valid `[this + 0x64]` values, valid `[event + 0xC]` values).
CASES = {
    (PIANO_HANDLER, 0xB7): ((0, 1), (7, 0xB)),
    (PIANO_HANDLER, 0xB8): ((2, 3), (7, 0xC)),
    (ORGAN_HANDLER, 0xB1): ((0, 1), (9,)),
    (ORGAN_HANDLER, 0xB2): ((2, 3), (10,)),
}
MUTE = -0.05  # VERIFIED: sentinel `0xBD4CCCCD` stored for a level of 0
SCRATCH = 0x98000000


def float_bits(x: float) -> int:
    return struct.unpack("<I", struct.pack("<f", x))[0]


def bits_float(b: int) -> float:
    return struct.unpack("<f", struct.pack("<I", b & 0xFFFFFFFF))[0]


def expected_word(value: int) -> float:
    """The law read from the listing: `(v / 127)^2`, `-0.05` below `1e-7` (in float32)."""
    x = struct.unpack("<f", struct.pack("<f", value * 65536 * (1.0 / (127 * 65536))))[0]
    if x < 1e-7:
        return MUTE
    return struct.unpack("<f", struct.pack("<f", x * x))[0]


@dataclass
class Result:
    stored: float | None
    sent: tuple[int, int, int] | None  # (table, layer, slot_word) passed to the sender


def run_handler(emu, handler: int, edit_id: int, value: int, selector: int | None = None) -> Result:  # noqa: ANN001
    """Call `handler(this, event, id, &q16)` on a fabricated controller; return the stored word."""
    selectors, kinds = CASES[(handler, edit_id)]
    this, event, cell, table = SCRATCH, SCRATCH + 0x1000, SCRATCH + 0x2000, SCRATCH + 0x4000
    layer = 3
    if not getattr(emu, "_level_scratch", False):
        emu.uc.mem_map(SCRATCH, 0x10000)
        emu._level_scratch = True
    emu.write_bytes(this, bytes(0x200))
    emu.write_bytes(event, bytes(0x40))
    emu.write_bytes(table, bytes(0x1000))
    emu.write32(this + 0x64, selectors[0] if selector is None else selector)
    emu.write32(this + 0x8C, layer)
    emu.write32(this + 0x98, table)
    emu.write32(event + 0xC, kinds[0])
    emu.write32(cell, (value << 16) & 0xFFFFFFFF)
    sent: list[tuple[int, int, int]] = []

    def sender(e) -> None:  # noqa: ANN001
        sent.append((e.reg(0), e.reg(1), e.reg(2)))

    emu.add_stub(SENDER, sender)
    emu.call(handler, this, event, edit_id, cell)
    if not sent:
        return Result(None, None)
    t, lay, slot = sent[0]
    stored = emu.read32(t + 4 * (7 * lay + slot + 0x34))
    return Result(bits_float(stored), sent[0])


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    sys.path.insert(0, str(Path(__file__).parent))
    import arm_emu as ae

    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("code", type=Path)
    args = ap.parse_args(argv)
    emu = ae.load_firmware(args.code)
    bad = 0
    for (handler, edit_id), _ in CASES.items():
        print(f"handler {handler:#x} id {edit_id:#x}")
        for v in (0, 1, 2, 10, 32, 63, 64, 100, 126, 127):
            r = run_handler(emu, handler, edit_id, v)
            exp = expected_word(v)
            ok = r.stored is not None and abs(r.stored - exp) <= 1e-6 * max(1.0, abs(exp))
            bad += not ok
            print(f"  v={v:3d} stored={r.stored} expected={exp:.9g} {'ok' if ok else 'MISMATCH'}")
    return 1 if bad else 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
