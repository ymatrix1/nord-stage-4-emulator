#!/usr/bin/env python3
"""Disassemble raw SHARC (ADSP-214xx) VISA parcels with ADI's own `elfdump` (CCES 2.12.1).

The Analog Devices disassembler is the ground truth for the instruction layouts of this project
(`docs/research/dsp-image-hunt.md` 5.34). It needs no licence. The tool wraps parcels in a minimal
ELF relocatable object (one code section `seg_pmco`, ADSP-21469 attributes), runs
`elfdump -ns seg_pmco` and parses the listing.

    python tools/adi_dis.py IMAGE BYTE_START BYTE_END [--cces DIR] [--isa]

`IMAGE` is a flat SRAM image (`tools/ns4_dsp_ldr.py --sram`, 16-bit little-endian parcels);
the output has one line per instruction: `short_word_address  parcels  text`. The CCES folder
comes from `--cces` or the `NS4_CCES` environment variable, default
`C:/Analog Devices/CrossCore Embedded Studio 2.12.1`.
"""

import argparse
import os
import re
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

DEFAULT_CCES = "C:/Analog Devices/CrossCore Embedded Studio 2.12.1"
EM_SHARC = 0x85
SHT_PROGBITS, SHT_SYMTAB, SHT_STRTAB = 1, 2, 3
SHT_ADI_ALIGN, SHT_ADI_ATTR = 0x70000000, 0x70000002
SHF_ALLOC_EXEC = 6
# `.adi.attributes` of an ADSP-21469 object (a fixed 49 byte tag block naming the processor).
ADI_ATTRIBUTES = bytes.fromhex(
    "4130000000416e6f6e41444900011b00000004414453502d"
    "3231343639000505061307ffff0308010209000000030012"
    "03"
)
# `.align.seg_pmco` record of a VISA code section (copied from an ADI library object).
ALIGN_RECORD = bytes.fromhex("0000000000000000c0001000")
LINE = re.compile(r"^([0-9a-f]{8})\s{3}((?:[0-9a-f]{4} ?|[0-9a-f]{12} ?)+)\s{2,}(.*?);?\s*$")


def build_elf(parcels: list[int], visa: bool = True) -> bytes:
    """A relocatable ELF32 object whose `seg_pmco` section holds `parcels` (big-endian parcels,
    first parcel most significant, which is how the ADI tools store VISA code).

    Besides the code, `elfdump` needs the `.align.seg_pmco` record (it marks the section as VISA
    code, so addresses count parcels and instructions are 16, 32 or 48 bits wide) and the
    `.adi.attributes` block (it selects the processor). Without the align record it decodes
    fixed 48-bit ISA words (`visa=False` omits the record, for ISA-mode images such as SY1SY2/SMP).
    """
    code = b"".join(struct.pack(">H", p) for p in parcels)
    names = [".strtab", ".symtab", "seg_pmco", ".align.seg_pmco", ".adi.attributes"]
    strtab = b"\0"
    name = {}
    for n in names:
        name[n] = len(strtab)
        strtab += n.encode() + b"\0"
    symtab = bytes(16)  # the null symbol
    # (name, type, flags, data, link, info, align, entsize) per section after the null one
    blobs = [
        (".strtab", SHT_STRTAB, 0, strtab, 0, 0, 1, 0),
        (".symtab", SHT_SYMTAB, 0, symtab, 1, 1, 1, 16),
        ("seg_pmco", SHT_PROGBITS, SHF_ALLOC_EXEC, code, 0, 0, 1, 1),
        (".align.seg_pmco", SHT_ADI_ALIGN, 0, ALIGN_RECORD, 2, 3, 1, 12),
        (".adi.attributes", SHT_ADI_ATTR, 0, ADI_ATTRIBUTES, 0, 0, 1, 0),
    ]
    if not visa:
        blobs = [b for b in blobs if b[0] != ".align.seg_pmco"]
    ehsize, shentsize = 52, 40
    body = bytearray()
    sections = [(0,) * 10]
    for sname, stype, sflags, data, link, info, align, ent in blobs:
        while (ehsize + len(body)) % 4:
            body.append(0)
        offset = ehsize + len(body)
        body += data
        sections.append((name[sname], stype, sflags, 0, offset, len(data), link, info, align, ent))
    while (ehsize + len(body)) % 4:
        body.append(0)
    shoff = ehsize + len(body)
    ident = b"\x7fELF" + bytes([1, 1, 1]) + bytes(9)
    header = struct.pack(
        "<16sHHIIIIIHHHHHH",
        ident,
        1,
        EM_SHARC,
        1,
        0,
        0,
        shoff,
        0,
        ehsize,
        0,
        0,
        shentsize,
        len(sections),
        1,
    )
    out = bytearray(header) + body
    for s in sections:
        out += struct.pack("<10I", *s)
    return bytes(out)


def parse_listing(text: str) -> dict[int, tuple[list[int], str]]:
    """`elfdump -ns` output -> `{short_word_address: (parcels, text)}`."""
    result = {}
    for line in text.splitlines():
        m = LINE.match(line)
        if m:
            digits = m.group(2).replace(" ", "")
            parcels = [int(digits[i : i + 4], 16) for i in range(0, len(digits), 4)]
            result[int(m.group(1), 16)] = (parcels, m.group(3).strip())
    return result


def disassemble(
    parcels: list[int], cces: str | None = None, visa: bool = True
) -> dict[int, tuple[list[int], str]]:
    """Disassemble `parcels` with ADI's `elfdump`; addresses count parcels from 0."""
    folder = Path(cces or os.environ.get("NS4_CCES") or DEFAULT_CCES)
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "code.doj"
        path.write_bytes(build_elf(parcels, visa))
        run = subprocess.run(
            [str(folder / "elfdump.exe"), "-ns", "seg_pmco", str(path)],
            capture_output=True,
            text=True,
            check=False,
        )
    return parse_listing(run.stdout)


def image_parcels(image: bytes, start: int, end: int) -> list[int]:
    """The little-endian parcels of a flat SRAM image between two byte offsets."""
    raw = image[start:end]
    return list(struct.unpack(f"<{len(raw) // 2}H", raw[: len(raw) // 2 * 2]))


def isa_parcels(image: bytes, start: int, end: int) -> list[int]:
    """Parcels of ISA-mode words (6 bytes each, least significant parcel first in the file),
    reordered most significant first as `build_elf` expects."""
    out: list[int] = []
    for o in range(start, end - 5, 6):
        lo, mid, hi = struct.unpack_from("<3H", image, o)
        out += [hi, mid, lo]
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("image")
    ap.add_argument("start", type=lambda s: int(s, 0))
    ap.add_argument("end", type=lambda s: int(s, 0))
    ap.add_argument("--cces", default=None)
    ap.add_argument("--isa", action="store_true", help="48-bit ISA words (SY1SY2/SMP kernel)")
    args = ap.parse_args()
    data = Path(args.image).read_bytes()
    if args.isa:
        listing = disassemble(isa_parcels(data, args.start, args.end), args.cces, visa=False)
    else:
        listing = disassemble(image_parcels(data, args.start, args.end), args.cces)
    for addr, (par, text) in sorted(listing.items()):
        print(f"{addr:08x}  {' '.join(f'{p:04x}' for p in par)}  {text}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
