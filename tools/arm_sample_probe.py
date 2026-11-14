#!/usr/bin/env python3
"""Probe how the booted host firmware could be given a SAMPLES layer (`.nsmp4` from the backup).

Findings (evidence and limits: `docs/research/synth-sample-kernel-rule.md` section 10):

* The library file loader `0xC00B92E4` behind `arm_piano_load.LibraryStub` is **not** the sample
  path. Serving an `.nsmp4` image through it ends in object state 3 / 4 (no object bound), because
  the loader only knows the type words `onpn`, `nsp\\0`, `npdl`, `map\\0`, `abc\\0` and checks a
  `CNSP` magic. `try_load` reproduces that run.
* **The sample loader is `0xC01D710C`** (section 11 of the note): serving a `nsmp` device image
  (`sample_device_image`) plus one directory entry (`serve_sample`) makes the real loader return 0
  with status "Sample is ok"; `bind_layer` then fills the layer slot list.
* The synth voice event `0xC01EC690 -> 0xC01E72C4` ends before any SY message for every layer of
  the settled state: the per-layer synth object `[0xC05B028C + 4 layer]` holds an empty sample slot
  list (`[[obj + 0x4C0]] == 0`) and zero polyphony limits. `synth_gate` reads those fields.

    uv run --with unicorn python tools/arm_sample_probe.py target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --cache target/keydown.snap \
        --ns4b ".reference/Nord Stage 4 Factory Restore RevK v1.66.ns4b" \
        --member "Samp Lib/Accordion_Harm/Carina Accordion 1_PS 4.1.nsmp4"

Nothing from the backup is written to disk.
"""

from __future__ import annotations

import argparse
import struct
import sys
from dataclasses import dataclass
from pathlib import Path

import arm_emu as ae
import arm_keydown as kd
import arm_piano_load as pl
import arm_steady as ast

# VERIFIED (all 195 `.nsmp4` of the factory backup, `survey`): 48-byte `CBIN` header whose kind
# word at `+8` is `nsmp`, then `NSMP` at `0x2C`, a 12-byte prefix, and chunks
# `tag(4) | id u32 BE | length u32 BE | payload` (tags `\0hdr`, `\0cat`, `\0map`, `\0stk`).
KIND_OFFSET = 8
KIND_NSMP = b"nsmp"
MAGIC_OFFSET = 0x2C
MAGIC_NSMP = b"NSMP"
CHUNK_START = 0x3C
# VERIFIED (disassembly of `0xC00B92E4`): the type words (LE halves of the device header word at
# `0xA4`) the generic library loader compares, in order of the compare chain.
LOADER_TYPE_WORDS = {
    "onpn": 0x6E706E6F,
    "nsp": 0x6E737000,
    "npdl": 0x6E70646C,
    "map": 0x6D617000,
    "abc": 0x61626300,
}
# VERIFIED (run, `synth_gate`): per-layer synth object table and the three fields that stop the
# synth voice event `0xC01E72C4` (sample slot list pointer, enable word, status byte).
SYNTH_LAYER_TABLE = 0xC05B028C
SLOT_LIST_OFFSET, STATUS_OFFSET, ENABLE_OFFSET = 0x4C0, 0x4BC, 8
# VERIFIED (listing `0xC01E7554..0xC01E7600`): three per-kind voice counters of the allocator at
# `0xC05B0C78 + 0x78 / 0x7C / 0x80`, compared with limits read from the object `[owner + 8]`
# (`+0xD0`, `+0x2DC`, `+0x4E8`; not the layer table object, so not read here).
COUNTER_BASE = 0xC05B0C78
COUNTER_OFFSETS = (0x78, 0x7C, 0x80)


@dataclass(frozen=True)
class Chunk:
    """One tagged chunk of an `.nsmp4`: tag (without the leading NUL), id, payload span."""

    tag: str
    chunk_id: int
    start: int
    length: int


def is_nsmp(data: bytes) -> bool:
    """True for a `CBIN` header of kind `nsmp` followed by `NSMP` at `0x2C`."""
    return (
        data[:4] == b"CBIN"
        and data[KIND_OFFSET : KIND_OFFSET + 4] == KIND_NSMP
        and data[MAGIC_OFFSET : MAGIC_OFFSET + 4] == MAGIC_NSMP
    )


def chunks(data: bytes) -> list[Chunk]:
    """Chunks that lie completely inside `data` (a truncated tail chunk is dropped)."""
    out: list[Chunk] = []
    pos = CHUNK_START
    while pos + 12 <= len(data):
        tag, chunk_id, length = struct.unpack(">4sII", data[pos : pos + 12])
        if tag[:1] != b"\0" or not tag[1:].isalpha():
            break
        if pos + 12 + length > len(data):
            break
        out.append(Chunk(tag[1:].decode("ascii"), chunk_id, pos + 12, length))
        pos += 12 + length
    return out


