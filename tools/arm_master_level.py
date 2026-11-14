#!/usr/bin/env python3
"""Run the ARM writer of the FX master level word `DM(0xB6518)` in the settled firmware emulator.

`docs/research/piano-receiver-scale.md` found that the FX Core1 output stage multiplies by
`DM(0xB6518) * DM(0xB6519) * DM(0xB651A)` and that `DM(0xB6518)` is 0 in the image. The word is
record 2 of the channel table `0xC0549E28` (`{target 0 = FX core 1, base 0x282D9460}`) and has two
ARM senders that share the mixer object `[0xC055D720]` (heap object, `{mult, taper, ...}`):

* `0xC01897A4` (event with `[event + 8] == 0`, value `[event + 0xC] = v`, 0..127): sets
  `mult = (v * 2^16 * 0x34010204)^2 = (v / 127)^2` and sends `mult * taper`
* `0xC010BE18` (`Fnc_Volume` knob index `i`): sets `taper = TABLE[i]` (128 floats at
  `0xC059FACC`, `TABLE[127] = 1.0`) and sends `taper * mult`

The mixer constructor (`0xC0249F00`) stores `mult = 0.0`, `taper = 1.0`, so the DSP keeps its image
word 0 (muted) until the first volume event; with the defaults (127 and 127) the word is 1.0.

    uv run --with unicorn python tools/arm_master_level.py run target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --cache target/settled.snap

`word` prints the law without firmware (`python tools/arm_master_level.py word 100`).
`layer` calls the layer level sender `0xC023195C(mixer, layer, slot)` with the "active" flag
`[mixer + 7 layer + slot + 0x140]` set and the sent-word memos cleared, and prints the DSP words
it writes (organ chain gains in PNO core 1, FX channel gains in FX core 1).
"""

from __future__ import annotations

import argparse
import struct
import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))

# VERIFIED (disassembly 0xC01898E4..0xC018990C, run): `lsl r0, r0, #16`, int-to-float, multiply by
# the literal `0x34010204` (1 / (127 * 65536)), then `x * x`.
Q16_SCALE_WORD = 0x34010204
# VERIFIED (record 2 of table 0xC0549E28, `arm-chan-tables.csv`): FX core 1, byte address of
# `DM(0xB6518)`.
MASTER_LEVEL_ADDRESS = 0x282D9460
MASTER_LEVEL_DM = 0xB6518
# VERIFIED (disassembly): the mixer object pointer, the two senders and the knob handler.
MIXER_GLOBAL = 0xC055D720
VOLUME_EVENT_SENDER = 0xC01897A4
KNOB_HANDLER = 0xC010BE18
TAPER_TABLE_OFFSET = 0x204  # words `[0xC059F8C8 + 0x204 + 4 i]`, i = 0..127
TAPER_BASE = 0xC059F8C8
# VERIFIED (disassembly `0xC01246A0` compare in the sender prologue): vtable word that lets the
# sender reach its volume branch without the UI objects.
VTABLE_STUB = 0xC01246A0
SCRATCH = 0x98000000


def f32(x: float) -> float:
    return struct.unpack("<f", struct.pack("<f", x))[0]


def f32_bits(x: float) -> int:
    return struct.unpack("<I", struct.pack("<f", x))[0]


def bits_f32(b: int) -> float:
    return struct.unpack("<f", struct.pack("<I", b & 0xFFFFFFFF))[0]


def midi_volume_mult(v: int) -> float:
    """Multiplier the firmware stores for a volume event `v` (0..127): `(v / 127)^2`, float32."""
    x = f32(f32(float(v << 16)) * bits_f32(Q16_SCALE_WORD))
    return f32(x * x)


def master_level_word(v: int, taper: float = 1.0) -> float:
    """`DM(0xB6518)` as the ARM sends it: `midi_volume_mult(v) * taper` (float32 product)."""
    return f32(midi_volume_mult(v) * f32(taper))


