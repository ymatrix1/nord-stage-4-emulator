#!/usr/bin/env python3
"""Load a piano sound object (`.npno`) in the booted host firmware with a stubbed library volume.

The firmware keeps the sound library on FAT-like volumes behind `0xC0094EA0` (file read) and
`0xC0096264` (directory entry lookup). `LibraryStub` replaces both so that the real manager
method `0xC00C0444(mgr, 1, entry)` runs the real loader `0xC00B92E4` and creates a real sound
object from a local file (nothing from the file is committed; findings in
`docs/research/piano-sound-load.md`):

    uv run --with unicorn python tools/arm_piano_load.py target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --cache target/keydown.snap \
        --ns4b ".reference/Nord Stage 4 Factory Restore RevK v1.66.ns4b" \
        --npno "Piano/Clavinet/Clavinet D6  6.1.npno"

`device_image` rebuilds the byte layout the loader reads (a `0xB0`-byte device header, then the
backup file from its `CNSP` marker at `0x2C`); `sound_objects` and `managers` find the firmware's
pool objects by their vtable word in RAM.
"""

from __future__ import annotations

import argparse
import struct
import sys
import zipfile
from dataclasses import dataclass, field
from pathlib import Path

import arm_emu as ae
import arm_keydown as kd
import arm_steady as ast

# VERIFIED (run, `docs/research/piano-sound-load.md` section 1): file read
# `0xC0094EA0(storage, entry, offset, buf, len)` (`len` is the stack argument) and the directory
# lookup `0xC0096264(storage, entry, out)` that fills a descriptor `D` of `0x18` bytes.
FILE_READ = 0xC0094EA0
DIR_LOOKUP = 0xC0096264
# VERIFIED (run): manager method `0xC00C0444(mgr, op, arg)`, op 1 = load file `arg`; the manager
# vtable word and the sound object vtable word, used to find the objects in RAM.
MANAGER_METHOD = 0xC00C0444
MANAGER_VTABLE = 0xC02B06D0
SOUND_VTABLE = 0xC02AE808
# EMPIRICAL (settled state, `docs/research/piano-sound-load.md` section 1): the sound object
# stride in the pool array and the size of the library header block the loader copies.
SOUND_STRIDE = 0x64D0
DEVICE_HEADER = 0xB0
# VERIFIED (disassembly of `0xC0095C0C` / `0xC00B92E4`): the first `0xA4` bytes of a library file
# are copied to `D + 0x18`, the 4 bytes at `0xA4` are the type word (LE `0x6E706E6F` for a piano),
# `CNSP` follows at `0xB0` (`0xA4 + 0xC`).
TYPE_OFFSET = 0xA4
PIANO_TYPE_WORD = 0x6E706E6F
# VERIFIED (`docs/research/nord-piano-npno-and-codec.md` header table): the marker in the
# backup (`.ns4b`) copy of a piano file; everything from here on is served after the device header.
BACKUP_CNSP_OFFSET = 0x2C
# VERIFIED (run, `docs/research/piano-sound-load.md` section 2): the piano core select method
# `0xC00AAA58(core, id, arg)` finds the pool object whose `[P + 0x6420]` equals `id`, and tail-calls
# the binder `0xC00A3988(core, layer, P, arg)` that stores `P` at `[core + 4 + 4 * layer]`.
SELECT_SOUND = 0xC00AAA58
CORE0 = 0x90BF5400
# EMPIRICAL (disassembly of `0xC006CCBC`, pc `0xC006CD60`): the serial send to the DSP polls a GPIO
# input bit `(halfword >> pin) & 1 == expected` and gives up only after 4800 timer counts; with
# the interrupt-driven timer of the harness that is 4800 ticks, so the first send blocks forever.
GPIO_POLL_PC = 0xC006CD60
# D + 0xC: 2 loads the header block only (object state 2), 0 reads the 38 stroke records too.
KIND_HEADER_ONLY = 2
KIND_FULL = 0
WORD = struct.Struct("<I")


def device_image(npno: bytes, name: str = "Library.npno") -> bytes:
    """Bytes of a library file as the loader reads them: device header + backup bytes from `CNSP`.

    ASSUMED: the real device file starts with a `0xB0`-byte header (name, type word) that the
    backup archive does not carry; the loader's offsets (`CNSP` at `0xB0`, version words at
    `0xC6` and `0x6CC`, stroke records from `0x7E2`) match the backup bytes from `0x2C` shifted
    by `0x84` (see the research note). Confirmed only by the loader accepting the result.
    """
    head = bytearray(DEVICE_HEADER)
    raw = name.encode("ascii", "replace")[:0x20]
    head[: len(raw)] = raw
    head[TYPE_OFFSET : TYPE_OFFSET + 4] = WORD.pack(PIANO_TYPE_WORD)
    return bytes(head) + npno[BACKUP_CNSP_OFFSET:]


