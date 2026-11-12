#!/usr/bin/env python3
"""Parse the loader block stream of an exported SHARC DSP image (FX, PNO or ORG).

Format found in `docs/research/dsp-image-hunt.md` 5.17 (all little endian). The image is a sequence
of blocks, each a 16-byte header `tag, address, count, aux` plus a payload for data blocks:

    tag byte 3  XOR of the other 15 header bytes (checked on every block of v1.66 FX, PNO, ORG)
    tag low 16  0x5003 core section header: address = entry value, aux = bytes in the section
                0x0003 data block: `count` payload bytes follow, loaded at `address`
                0x0103 zero-fill block: `count` bytes of zeros at `address`, no payload
                0x8003 final block of a core section: address = entry value, count 0

Each image holds two core sections (Core1, Core2). SY1SY2 and SMP have no such header.

    python tools/ns4_dsp_ldr.py dsp_image_0_FX.bin
    python tools/ns4_dsp_ldr.py dsp_image_0_FX.bin --blocks
    python tools/ns4_dsp_ldr.py dsp_image_0_FX.bin --out dir   # data segments (do not commit)
    python tools/ns4_dsp_ldr.py dsp_image_0_FX.bin --sram dir  # flat SRAM + L2 image per core

Address mapping (docs 5.18, VERIFIED on all 24 core sections of FX, PNO, ORG): internal blocks have
top byte 0x28; `short-word address = (address & 0x7FFFFF) / 2` in the ADSP-21489 short-word space,
and bit 23 of the address is the chip (0 for Core1, 1 for Core2). The section entry value is a
short-word address. The flat SRAM image is indexed by `address & 0x7FFFFF`, so a byte offset
`2 * short_word_address` addresses it directly (usable with `tools/sharc_visa.py`).
"""

from __future__ import annotations

import argparse
import struct
import sys
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path

HEADER = 16
SECTION, DATA, ZERO, FINAL = 0x5003, 0x0003, 0x0103, 0x8003
KIND_NAMES = {SECTION: "section", DATA: "data", ZERO: "zero", FINAL: "final"}


@dataclass(frozen=True)
class Block:
    offset: int  # byte offset of the header in the image
    kind: int  # tag & 0xFFFF
    address: int
    count: int
    aux: int
    checksum_ok: bool

    @property
    def payload_size(self) -> int:
        return self.count if self.kind == DATA else 0

    @property
    def next_offset(self) -> int:
        return self.offset + HEADER + self.payload_size


def header_checksum(header: bytes) -> int:
    """XOR of the 15 header bytes other than tag byte 3."""
    x = 0
    for i, b in enumerate(header[:HEADER]):
        if i != 3:
            x ^= b
    return x


def parse_blocks(img: bytes) -> tuple[list[Block], int]:
    """Blocks of the stream and the offset where parsing stopped (== len(img) when complete)."""
    blocks: list[Block] = []
    o = 0
    while o + HEADER <= len(img):
        tag, address, count, aux = struct.unpack_from("<4I", img, o)
        kind = tag & 0xFFFF
        if kind not in KIND_NAMES:
            break
        block = Block(
            o, kind, address, count, aux, header_checksum(img[o : o + HEADER]) == tag >> 24
        )
        if block.next_offset > len(img):
            break
        blocks.append(block)
        o = block.next_offset
    return blocks, o


def split_cores(blocks: list[Block]) -> list[list[Block]]:
    """Group blocks into core sections, each starting at a `section` header block."""
    cores: list[list[Block]] = []
    for b in blocks:
        if b.kind == SECTION:
            cores.append([])
        if cores:
            cores[-1].append(b)
    return cores


def load_map(core: list[Block]) -> dict[int, tuple[int, int, int, int, int]]:
    """Per top address byte: `(data blocks, data bytes, zero blocks, zero bytes, ...)` plus range.

    Returns `{top byte: (n_data, data_bytes, n_zero, zero_bytes, low)}`; `address_range` gives ends.
    """
    acc: dict[int, list[int]] = defaultdict(lambda: [0, 0, 0, 0, 0xFFFFFFFF])
    for b in core:
        if b.kind in (DATA, ZERO):
            r = acc[b.address >> 24]
            if b.kind == DATA:
                r[0] += 1
                r[1] += b.count
            else:
                r[2] += 1
                r[3] += b.count
            r[4] = min(r[4], b.address)
    return {k: (v[0], v[1], v[2], v[3], v[4]) for k, v in acc.items()}