def run(
    code: Path, sram: Path | None, cache: Path | None
) -> int:  # pragma: no cover - needs firmware
    import arm_edit_capture as cap

    emu, _blob = cap.settled_emulator(code, sram, cache)
    emu.uc.mem_map(SCRATCH, 0x10000)
    mixer = emu.read32(MIXER_GLOBAL)
    mult, taper0 = bits_f32(emu.read32(mixer)), bits_f32(emu.read32(mixer + 4))
    print(f"mixer object {mixer:#x}: mult {mult} taper {taper0}")
    # fake `this`: vtable slot compare passes, no UI children
    emu.write32(SCRATCH + 0x14, VTABLE_STUB)
    emu.write32(SCRATCH + 0x100, SCRATCH)
    this = SCRATCH + 0x200
    for off in (0x18C, 0x284):
        emu.write32(this + off, SCRATCH + 0x100)
    event = SCRATCH + 0x400
    for v in (127, 100, 64, 0, 127):
        emu.clear_messages()
        for off, val in ((0, 0), (4, 0), (8, 0), (12, v)):
            emu.write32(event + off, val)
        emu.call(VOLUME_EVENT_SENDER, this, event)
        sent = [m for m in emu.messages if m.dm_word == MASTER_LEVEL_DM]
        words = [f"{m.chip} core {m.core} {m.word2:#010x}" for m in sent]
        print(
            f"volume {v:3d}: stored {bits_f32(emu.read32(mixer)):.7f} sent {words} "
            f"model {f32_bits(master_level_word(v)):#010x}"
        )
    taper = [bits_f32(emu.read32(TAPER_BASE + TAPER_TABLE_OFFSET + 4 * i)) for i in range(128)]
    print(
        "taper", " ".join(f"{i}:{taper[i]:.4f}" for i in (0, 1, 16, 32, 64, 96, 112, 120, 126, 127))
    )
    return 0


LAYER_SENDER = 0xC023195C

# VERIFIED (disassembly 0xC023195C..0xC0231B0C, run of `modes`): the sender maps the slot (0, 1
# organ layers A and B, 2, 3 piano, 4..6 synth) and the layer mode flag `[mixer + scene + 0x5B8]`
# (the Reverb GLOBAL state: the unit store writer at 0xC010E36C sets it to `value(0x1B9) == 1`) to a
# row index `r8` (id `[mixer + 8 + 4 (25 scene + r8)]`) and a scale cell index `k` (the cell is
# `[mixer + 0x150 + 4 k]`).
ROW_FLAG0 = (7, 8, 12, 13, 14)  # byte table 0xC02B8B40, slots 2..6 with Reverb GLOBAL off
CELL_OF_INDEX = (
    1,
    2,
    3,
    4,
    4,
    6,
    6,
    8,
    8,
    8,
    10,
    10,
    10,
    5,
    5,
    9,
    9,
    9,
    7,
    7,
    11,
    11,
    11,
)  # 0xC02B8B54
# VERIFIED (run): scale cells of the constructor at 0xC0249F00.
SCALE_CELLS = (1.6, 1.3, 1.0, 1.0, 1.14, 2.5, 1.0, 1.0, 0.25, 3.25, 1.0, 1.0)


def layer_scale_cell(slot: int, reverb_global: bool) -> tuple[int, int]:
    """`(row r8, scale cell k)` the layer sender picks for `slot` 0..6 and the mode flag."""
    if slot < 2:  # organ slots: the index is -2 / -1, out of the cell table, so cell 0
        return slot, 0
    if reverb_global:
        return slot + 13, CELL_OF_INDEX[slot + 11]
    row = ROW_FLAG0[slot - 2]
    return row, CELL_OF_INDEX[row - 2]


def layer_scale(slot: int, reverb_global: bool) -> float:
    """Scale the sender applies to a layer level word of 1.0 for the slot (float32 cell)."""
    return f32(SCALE_CELLS[layer_scale_cell(slot, reverb_global)[1]])


