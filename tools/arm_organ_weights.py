#!/usr/bin/env python3
"""Organ tonewheel weights from the real ARM code: key to wheel map, drawbar scale, message burst.

The booted host firmware (`arm_steady.py`) owns four *bank engine* objects (one per mix-stage set
`s` = 0..3, table `BANK_TABLE`) that turn key events and drawbar levels into the 92 wheel weights
`a`, `b`, `wB` of the PNO core 1 mix stage (`organ-oracle-audio.md` section 3). This tool drives
them like the firmware does and records the DSP messages:

* `NOTE_ON(engine, 0, &{note, velocity << 8}, 1)` is the virtual key-down handler (vtable slot
  `0xC05267BC`); it keeps a per-key item and sends the first `a / b / wB` partial update.
* `UPDATE(engine)` (`0xC022AFB8`, needs bytes `+0x4C` and `+0x4D` set) accumulates every active
  key times every drawbar into 92 integers and sends all `a`, `b`, `wB` words.

    uv run --with unicorn python tools/arm_organ_weights.py keymap \
        target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin \
        --cache target/settled.snap --csv keymap.csv
    uv run --with unicorn python tools/arm_organ_weights.py verify  ...   # model vs firmware
    uv run --with unicorn python tools/arm_organ_weights.py curve   ...   # drawbar / velocity sweep
    uv run --with unicorn python tools/arm_organ_weights.py script  ...   # sharc-bank replay burst

Evidence, formulas and limits: `docs/research/organ-weights.md`.
"""

from __future__ import annotations

import argparse
import struct
import sys
from collections.abc import Callable, Iterable
from dataclasses import dataclass, field
from pathlib import Path

import arm_emu as ae
import arm_steady as ast

# VERIFIED (run, `organ-weights.md` section 2): table of the four bank engine objects, index 1..4
# = mix-stage set 0..3 (vtable `0xC05267B4`, set index at `+0x18`, proxy object at `+0x10`).
BANK_TABLE = 0xC05A0424
SETS = 4
# VERIFIED (disassembly + run): virtual key-down handler `(this, _, &{note, vel << 8}, 1)` and the
# update method that sends all weights (checks `+0x4D`, then `+0x4C` as dirty flag).
NOTE_ON = 0xC022C3A8
UPDATE = 0xC022AFB8
# VERIFIED (disassembly of `0xC022AFC4`, run): the nine drawbar indices (0..8) of the engine sit at
# `+0x28 + 4 j`; the two flag bytes at `+0x4C`, `+0x4D`. ASSUMED: the firmware path that writes the
# indices was not found (no edit id of `arm-edit-params.csv` changes them: 248 ids, 2 layers), so
# `BankDriver.set_drawbars` pokes them.
DRAWBAR_OFFSET = 0x28
FLAG_OFFSET = 0x4C
# VERIFIED (read from the image, `arm-organ-messages.md` section 7): drawbar index to 7-bit level.
DRAWBAR_7BIT = (0, 3, 6, 13, 22, 39, 62, 87, 127)
# VERIFIED (run: wheel of every (note, drawbar) pair, `verify`): drawbar j sounds the wheel of
# `note - 24 + FOOTAGE_SEMITONES[j]`, then folded into the bank range (`fold_wheel`).
FOOTAGE_NAMES = ("16", "5 1/3", "8", "4", "2 2/3", "2", "1 3/5", "1 1/3", "1")
FOOTAGE_SEMITONES = (-12, 7, 0, 12, 19, 24, 28, 31, 36)
WHEELS = 92
# VERIFIED (run, `verify`: 128 x 9 wheel indices): lowest MIDI note of wheel 0 = 24; wheels below
# 0 move up by octaves into 0..11, wheels above `FOLD_TOP` move down by octaves into
# `FOLD_TOP - 11 .. FOLD_TOP`; `FOLD_TOP = 90` is the only value that reproduces all 1152 indices.
WHEEL0_NOTE = 24
FOLD_TOP = 90
# VERIFIED (run): DM word bases of the three arrays sent by the engine of set `s`
# (`organ-oracle-audio.md` section 3): `a = 0x9594F + 0x65 s + i`, `b = 0x95AE2 + 0x64 s + i`,
# `wB = 0x957DE + 0x5C s + i`. `wA` (`0x9566E`) is never sent.
A_BASE, A_STRIDE = 0x9594F, 0x65
B_BASE, B_STRIDE = 0x95AE2, 0x64
WB_BASE, WB_STRIDE = 0x957DE, 0x5C
# VERIFIED (run, scale fit over 5 velocities, max rel err 1e-7): the sender converts the
# accumulated integer `N` to float as `N * 2**-23` (`0xC001FF18`: exponent `135 - clz`).
FIXED_SHIFT = 23
SCRATCH = 0x98000000


