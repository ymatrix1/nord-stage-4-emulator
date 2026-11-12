#!/usr/bin/env python3
"""Parse a raw DSP image (SY1SY2, SMP) that is not an ADI loader stream of the FX/PNO/ORG kind.

Layout found in `docs/research/sharc-sy1sy2-smp.md` (all little endian, VERIFIED by walking every
byte of both v1.66 images):

    0x000  boot kernel, 256 words in ISA mode (fixed 48-bit words, least significant parcel
           first) for 48-bit address 0x8C000; byte-identical in SY1SY2 and SMP up to 0x6B2
    0x600  block stream, 12-byte headers {tag, count, address}, each block aligned to 4 bytes:
           tag 1  zero fill, `count` units at `address`, no payload
           tag 3  VISA code, `count` 16-bit short words at short-word `address`
           tag 4  data, `count` 32-bit words at normal-word (DM) address `address`
    end-0x60C  tail: 12-byte header, then the 256-word interrupt vector table (reset slot = word 5)

Flat SRAM image: index `2 * short_word` for code and `4 * dm_word` for data (the same memory,
see `tools/ns4_dsp_ldr.py`), table at short-word 0x120000.

    python tools/ns4_dsp_raw.py dsp_image_3_SY1SY2.bin
    python tools/ns4_dsp_raw.py dsp_image_3_SY1SY2.bin --sram sy.bin --kernel kernel.bin
"""

from __future__ import annotations

import argparse
import struct
import sys
from dataclasses import dataclass
from pathlib import Path

KERNEL_BYTES = 0x600
ISA_BASE = 0x8C000
# bytes 30..35 hold the reset call `call 0x8c091 (db)` = word 0x06BE_0408_C091, stored LS first
RESET_CALL = bytes.fromhex("91c00804be06")
RESET_OFFSET = 30
ZERO, CODE, DATA = 1, 3, 4
UNIT_BYTES = {ZERO: 0, CODE: 2, DATA: 4}
HEADER = 12
SRAM_BYTES = 0x40_0000
IVT_SHORT = 0x12_0000
IVT_BYTES = 0x600
RESET_SLOT = 5


@dataclass(frozen=True)
class Block:
    offset: int  # byte offset of the header
    tag: int
    count: int
    address: int

    @property
    def payload(self) -> int:
        return self.count * UNIT_BYTES[self.tag]

    @property
    def next_offset(self) -> int:
        return (self.offset + HEADER + self.payload + 3) & ~3


def is_raw_image(img: bytes) -> bool:
    """Whether `img` starts with the ISA boot kernel (five zero words, then the reset call)."""
    return (
        len(img) >= KERNEL_BYTES
        and not any(img[:RESET_OFFSET])
        and img[RESET_OFFSET : RESET_OFFSET + 6] == RESET_CALL
    )


def isa_words(img: bytes, start: int = 0, end: int = KERNEL_BYTES) -> list[int]:
    """48-bit ISA words of `img[start:end]` (6 bytes each, little endian)."""
    return [int.from_bytes(img[o : o + 6], "little") for o in range(start, end - 5, 6)]


def parse_blocks(img: bytes) -> tuple[list[Block], int]:
    """Blocks of the stream after the kernel and the offset where parsing stopped."""
    blocks: list[Block] = []
    o = KERNEL_BYTES
    while o + HEADER <= len(img):
        tag, count, address = struct.unpack_from("<3I", img, o)
        if tag not in UNIT_BYTES:
            break
        block = Block(o, tag, count, address)
        if block.offset + HEADER + block.payload > len(img):
            break
        blocks.append(block)
        o = block.next_offset
    return blocks, o


def tail(img: bytes) -> bytes:
    """The vector table after the block stream (header stripped), empty when absent."""
    _, end = parse_blocks(img)
    return img[end + HEADER : end + HEADER + IVT_BYTES]


def reset_target(img: bytes) -> int | None:
    """Target of the `JUMP` in reset slot 5 of the vector table (the program entry)."""
    t = tail(img)
    if len(t) < 6 * (RESET_SLOT + 1):
        return None
    word = int.from_bytes(t[6 * RESET_SLOT : 6 * RESET_SLOT + 6], "little")
    return word & 0xFFFFFF if word >> 40 == 0x06 else None


def sram_image(img: bytes) -> bytes:
    """Flat SRAM image of the block stream plus the vector table."""
    mem = bytearray(SRAM_BYTES)
    for b in parse_blocks(img)[0]:
        scale = 2 if b.tag == CODE else 4 if b.tag == DATA else 4 if b.address < 0x100000 else 2
        start = b.address * scale
        size = b.payload if b.tag != ZERO else b.count * scale
        if start + size > SRAM_BYTES:
            continue
        if b.tag == ZERO:
            mem[start : start + size] = bytes(size)
        else:
            mem[start : start + size] = img[b.offset + HEADER : b.offset + HEADER + size]
    ivt = tail(img)
    mem[2 * IVT_SHORT : 2 * IVT_SHORT + len(ivt)] = ivt
    return bytes(mem)


def common_prefix(a: bytes, b: bytes) -> int:
    n = 0
    while n < min(len(a), len(b)) and a[n] == b[n]:
        n += 1
    return n


def summary(img: bytes) -> str:
    blocks, end = parse_blocks(img)
    kinds = {t: [b for b in blocks if b.tag == t] for t in UNIT_BYTES}
    code = kinds[CODE]
    lines = [
        f"{len(img)} bytes; ISA boot kernel at 48-bit address 0x{ISA_BASE:X}, reset call ok",
        f"{len(blocks)} blocks to 0x{end:X}: "
        + ", ".join(f"{len(v)} x tag {t}" for t, v in kinds.items()),
    ]
    if code:
        lines.append(
            f"code 0x{min(b.address for b in code):X}..0x{max(b.address + b.count for b in code):X}"
            f" ({sum(b.count for b in code)} short words in {len(code)} blocks)"
        )
    dm = [b for b in blocks if b.tag in (ZERO, DATA) and b.address < 0x100000]
    if dm:
        lines.append(
            f"data 0x{min(b.address for b in dm):X}..0x{max(b.address + b.count for b in dm):X}"
            f" ({sum(b.count for b in dm if b.tag == DATA)} words, "
            f"{sum(b.count for b in dm if b.tag == ZERO)} zero-filled)"
        )
    target = reset_target(img)
    lines.append(
        f"tail {len(img) - end} bytes; reset vector jumps to " + (hex(target) if target else "?")
    )
    return "\n".join(lines)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("image")
    ap.add_argument("--other", help="second image to compare the common prefix with")
    ap.add_argument("--kernel", help="write the 0x600-byte boot kernel to this file")
    ap.add_argument("--sram", help="write the flat SRAM image to this file")
    args = ap.parse_args()
    img = Path(args.image).read_bytes()
    if not is_raw_image(img):
        print("not a raw ISA-kernel image")
        return 1
    print(summary(img))
    if args.other:
        n = common_prefix(img, Path(args.other).read_bytes())
        print(f"common prefix with {args.other}: 0x{n:X} bytes")
    if args.kernel:
        Path(args.kernel).write_bytes(img[:KERNEL_BYTES])
    if args.sram:
        Path(args.sram).write_bytes(sram_image(img))
    return 0


if __name__ == "__main__":
    sys.exit(main())
