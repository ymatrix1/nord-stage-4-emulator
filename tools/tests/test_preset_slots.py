import io
import os
import sys
import zipfile

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import preset_slots as t  # noqa: E402


def head(bank: int, loc: int) -> bytes:
    return (
        b"CBIN"
        + (1).to_bytes(4, "little")
        + b"ns4o"
        + bank.to_bytes(2, "little")
        + loc.to_bytes(2, "little")
        + bytes(32)
    )


def test_section_and_header_slot_and_folder_bank():
    assert t.section_of("Synth Preset/Bank 1/a.ns4y") == "Synth"
    assert t.section_of("Program/Bank A/a.ns4p") is None
    assert t.header_slot(head(1, 9)) == (1, 9)
    assert t.header_slot(b"nope") is None
    assert t.folder_bank("Piano Preset/Bank 2/x.ns4n") == 2
    assert t.folder_bank("x.ns4n") is None


def zipped(entries):
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as z:
        for name, data in entries:
            z.writestr(name, data)
    return zipfile.ZipFile(buf)


def test_check_counts_per_bank_and_reports_no_problem_for_a_clean_archive():
    z = zipped(
        [
            ("Organ Preset/Bank 1/a.ns4o", head(0, 0)),
            ("Organ Preset/Bank 1/b.ns4o", head(0, 1)),
            ("Organ Preset/Bank 2/c.ns4o", head(1, 0)),
            ("Other/x.ns4p", b"x"),
        ]
    )
    counts, problems = t.check(z)
    assert counts[("Organ", 0)] == 2 and counts[("Organ", 1)] == 1
    assert problems == []


def test_check_flags_duplicates_range_and_folder_mismatches():
    z = zipped(
        [
            ("Organ Preset/Bank 1/a.ns4o", head(0, 0)),
            ("Organ Preset/Bank 1/b.ns4o", head(0, 0)),
            ("Organ Preset/Bank 1/c.ns4o", head(0, 70)),
            ("Organ Preset/Bank 3/d.ns4o", head(0, 5)),
            ("Organ Preset/Bank 1/e.ns4o", b"short"),
        ]
    )
    _, problems = t.check(z)
    text = " | ".join(problems)
    assert "duplicate" in text and "out of range" in text
    assert "folder does not match" in text and "no CBIN header" in text