@dataclass(frozen=True)
class KeyEntry:
    """Wheel index and tapering byte per drawbar of one MIDI note (read from the engine)."""

    note: int
    wheels: tuple[int, ...]
    taper: tuple[int, ...]


@dataclass(frozen=True)
class Weight:
    """One decoded `a` / `b` / `wB` word: `kind`, mix-stage set, wheel, raw float bits."""

    kind: str
    set: int
    wheel: int
    bits: int

    @property
    def value(self) -> float:
        return struct.unpack("<f", struct.pack("<I", self.bits))[0]


@dataclass
class Capture:
    """Messages of one scenario, plus the decoded PNO core 1 weight words."""

    messages: list[ae.Message] = field(default_factory=list)

    @property
    def weights(self) -> list[Weight]:
        return [w for m in self.messages if (w := decode_message(m)) is not None]

    def wb(self, set_index: int = 0) -> dict[int, int]:
        """Last `wB` word (raw bits) per wheel of one set."""
        return {w.wheel: w.bits for w in self.weights if w.kind == "wB" and w.set == set_index}


def classify(dm_word: int) -> tuple[str, int, int] | None:
    """`(kind, set, wheel)` of a DM word in the `a` / `b` / `wB` arrays, else None."""
    for kind, base, stride in (
        ("a", A_BASE, A_STRIDE),
        ("b", B_BASE, B_STRIDE),
        ("wB", WB_BASE, WB_STRIDE),
    ):
        for s in range(SETS):
            first = base + stride * s
            if first <= dm_word < first + WHEELS:
                return kind, s, dm_word - first
    return None


def dm_address(kind: str, set_index: int, wheel: int) -> int:
    """Inverse of `classify`."""
    base, stride = {"a": (A_BASE, A_STRIDE), "b": (B_BASE, B_STRIDE), "wB": (WB_BASE, WB_STRIDE)}[
        kind
    ]
    return base + stride * set_index + wheel


def decode_message(m: ae.Message) -> Weight | None:
    """The weight word a PNO core 1 path 2 message writes, or None."""
    if (m.chip, m.core, m.path) != ("PNO", 1, 2):
        return None
    hit = classify(m.dm_word)
    return Weight(hit[0], hit[1], hit[2], m.word2) if hit else None


def fixed_to_float(n: int) -> float:
    """`n * 2**-23` rounded to float32, as the firmware sender makes it (positive `n`)."""
    return struct.unpack("<f", struct.pack("<f", n * 2.0**-FIXED_SHIFT))[0]


def fold_wheel(w: int) -> int:
    """Fold a raw wheel number into `0..FOLD_TOP` by whole octaves (VERIFIED against the engine)."""
    while w < 0:
        w += 12
    while w > FOLD_TOP:
        w -= 12
    return w


def expected_wheels(note: int) -> tuple[int, ...]:
    """Wheel of each drawbar for `note` (the rule the engine tables follow)."""
    return tuple(fold_wheel(note - WHEEL0_NOTE + off) for off in FOOTAGE_SEMITONES)


def model_wb_ints(
    keys: Iterable[tuple[int, int]], levels: tuple[int, ...], taper: int = 127
) -> dict[int, int]:
    """Integer `wB` per wheel: `sum d7[j] * ((vel * taper) >> 4)` over keys and drawbars.

    VERIFIED against the firmware for every note (`verify`) with `taper = 127` (all tapering bytes
    read 127) and `levels` = `DRAWBAR_7BIT[index]`.
    """
    acc: dict[int, int] = {}
    for note, vel in keys:
        for j, w in enumerate(expected_wheels(note)):
            acc[w] = acc.get(w, 0) + levels[j] * ((vel * taper) >> 4)
    return {w: n for w, n in acc.items() if n}