def layer(code: Path, sram: Path | None, cache: Path | None) -> int:  # pragma: no cover
    import arm_edit_capture as cap

    emu, _blob = cap.settled_emulator(code, sram, cache)
    mixer = emu.read32(MIXER_GLOBAL)
    print(
        "scale cells [mixer + 0x150 + 4 k]:",
        [round(bits_f32(emu.read32(mixer + 0x150 + 4 * k)), 4) for k in range(12)],
    )
    for lay in (0, 1):
        for slot in range(7):
            r8 = slot if slot < 2 else slot + 13
            idx = 25 * lay + r8
            emu.uc.mem_write(mixer + 7 * lay + slot + 0x140, b"")
            for off in (0x344, 0x40C):
                emu.write32(mixer + off + 4 * idx, 0x12345678)
            emu.clear_messages()
            emu.call(LAYER_SENDER, mixer, lay, slot)
            sent = [
                (m.chip, m.core, hex(m.dm_word), round(bits_f32(m.word2), 5)) for m in emu.messages
            ]
            print(f"layer {lay} slot {slot}: {sent}")
    return 0


def modes(code: Path, sram: Path | None, cache: Path | None) -> int:  # pragma: no cover
    """Run the layer sender for both values of the mode flag and compare with `layer_scale`."""
    import arm_edit_capture as cap

    emu, _blob = cap.settled_emulator(code, sram, cache)
    mixer = emu.read32(MIXER_GLOBAL)
    bad = 0
    # the "active" bytes `[mixer + 7 scene + slot + 0x140]` select the product of the two level
    # cells (layer A 1.0 in the settled state) instead of the mute sentinel -0.05
    emu.uc.mem_write(mixer + 0x140, bytes([1] * 7))
    for flag in (0, 1):
        emu.uc.mem_write(mixer + 0x5B8, bytes([flag]))  # scene 0 flag byte
        for slot in range(7):
            row, _cell = layer_scale_cell(slot, bool(flag))
            for off in (0x344, 0x40C):
                emu.write32(mixer + off + 4 * row, 0x12345678)
            emu.clear_messages()
            emu.call(LAYER_SENDER, mixer, 0, slot)
            got = sorted({round(bits_f32(m.word2), 4) for m in emu.messages})
            want = layer_scale(slot, bool(flag))
            ok = got == [round(want, 4)]
            bad += not ok
            print(
                f"reverb_global {flag} slot {slot} row {row}: sent {got} model {want:.4f} "
                f"{'ok' if ok else 'MISMATCH'}"
            )
    return 1 if bad else 0


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    w = sub.add_parser("word", help="the law without firmware")
    w.add_argument("volume", type=int)
    w.add_argument("--taper", type=float, default=1.0)
    r = sub.add_parser("run", help="run the senders in the settled emulator")
    r.add_argument("image", type=Path)
    r.add_argument("sram", type=Path, nargs="?")
    r.add_argument("--cache", type=Path)
    ly = sub.add_parser("layer", help="run the layer level sender 0xC023195C")
    ly.add_argument("image", type=Path)
    ly.add_argument("sram", type=Path, nargs="?")
    ly.add_argument("--cache", type=Path)
    md = sub.add_parser("modes", help="layer sender for both mode flag values")
    md.add_argument("image", type=Path)
    md.add_argument("sram", type=Path, nargs="?")
    md.add_argument("--cache", type=Path)
    ns = ap.parse_args(argv)
    if ns.cmd == "modes":
        return modes(ns.image, ns.sram, ns.cache)
    if ns.cmd == "layer":
        return layer(ns.image, ns.sram, ns.cache)
    if ns.cmd == "word":
        word = master_level_word(ns.volume, ns.taper)
        print(f"{word:.7f} {f32_bits(word):#010x}")
        return 0
    return run(ns.image, ns.sram, ns.cache)


if __name__ == "__main__":
    sys.exit(main())
