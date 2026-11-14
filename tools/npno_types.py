"""Check the piano type of every `.npno` of an `.ns4b` backup.

The type of a model is the CBIN header `bank_index` at offset 12 (u16 LE; `location_index`, the
position inside the type, follows at offset 14), index order Grand 0, Upright 1,
Electric 2, Clav 3, Digital 4, Misc 5 (same as the `.ns4p` piano layer type field). This script
compares it with the archive folder `Piano/<Folder>/`, which the GUI uses to list models without
reading the multi-megabyte entries.

It also checks the size letter: the header name at offset 0x48 (32 bytes, `<model>#<detail> <size>`)
ends with the size token Sml / Med / Lrg / XL for the Grand, Upright and Digital models and some
Electric ones (manual p.24 "About Piano size"), and it equals the token in the entry file name.

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


SIZES = ("Sml", "Med", "Lrg", "XL")
NUL = chr(0)


def header_size(head: bytes):
    """Size token (`Sml`, `Med`, `Lrg`, `XL`) ending the header name at 0x48, or None."""
    if len(head) < 0x68 or head[0:4] != b"CBIN" or head[8:12] != b"npno":
        return None
    words = head[0x48:0x68].decode("latin1").split(NUL)[0].split()
    return words[-1] if words and words[-1] in SIZES else None


def entry_size(entry: str):
    """Size token of an entry file name (`White Grand XL 6.3.npno`), or None."""
    stem = entry.replace("\\", "/").split("/")[-1].rsplit(".", 1)[0]
    return next((w for w in stem.split() if w in SIZES), None)


def check_sizes(zf) -> tuple[int, int, list[str]]:
    """Returns (npno count, count with a size, entries whose header and file name disagree)."""
    n = sized = 0
    bad = []
    for info in zf.infolist():
        if not info.filename.lower().endswith(".npno"):
            continue
        n += 1
        with zf.open(info) as f:
            head = f.read(0x68)
        h = header_size(head)
        sized += h is not None
        if h != entry_size(info.filename):
            bad.append(info.filename)
    return n, sized, bad


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
        _, sized, bad_sizes = check_sizes(zf)
    for b in bad:
        print("MISMATCH", b)
    for b in bad_sizes:
        print("SIZE MISMATCH", b)
    print(f"{n} npno files; header type vs folder mismatches: {len(bad)}")
    print(f"{sized} with a size token; header vs file name size mismatches: {len(bad_sizes)}")
    return 1 if bad or bad_sizes else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