def key_table(read32: Callable[[int], int], engine: int, notes: int = 128) -> list[KeyEntry]:
    """Per-note wheel indices and taper bytes of one bank engine (items of `[engine + 0x14]`)."""
    sub = read32(engine + 0x14)
    items = read32(sub + 0x28)
    out = []
    for n in range(notes):
        item = read32(items + 4 * n)
        wheels, taper = read32(item), read32(item + 4)
        out.append(
            KeyEntry(
                n,
                tuple(read32(wheels + 4 * j) for j in range(9)),
                tuple(read32(taper + 4 * j) for j in range(9)),
            )
        )
    return out


def keymap_csv(table: list[KeyEntry]) -> str:
    """One line per note: the nine wheel indices and the nine taper bytes."""
    head = ["note"] + [f"wheel_{f.replace(' ', '_').replace('/', 'o')}" for f in FOOTAGE_NAMES]
    head += [f"taper_{f.replace(' ', '_').replace('/', 'o')}" for f in FOOTAGE_NAMES]
    rows = [",".join(head)]
    rows += [",".join(str(x) for x in (e.note, *e.wheels, *e.taper)) for e in table]
    return "\n".join(rows) + "\n"


def script_lines(messages: Iterable[ae.Message], call: int, only_set: int | None = None) -> str:
    """`sharc-bank` / `sharc-organ --script` lines `CALL ADDR VALUE` for the PNO core 1 words."""
    out = []
    for m in messages:
        w = decode_message(m)
        if w is None or (only_set is not None and w.set != only_set):
            continue
        out.append(f"{call} {m.dm_word:#x} {m.word2:#010x}  # {w.kind}[{w.wheel}] set {w.set}")
    return "\n".join(out) + ("\n" if out else "")


class BankDriver:
    """Drives the bank engines of a settled emulator; every scenario starts from the snapshot."""

    def __init__(self, emu: ae.ArmEmu, blob: bytes) -> None:
        self.emu = emu
        self.blob = blob
        if not any(b <= SCRATCH <= e for b, e, _ in emu.uc.mem_regions()):
            emu.uc.mem_map(SCRATCH, ae.PAGE)

    def engine(self, set_index: int) -> int:
        return self.emu.read32(BANK_TABLE + 4 * (set_index + 1))

    def reset(self) -> None:
        ast.restore(self.emu, self.blob)
        self.emu.install_dsp_ports()
        self.emu.clear_messages()

    def note_on(self, set_index: int, note: int, velocity: int = 127) -> list[ae.Message]:
        self.emu.write32(SCRATCH, note)
        self.emu.write32(SCRATCH + 4, velocity << 8)
        self.emu.clear_messages()
        ast.call_preserving(self.emu, NOTE_ON, self.engine(set_index), 0, SCRATCH, 1)
        return self.emu.messages

    def set_drawbars(self, set_index: int, indices: tuple[int, ...]) -> None:
        eng = self.engine(set_index)
        for j, v in enumerate(indices):
            self.emu.write32(eng + DRAWBAR_OFFSET + 4 * j, v)
        self.emu.uc.mem_write(eng + FLAG_OFFSET, bytes([1, 1]))

    def update(self, set_index: int) -> list[ae.Message]:
        self.emu.clear_messages()
        ast.call_preserving(self.emu, UPDATE, self.engine(set_index))
        return self.emu.messages

    def scenario(
        self, notes: Iterable[tuple[int, int]], indices: tuple[int, ...], set_index: int = 0
    ) -> tuple[Capture, Capture]:
        """Fresh state, keys down, drawbars set, update: returns `(note-on burst, update burst)`."""
        self.reset()
        on = Capture()
        for note, vel in notes:
            on.messages += self.note_on(set_index, note, vel)
        self.set_drawbars(set_index, indices)
        return on, Capture(self.update(set_index))


