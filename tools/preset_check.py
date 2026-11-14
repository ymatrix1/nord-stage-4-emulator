"""Check the CBIN files of an `.ns4b` backup against the format facts the preset code relies on.

Every `.ns4p` / `.ns4l` / `.ns4o` / `.ns4n` / `.ns4y` / `.ns4t` entry is checked for:

- total size and schema id (little-endian at 0x14, big-endian repeat at 0x2E) for its tag,
- the header checksum: CRC-32 (zlib) of the file bytes from 0x2C to the end, stored at 0x18,
- for presets, that the payload size equals the section block of a program (Organ 135, Piano 147,
  Synth 493 bytes), and that the 18 reserved bytes at 0x1C are zero.

Rust side: `nord_core::cbin::checksum`, `cbin::section_preset`. The decode and bit-exact re-encode
check lives in `ns4 preset-check <backup>`. Local-only data: the backup stays under `.reference/`.

Usage: python tools/preset_check.py "Nord Stage 4 Factory Restore RevK v1.66.ns4b"
"""

import sys
import zipfile
import zlib
from collections import Counter

CHECKSUM_START = 0x2C
# tag -> (total file size, schema id)
KINDS = {
    b"ns4p": (868, 0x0139),
    b"ns4l": (868, 0x0139),
    b"ns4y": (541, 0x00D0),
    b"ns4n": (195, 0x00CB),
    b"ns4o": (183, 0x00CD),
    b"ns4t": (124, 0x006C),
}
# Section blocks of a program (`.ns4p` payload 45 + 135 + 147 + 493 = 820 bytes).
PRESET_BLOCK = {b"ns4o": 135, b"ns4n": 147, b"ns4y": 493}


def checksum(data: bytes) -> int:
    """Header checksum of a CBIN file: CRC-32 of the bytes from 0x2C."""
    return zlib.crc32(data[CHECKSUM_START:]) & 0xFFFFFFFF


def check_file(name: str, data: bytes):
    """Problems of one CBIN file (empty list when it is as expected); None for other tags."""
    tag = data[8:12]
    if data[0:4] != b"CBIN" or tag not in KINDS:
        return None
    size, schema = KINDS[tag]
    problems = []
    if len(data) != size:
        return [f"size {len(data)} != {size}: {name}"]
    if int.from_bytes(data[20:24], "little") != schema:
        problems.append(f"schema id (LE) is not {schema:#06x}: {name}")
    if int.from_bytes(data[46:48], "big") != schema:
        problems.append(f"schema id (BE) is not {schema:#06x}: {name}")
    if int.from_bytes(data[24:28], "little") != checksum(data):
        problems.append(f"checksum is not the CRC-32 of bytes 0x2C..: {name}")
    if any(data[28:46]):
        problems.append(f"reserved bytes are not zero: {name}")
    if tag in PRESET_BLOCK and len(data) - 48 != PRESET_BLOCK[tag]:
        problems.append(f"payload is not the program section block: {name}")
    return problems


def check(zf):
    """Returns (files checked per tag, problems) for every CBIN entry of the archive."""
    counts = Counter()
    problems = []
    for info in zf.infolist():
        with zf.open(info) as f:
            head = f.read(12)
            if head[0:4] != b"CBIN" or head[8:12] not in KINDS:
                continue
            data = head + f.read()
        found = check_file(info.filename, data)
        if found is None:
            continue
        counts[head[8:12].decode()] += 1
        problems.extend(found)
    return counts, problems


def main(argv) -> int:
    if len(argv) < 2:
        print(__doc__)
        return 2
    with zipfile.ZipFile(argv[1]) as zf:
        counts, problems = check(zf)
    for tag, n in sorted(counts.items()):
        print(f"{tag}: {n} files")
    for p in problems:
        print("PROBLEM", p)
    print(f"{sum(counts.values())} CBIN files; problems: {len(problems)}")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
