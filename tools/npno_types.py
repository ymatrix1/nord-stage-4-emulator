"""Check the piano type of every `.npno` of an `.ns4b` backup.

The type of a model is the CBIN header `bank_index` at offset 12 (u16 LE; `location_index`, the
position inside the type, follows at offset 14), index order Grand 0, Upright 1,
Electric 2, Clav 3, Digital 4, Misc 5 (same as the `.ns4p` piano layer type field). This script
compares it with the archive folder `Piano/<Folder>/`, which the GUI uses to list models without
reading the multi-megabyte entries.

Usage: python tools/npno_types.py "Nord Stage 4 Factory Restore RevK v1.66.ns4b"
"""

import sys
import zipfile

TYPES = ["Grand", "Upright", "Electric", "Clav", "Digital", "Misc"]
FOLDERS = {
    "Grand": 0,
    "Upright": 1,
    "Electric": 2,
    "Clavinet": 3,
    "Clav": 3,
    "Digital": 4,
    "Misc": 5,
}


def header_type(head: bytes):
    """Type index from the first 16 bytes of a `.npno`, or None when it is not one."""
    if len(head) < 16 or head[0:4] != b"CBIN" or head[8:12] != b"npno":
        return None
    idx = int.from_bytes(head[12:14], "little")
    return idx if idx < len(TYPES) else None


def folder_type(entry: str):
    """Type index from the folder under `Piano/` of an archive entry name, or None."""
    for part in reversed(entry.replace("\\", "/").split("/")[:-1]):
        if part in FOLDERS:
            return FOLDERS[part]
    return None


def check(zf) -> tuple[int, list[str]]:
    """Returns (npno count, mismatching entry names)."""
    n = 0
    bad = []
    for info in zf.infolist():
        if not info.filename.lower().endswith(".npno"):
            continue
        n += 1
        with zf.open(info) as f:
            head = f.read(16)
        if header_type(head) != folder_type(info.filename):
            bad.append(info.filename)
    return n, bad


def main(argv) -> int:
    with zipfile.ZipFile(argv[1]) as zf:
        n, bad = check(zf)
    for b in bad:
        print("MISMATCH", b)
    print(f"{n} npno files; header type vs folder mismatches: {len(bad)}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