def sample_name(data: bytes) -> str:
    """Name string of the `hdr` chunk (first printable run after its 10-byte prefix).

    EMPIRICAL (one file read by hand, then all 195 by `survey`): the payload is
    `00 00 00 00 03 a0 00 00 0a 01 | name ... NUL`.
    """
    for chunk in chunks(data):
        if chunk.tag == "hdr":
            raw = data[chunk.start + 10 : chunk.start + chunk.length]
            return raw.split(b"\0", 1)[0].decode("ascii", "replace")
    return ""


def device_image(data: bytes, type_word: int, name: str = "Sample.nsmp") -> bytes:
    """Library device image of `data`: `0xB0`-byte header (name, type word at `0xA4`) then the
    backup bytes from `0x2C` (the layout `arm_piano_load.device_image` uses for pianos).

    ASSUMED: sample files use the same device header; `try_load` shows that the generic loader
    rejects the body, so this is only the probe input.
    """
    head = bytearray(pl.DEVICE_HEADER)
    raw = name.encode("ascii", "replace")[:0x20]
    head[: len(raw)] = raw
    head[pl.TYPE_OFFSET : pl.TYPE_OFFSET + 4] = struct.pack("<I", type_word)
    return bytes(head) + data[MAGIC_OFFSET:]


@dataclass
class LoadResult:
    """Outcome of `0xC00C0444(manager, 1, entry)` for one served image."""

    status: int
    reads: list[tuple[int, int, int]]
    state: int | None


def try_load(rig: kd.Rig, image: bytes) -> LoadResult:
    """Serve `image` as entry 1 and run the real loader once (state 3/4: not bound, 0/2: ok)."""
    stub = pl.LibraryStub({1: image}, pl.KIND_HEADER_ONLY).install(rig.emu)
    managers = pl.managers(rig.emu)
    status = pl.load_sound(rig.emu, managers[0], 1)
    objects = pl.sound_objects(rig.emu)
    state = pl.sound_state(rig.emu, objects[0]) if objects else None
    return LoadResult(status, list(stub.reads), state)


@dataclass(frozen=True)
class Gate:
    """The fields that decide whether the synth voice event can start a voice of one layer."""

    layer: int
    obj: int
    slot_list: int
    first_slot: int
    enable: int
    status: int
    counters: tuple[int, ...]

    @property
    def open(self) -> bool:
        """A sample slot object is present (necessary, not sufficient)."""
        return self.first_slot != 0


def synth_gate(emu: ae.ArmEmu, layer: int) -> Gate:
    """Read the per-layer synth object of `layer` (0 when the table entry is empty)."""
    obj = emu.read32(SYNTH_LAYER_TABLE + 4 * layer)
    if not obj:
        return Gate(layer, 0, 0, 0, 0, 0, ())
    slot_list = emu.read32(obj + SLOT_LIST_OFFSET)
    return Gate(
        layer,
        obj,
        slot_list,
        emu.read32(slot_list) if slot_list else 0,
        emu.read32(obj + ENABLE_OFFSET),
        emu.read32(obj + STATUS_OFFSET) & 0xFF,
        tuple(emu.read32(COUNTER_BASE + off) for off in COUNTER_OFFSETS),
    )


# VERIFIED (run, section 11 of the research note): the sample loader. `0xC01D710C(manager,
# key_pair, skip_pair)` with `manager = [0xC055D9B0]` ([0] = its own device object, [4] = the pool
# of `0x9D8`-byte sample objects, class table `0xC02FB2C0`) loads library entry `key_pair.hw1`
# into the first free pool object and returns 0 with `[obj + 0xC] == 0` ("Sample is ok").
SAMPLE_MANAGER = 0xC055D9B0
SAMPLE_LOADER = 0xC01D710C
SAMPLE_BIND = 0xC01E5978  # (slot list, id): puts the pool object whose `[obj + 4] == id` in slot 0
# VERIFIED (disassembly of `0xC0095C0C`): the sample device header is `[dev + 0x6C] = 0x68` name
# bytes, the type word at `0x68` (`nsmp` = LE `0x6E736D70`, compared at `0xC01D73D4`) and two more
# words; the backup bytes from `NSMP` (`0x2C`) follow at `0x74`.
SAMPLE_HEADER = 0x74
SAMPLE_TYPE_OFFSET = 0x68
SAMPLE_TYPE_WORD = 0x6E736D70
# VERIFIED (run): directory vector fields the loader tests with `0xC00507A4` (lower bound on the
# u16 id at `entry + 4`) in the device directory `[dev + 0x60]`.
DIR_START, DIR_COUNT, DIR_VECTOR = 0x10, 0x14, 0x2C
# VERIFIED (run, `edit ids of docs/research/arm-edit-params.csv`): `Edt_Syn_kSmpId`, `Edt_Syn_kMode`
# (0 = SAMPLES, which sets the flag byte `[layer + 0x4BC]` at `0xC00171D4`).
EDIT_SMP_ID, EDIT_MODE = 0x147, 0xFA