def address_range(core: list[Block], top: int) -> tuple[int, int]:
    """Lowest address and highest end address of data and zero blocks with top byte `top`."""
    spans = [
        (b.address, b.address + b.count)
        for b in core
        if b.kind in (DATA, ZERO) and b.address >> 24 == top
    ]
    return min(a for a, _ in spans), max(e for _, e in spans)


INTERNAL_TOP = 0x28
SRAM_BYTES = 0x400000  # address & 0x7FFFFF stays below 0x3A0000 in v1.66


def chip_of(address: int) -> int:
    """Chip index encoded in bit 23 of a loader address (0 = Core1, 1 = Core2)."""
    return (address >> 23) & 1


def short_word_range(address: int, count: int) -> tuple[int, int]:
    """First and last short-word address (inclusive) of `count` bytes at `address`."""
    start = address & 0x7FFFFF
    return start // 2, (start + count) // 2 - 1


def sram_image(img: bytes, core: list[Block]) -> bytearray:
    """Flat memory image of the internal blocks (top byte 0x28) of one core, zeros elsewhere.

    Index = `address & 0x7FFFFF`; the short-word address of byte `i` is `i // 2`.
    """
    mem = bytearray(SRAM_BYTES)
    for b in core:
        if b.kind == DATA and b.address >> 24 == INTERNAL_TOP:
            lo = b.address & 0x7FFFFF
            mem[lo : lo + b.count] = img[b.offset + HEADER : b.offset + HEADER + b.count]
    return mem


L2_TOP, L2_BASE, L2_BYTES = 0x20, 0x20000000, 0x100000  # 1 MiB L2 of the ADSP-2157x


def l2_image(img: bytes, core: list[Block]) -> bytearray:
    """Flat 1 MiB image of the L2 data blocks (top byte 0x20), indexed `address - 0x20000000`."""
    mem = bytearray(L2_BYTES)
    for b in core:
        if b.kind == DATA and b.address >> 24 == L2_TOP:
            lo = b.address - L2_BASE
            mem[lo : lo + b.count] = img[b.offset + HEADER : b.offset + HEADER + b.count]
    return mem


def segments(img: bytes, core: list[Block]) -> list[tuple[int, bytes]]:
    """`(address, bytes)` for every data block of a core, in stream order."""
    return [
        (b.address, img[b.offset + HEADER : b.offset + HEADER + b.count])
        for b in core
        if b.kind == DATA
    ]


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("image", type=Path)
    p.add_argument("--blocks", action="store_true", help="list every block")
    p.add_argument("--out", type=Path, help="write each core's data segments here")
    p.add_argument("--sram", type=Path, help="write a flat SRAM image per core here")
    args = p.parse_args(argv)
    img = args.image.read_bytes()
    blocks, end = parse_blocks(img)
    if not blocks or end != len(img):
        print(
            f"not a loader stream (parsed {len(blocks)} blocks, "
            f"stopped at 0x{end:x} of 0x{len(img):x})"
        )
        return 1
    bad = sum(not b.checksum_ok for b in blocks)
    cores = split_cores(blocks)
    print(f"{len(blocks)} blocks, {len(cores)} core sections, header checksum failures: {bad}")
    for ci, core in enumerate(cores, 1):
        entry = core[0].address
        print(
            f"core {ci}: section at 0x{core[0].offset:x}, entry value 0x{entry:x}, "
            f"{core[0].aux} bytes"
        )
        for top, (nd, db, nz, zb, _low) in sorted(load_map(core).items()):
            lo, hi = address_range(core, top)
            print(
                f"  0x{top:02x}xxxxxx: {nd} data blocks ({db} B), {nz} zero blocks ({zb} B), "
                f"0x{lo:08x}..0x{hi:08x}"
            )
        if args.blocks:
            for b in core:
                print(
                    f"    0x{b.offset:06x} {KIND_NAMES[b.kind]:7s} "
                    f"addr 0x{b.address:08x} count {b.count}"
                )
        if args.sram:
            args.sram.mkdir(parents=True, exist_ok=True)
            (args.sram / f"core{ci}_sram.bin").write_bytes(sram_image(img, core))
            (args.sram / f"core{ci}_l2.bin").write_bytes(l2_image(img, core))
        if args.out:
            args.out.mkdir(parents=True, exist_ok=True)
            for i, (addr, data) in enumerate(segments(img, core)):
                (args.out / f"core{ci}_{i:03d}_{addr:08x}.bin").write_bytes(data)
    return 0


if __name__ == "__main__":
    sys.exit(main())