def read_npno(ns4b: Path, member: str, limit: int = 0x400000) -> bytes:
    """First `limit` bytes of `member` of the backup archive (the header tables, not the audio)."""
    with zipfile.ZipFile(ns4b) as archive, archive.open(member) as handle:
        return handle.read(limit)


@dataclass
class LibraryStub:
    """Serve `images[entry]` through the stubbed file read and directory lookup."""

    images: dict[int, bytes]
    kind: int = KIND_FULL
    reads: list[tuple[int, int, int]] = field(default_factory=list)

    def install(self, emu: ae.ArmEmu) -> LibraryStub:
        emu.add_stub(FILE_READ, self._read)
        emu.add_stub(DIR_LOOKUP, self._lookup)
        return self

    def _read(self, emu: ae.ArmEmu) -> int:
        entry, offset, buf = emu.reg(1), emu.reg(2), emu.reg(3)
        length = emu.read32(emu.reg(13))
        self.reads.append((entry, offset, length))
        data = self.images.get(entry)
        if data is None:
            return 1
        emu.write_bytes(buf, data[offset : offset + length].ljust(length, b"\0"))
        return 0

    def _lookup(self, emu: ae.ArmEmu) -> int:
        storage, entry, out = emu.reg(0), emu.reg(1), emu.reg(2)
        data = self.images.get(entry)
        if data is None:
            return 1
        volume = emu.read32(storage + 0xC) & 0xFFFF
        emu.write_bytes(out, struct.pack("<HHIIIII", volume, entry, 0, 0, self.kind, len(data), 0))
        return 0


def select_sound(emu: ae.ArmEmu, entry_id: int, core: int = CORE0, arg: int = 0) -> int:
    """Bind the loaded object with id `entry_id` (entry | volume << 14) to `core` through the
    firmware's own select method; returns its result."""
    return ast.call_preserving(emu, SELECT_SOUND, core, entry_id, arg, max_insns=20_000_000)


def bound_sound(emu: ae.ArmEmu, core: int = CORE0, layer: int = 0) -> int:
    """`[core + 4 + 4 * layer]`: the sound object the key path reads (0: none)."""
    return emu.read32(core + 4 + 4 * layer)


def install_dsp_ready(emu: ae.ArmEmu) -> None:
    """Model the DSP serial link as always ready: at the GPIO poll the loaded bit is replaced by
    the expected level (`r3 = expected << pin`, `r1` = expected, `r12` = pin), so a message
    send returns instead of waiting for the timeout. ASSUMED: the real DSP answers at once."""

    def hook(_uc, _address, _size, _user) -> None:  # noqa: ANN001
        emu.set_reg(3, (emu.reg(1) & 1) << (emu.reg(12) & 31))

    emu.uc.hook_add(emu._u.UC_HOOK_CODE, hook, begin=GPIO_POLL_PC, end=GPIO_POLL_PC)


# VERIFIED (run, `docs/research/piano-sound-load.md` section 5): the level method of the piano
# class is slot 4 of the class table `0xC02A1D20`: `0xC00A1EB4(this, P2, rec, key, X, arg6, out0,
# out1, ..)` with `P2 = P + 0x5EA0`. Its siblings are slot 4 of the tables at `0xC02A1CB4`
# (`0xC00A210C`), `0xC02A1CD8` (`0xC00A1D5C`) and `0xC02A1CFC` (`0xC00A1E00`).
LEVEL_PIANO = 0xC00A1EB4
LEVEL_METHODS = {0xC00A210C: 1, 0xC00A1D5C: 2, 0xC00A1E00: 3, LEVEL_PIANO: 4}
# VERIFIED (read from the listing, matches `piano/level_q23.rs`): index step per octave.
LEVEL_STEPS = 60


@dataclass
class LevelCall:
    """One call of a level method: call-frame word `x`, record halfword `rec5e` (`[rec + 0x5E]`),
    key argument and the amplitude the method stored through its first output pointer."""

    x: int
    rec5e: int
    key: int
    out: int | None


def level_index(rec5e: int, x: int, b1: int, b2: int, b3: int) -> int:
    """`n = rec5e + 2 X + 60 - b1 - b2 - b3` of the level methods (before the `max(0, n)`)."""
    return rec5e + 2 * x + LEVEL_STEPS - b1 - b2 - b3