def verify(driver: BankDriver, notes: Iterable[int]) -> list[str]:
    """Compare the model with the firmware for single notes, all drawbars at index 8."""
    bad = []
    for n in notes:
        _, up = driver.scenario([(n, 127)], (8,) * 9)
        got = up.wb(0)
        want = {
            w: struct.unpack("<I", struct.pack("<f", fixed_to_float(v)))[0]
            for w, v in model_wb_ints([(n, 127)], (127,) * 9).items()
        }
        got_nz = {w: b for w, b in got.items() if b}
        if got_nz != want:
            bad.append(
                f"note {n}: firmware {sorted(got_nz.items())[:4]} model {sorted(want.items())[:4]}"
            )
    return bad


def curve_rows(
    driver: BankDriver, note: int = 60, drawbar: int = 2
) -> list[tuple[str, int, int, int, int, int]]:
    """Measured `wB` of one wheel against the drawbar index (velocity 127) and the velocity.

    Rows `(sweep, value, wheel, bits, model_bits, d7)`; the model is `model_wb_ints`.
    """
    wheel = expected_wheels(note)[drawbar]
    rows = []
    for sweep, values in (("drawbar", range(9)), ("velocity", (1, 16, 32, 64, 127))):
        for v in values:
            idx = [0] * 9
            idx[drawbar] = v if sweep == "drawbar" else 8
            vel = 127 if sweep == "drawbar" else v
            _, up = driver.scenario([(note, vel)], tuple(idx))
            levels = tuple(DRAWBAR_7BIT[i] for i in idx)
            want = model_wb_ints([(note, vel)], levels).get(wheel, 0)
            want_bits = struct.unpack("<I", struct.pack("<f", fixed_to_float(want)))[0]
            rows.append((sweep, v, wheel, up.wb(0).get(wheel, 0), want_bits, levels[drawbar]))
    return rows


def curve_csv(rows: list[tuple[str, int, int, int, int, int]]) -> str:
    lines = ["sweep,value,wheel,wB_bits,model_bits,d7,wB"]
    for sweep, v, wheel, bits, want, d7 in rows:
        val = struct.unpack("<f", struct.pack("<I", bits))[0]
        lines.append(f"{sweep},{v},{wheel},{bits:#010x},{want:#010x},{d7},{val:.9g}")
    return "\n".join(lines) + "\n"


def prepare(code: Path, sram: Path | None, cache: Path | None) -> BankDriver:
    import arm_edit_capture as aec

    emu, blob = aec.settled_emulator(code, sram, cache)
    return BankDriver(emu, blob)


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("command", choices=("keymap", "verify", "script", "curve"))
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path, nargs="?")
    ap.add_argument("--cache", type=Path)
    ap.add_argument("--csv", type=Path)
    ap.add_argument("--notes", default="60", help="comma separated MIDI notes (script, verify)")
    ap.add_argument("--drawbars", default="888000000", help="nine digits 0..8")
    ap.add_argument("--set", type=int, default=0)
    ap.add_argument("--update-only", action="store_true", help="script: only the update burst")
    ns = ap.parse_args(argv)
    d = prepare(ns.image, ns.sram, ns.cache)
    notes = [int(x) for x in ns.notes.split(",")]
    if ns.command == "keymap":
        text = keymap_csv(key_table(d.emu.read32, d.engine(ns.set)))
        (ns.csv.write_text(text, encoding="utf-8") if ns.csv else print(text, end=""))
    elif ns.command == "curve":
        text = curve_csv(curve_rows(d))
        (ns.csv.write_text(text, encoding="utf-8") if ns.csv else print(text, end=""))
    elif ns.command == "verify":
        bad = verify(d, range(128) if ns.notes == "60" else notes)
        print("\n".join(bad) or "all notes match the model")
        return 1 if bad else 0
    else:
        idx = tuple(int(c) for c in ns.drawbars)
        on, up = d.scenario([(n, 127) for n in notes], idx, ns.set)
        text = script_lines(([] if ns.update_only else on.messages) + up.messages, 0, ns.set)
        (ns.csv.write_text(text, encoding="utf-8") if ns.csv else print(text, end=""))
    return 0


if __name__ == "__main__":
    sys.exit(main())
