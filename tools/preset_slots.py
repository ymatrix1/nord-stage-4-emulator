"""List the Preset Library slots of an `.ns4b` backup and check them.

Every preset entry (`.ns4o` Organ, `.ns4n` Piano, `.ns4y` Synth) is a CBIN file whose header holds
`bank_index` (u16 LE at offset 12, 0-based) and `location_index` (u16 LE at offset 14, `0..63`).
This script counts the presets per section and bank, reports duplicate (bank, location) pairs and
locations outside `0..63`, and checks that the entry folder is `Bank <bank + 1>`. Rust side:
`nord_core::cbin::backup_presets` (Preset Organize writes these headers).

Usage: python tools/preset_slots.py "Nord Stage 4 Factory Restore RevK v1.66.ns4b"
"""

import sys
import zipfile
from collections import Counter

EXTS = {".ns4o": "Organ", ".ns4n": "Piano", ".ns4y": "Synth"}


def section_of(entry: str):
    """Section name of a preset entry by extension, or None."""
    low = entry.lower()
    return next((s for e, s in EXTS.items() if low.endswith(e)), None)


def header_slot(head: bytes):
    """(bank, location) of the first 16 bytes of a CBIN file, or None."""
    if len(head) < 16 or head[0:4] != b"CBIN":
        return None
    return int.from_bytes(head[12:14], "little"), int.from_bytes(head[14:16], "little")


def folder_bank(entry: str):
    """Bank number (1-based) of the `Bank N` folder in an entry path, or None."""
    for part in entry.replace("\\", "/").split("/")[:-1]:
        if part.startswith("Bank ") and part[5:].isdigit():
            return int(part[5:])
    return None


def check(zf):
    """Returns (counts per (section, bank), problems)."""
    counts = Counter()
    seen = {}
    problems = []
    for info in zf.infolist():
        section = section_of(info.filename)
        if section is None:
            continue
        with zf.open(info) as f:
            slot = header_slot(f.read(16))
        if slot is None:
            problems.append(f"no CBIN header: {info.filename}")
            continue
        bank, loc = slot
        counts[(section, bank)] += 1
        if loc > 63:
            problems.append(f"location {loc} out of range: {info.filename}")
        if folder_bank(info.filename) != bank + 1:
            problems.append(f"folder does not match bank {bank}: {info.filename}")
        key = (section, bank, loc)
        if key in seen:
            problems.append(f"duplicate {key}: {info.filename} and {seen[key]}")
        seen[key] = info.filename
    return counts, problems


def main(argv) -> int:
    with zipfile.ZipFile(argv[1]) as zf:
        counts, problems = check(zf)
    for (section, bank), n in sorted(counts.items()):
        print(f"{section} bank {bank}: {n} presets")
    for p in problems:
        print("PROBLEM", p)
    print(f"{sum(counts.values())} presets; problems: {len(problems)}")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