def sample_device_image(data: bytes, name: str = "Sample") -> bytes:
    """Sample device file: `SAMPLE_HEADER` bytes (name, type word), then the backup from `NSMP`."""
    head = bytearray(SAMPLE_HEADER)
    raw = name.encode("ascii", "replace")[:0x68]
    head[: len(raw)] = raw
    head[SAMPLE_TYPE_OFFSET : SAMPLE_TYPE_OFFSET + 4] = struct.pack("<I", SAMPLE_TYPE_WORD)
    return bytes(head) + data[MAGIC_OFFSET:]


def sample_descriptor(volume: int, entry: int, length: int) -> bytes:
    """The `0x18`-byte directory descriptor `0xC0096264` fills: `+4` is the file length the sample
    loader subtracts `0xC + 0x68` from (the piano loader ignores it), `+0x10` the byte size."""
    return struct.pack("<HHIIIII", volume, entry, length, 0, 0, length, 0)


def directory_entry(blocks: int, entry: int) -> bytes:
    """Directory entry `0xC00961B0` reads: `u16 blocks`, `u16 +2`, `u16 id` at `+4`."""
    return struct.pack("<HHH", blocks, 0, entry)


def serve_sample(emu: ae.ArmEmu, image: bytes, entry: int = 1) -> pl.LibraryStub:
    """Install the file read, the directory lookup and one directory entry for `image`."""
    stub = pl.LibraryStub({entry: image}, 0).install(emu)

    def lookup(_emu: ae.ArmEmu) -> int:
        storage, wanted, out = emu.reg(0), emu.reg(1), emu.reg(2)
        volume = emu.read32(storage + 0xC) & 0xFFFF
        emu.write_bytes(out, sample_descriptor(volume, wanted, len(image)))
        return 0

    emu.add_stub(pl.DIR_LOOKUP, lookup)
    device = emu.read32(emu.read32(SAMPLE_MANAGER))
    directory = emu.read32(device + 0x60)
    block = max(1, emu.read32(device + 0x10))
    base = 0x90FE0000  # scratch RAM above the heap of the settled state
    emu.write32(base, base + 0x100)
    emu.write_bytes(
        base + 0x100, directory_entry(-(-len(image) // block), entry).ljust(0x40, b"\0")
    )
    emu.write32(emu.read32(directory + DIR_VECTOR), base)
    emu.write32(directory + DIR_START, 0)
    emu.write32(directory + DIR_COUNT, 1)
    return stub


def load_sample(emu: ae.ArmEmu, entry: int = 1) -> int:
    """Run the real sample loader for `entry` (after `serve_sample`); 0 = "Sample is ok"."""
    manager = emu.read32(SAMPLE_MANAGER)
    pair = 0x90FF0000
    emu.write_bytes(pair, struct.pack("<HHHH", 0, entry, 0xFFFF, 0xFFFF))
    return ast.call_preserving(emu, SAMPLE_LOADER, manager, pair, pair + 4, max_insns=20_000_000)


def bind_layer(emu: ae.ArmEmu, layer: int, sample_id: int = 1) -> int:
    """Put the loaded pool object `sample_id` in slot 0 of the layer's slot list; returns slot 0."""
    obj = emu.read32(SYNTH_LAYER_TABLE + 4 * layer)
    slot_list = emu.read32(obj + SLOT_LIST_OFFSET)
    ast.call_preserving(emu, SAMPLE_BIND, slot_list, sample_id, max_insns=20_000_000)
    return emu.read32(slot_list)


def survey(ns4b: Path) -> dict[str, int]:
    """Counts over every `Samp Lib/*.nsmp4`: files, valid headers, chunk tag sequences."""
    import zipfile

    counts: dict[str, int] = {"files": 0, "nsmp": 0, "named": 0}
    with zipfile.ZipFile(ns4b) as archive:
        for member in archive.namelist():
            if not member.startswith("Samp Lib/") or not member.endswith(".nsmp4"):
                continue
            counts["files"] += 1
            with archive.open(member) as handle:
                head = handle.read(0x400)
            counts["nsmp"] += is_nsmp(head)
            counts["named"] += bool(sample_name(head))
            seq = "-".join(c.tag for c in chunks(head))
            counts[seq] = counts.get(seq, 0) + 1
    return counts


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path, nargs="?")
    ap.add_argument("--cache", type=Path)
    ap.add_argument("--ns4b", type=Path, required=True)
    ap.add_argument("--member", required=True)
    ns = ap.parse_args(argv)
    print("survey", survey(ns.ns4b))
    data = pl.read_npno(ns.ns4b, ns.member, 0x10000)
    print("name", sample_name(data), [(c.tag, c.length) for c in chunks(data)])
    for label, word in LOADER_TYPE_WORDS.items():
        rig = kd.build_rig(ns.image, ns.sram, ns.cache)
        print(label, try_load(rig, device_image(data, word)))
    rig = kd.build_rig(ns.image, ns.sram, ns.cache)
    for layer in range(5):
        print(synth_gate(rig.emu, layer))
    rig = kd.build_rig(ns.image, ns.sram, ns.cache)
    serve_sample(rig.emu, sample_device_image(data, sample_name(data)))
    print("sample loader status", load_sample(rig.emu), "slot 0", hex(bind_layer(rig.emu, 0)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