def level_amplitude(index: int) -> int:
    """Q23 amplitude of a level index: `round(2^23 * 2^(-n / 60))` reduced by octaves, saturated
    at `0x7FFFFF` (same law as `crates/nord-core/src/piano/level_q23.rs`)."""
    n = max(index, 0)
    table = round(8388608 * 2 ** (-(n % LEVEL_STEPS) / LEVEL_STEPS))
    return min(table >> (n // LEVEL_STEPS), 0x7FFFFF)


def hook_level_calls(emu: ae.ArmEmu, method: int = LEVEL_PIANO) -> list[LevelCall]:
    """Record every call of `method` (entry arguments, then the value written at its first
    output pointer on return); returns the live list."""
    calls: list[LevelCall] = []

    def entry(_uc, _address, _size, _user) -> None:  # noqa: ANN001
        sp, lr = emu.reg(13), emu.reg(14)
        x, out_ptr = emu.read32(sp), emu.read32(sp + 8)
        (rec5e,) = struct.unpack("<h", emu.read_bytes(emu.reg(2) + 0x5E, 2))
        call = LevelCall(x, rec5e, emu.reg(3), None)
        calls.append(call)
        handle: list[int] = []

        def back(_uc2, _a, _s, _u) -> None:  # noqa: ANN001
            call.out = emu.read32(out_ptr)
            emu.uc.hook_del(handle[0])

        handle.append(emu.uc.hook_add(emu._u.UC_HOOK_CODE, back, begin=lr, end=lr))

    emu.uc.hook_add(emu._u.UC_HOOK_CODE, entry, begin=method, end=method)
    return calls


def find_words(
    emu: ae.ArmEmu, value: int, lo: int = 0x90000000, hi: int = 0x92000000, step: int = 0x100000
) -> list[int]:
    """Addresses in `[lo, hi)` whose aligned word equals `value` (unmapped chunks skipped)."""
    pattern, found = WORD.pack(value), []
    for base in range(lo, hi, step):
        try:
            chunk = emu.read_bytes(base, step)
        except Exception:  # noqa: BLE001 - unicorn raises UcError on unmapped memory
            continue
        at = chunk.find(pattern)
        while at >= 0:
            if at % 4 == 0:
                found.append(base + at)
            at = chunk.find(pattern, at + 1)
    return found


def managers(emu: ae.ArmEmu) -> list[int]:
    """Library manager objects (vtable `MANAGER_VTABLE`); index 0 serves volume 0."""
    return find_words(emu, MANAGER_VTABLE)


def sound_objects(emu: ae.ArmEmu) -> list[int]:
    """Pool objects of the sound class (vtable `SOUND_VTABLE`), `SOUND_STRIDE` apart."""
    return find_words(emu, SOUND_VTABLE)


def load_sound(emu: ae.ArmEmu, manager: int, entry: int, max_insns: int = 20_000_000) -> int:
    """`0xC00C0444(manager, 1, entry)` from the settled state; returns the loader status (0: ok)."""
    return ast.call_preserving(emu, MANAGER_METHOD, manager, 1, entry, max_insns=max_insns)


def sound_state(emu: ae.ArmEmu, obj: int) -> int:
    """State word `[obj + 0x64C0]` that `0xC00B90F8` stores (the mode: 0 / 1 / 2 ...)."""
    return emu.read32(obj + 0x64C0)


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path, nargs="?")
    ap.add_argument("--cache", type=Path)
    ap.add_argument("--ns4b", type=Path, required=True)
    ap.add_argument("--npno", required=True, help="archive member of the piano file")
    ap.add_argument("--kind", type=int, default=KIND_FULL)
    ap.add_argument("--note", type=int)
    ap.add_argument("--velocity", type=int, default=100)
    ns = ap.parse_args(argv)
    rig = kd.build_rig(ns.image, ns.sram, ns.cache)
    stub = LibraryStub({1: device_image(read_npno(ns.ns4b, ns.npno), "Library.npno")}, ns.kind)
    stub.install(rig.emu)
    mgrs = managers(rig.emu)
    print("managers", [hex(m) for m in mgrs])
    status = load_sound(rig.emu, mgrs[0], 1)
    objs = sound_objects(rig.emu)
    print("load status", status, "reads", len(stub.reads), "first object", hex(objs[0]))
    print("state", sound_state(rig.emu, objs[0]))
    if ns.note is None:
        return 0
    select_sound(rig.emu, 1)
    print("bound", hex(bound_sound(rig.emu)))
    install_dsp_ready(rig.emu)
    counts = kd.count_calls(rig.emu)
    rig.tick(2)
    rows = kd.capture_note(rig, ns.note, ns.velocity, counts, on_ticks=40, off_ticks=10)
    print({k: v for k, v in counts.items() if v})
    print(kd.CSV_HEADER)
    print(chr(10).join(rows))
    return 0


if __name__ == "__main__":
    sys.exit(main())
