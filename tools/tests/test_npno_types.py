import io
import os
import sys
import zipfile

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import npno_types as t  # noqa: E402


def head(cat: int, tag: bytes = b"npno") -> bytes:
    return (
        b"CBIN"
        + (1).to_bytes(4, "little")
        + tag
        + cat.to_bytes(2, "little")
        + (7).to_bytes(2, "little")
        + bytes(32)
    )


def test_header_type_reads_offset_12_and_rejects_other_files():
    assert t.header_type(head(3)) == 3
    assert t.header_type(head(6)) is None
    assert t.header_type(head(1, b"nsmp")) is None
    assert t.header_type(b"CBIN") is None


def test_folder_type_handles_clavinet_and_unknown_folders():
    assert t.folder_type("Piano/Clavinet/x.npno") == 3
    assert t.folder_type("Piano/Misc/Marimba.npno") == 5
    assert t.folder_type("Piano/Other/x.npno") is None


def test_check_flags_a_header_that_disagrees_with_its_folder():
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as z:
        z.writestr("Piano/Grand/a.npno", head(0))
        z.writestr("Piano/Upright/b.npno", head(0))
        z.writestr("Other/c.ns4p", b"x")
    n, bad = t.check(zipfile.ZipFile(buf))
    assert n == 2 and bad == ["Piano/Upright/b.npno"]


def head_named(name: bytes) -> bytes:
    return head(0) + bytes(0x48 - len(head(0))) + name.ljust(32, b" ")


def test_header_size_reads_the_last_token_of_the_header_name():
    assert t.header_size(head_named(b"Pearl Upright#GRT 120   Lrg")) == "Lrg"
    assert t.header_size(head_named(b"White Grand#Stw B     XL")) == "XL"
    assert t.header_size(head_named(b"Marimba#Classic   ")) is None
    assert t.header_size(head(0)) is None


def test_entry_size_reads_the_token_of_the_file_name():
    assert t.entry_size("Piano/Grand/White Grand XL 6.3.npno") == "XL"
    assert t.entry_size("Piano/Misc/Marimba  6.1.npno") is None


def test_check_sizes_counts_and_flags_disagreements():
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as z:
        z.writestr("Piano/Grand/A Med 6.0.npno", head_named(b"A#x   Med"))
        z.writestr("Piano/Grand/B Lrg 6.0.npno", head_named(b"B#x   Sml"))
        z.writestr("Piano/Misc/C 6.0.npno", head_named(b"C#x"))
    n, sized, bad = t.check_sizes(zipfile.ZipFile(buf))
    assert (n, sized, bad) == (3, 2, ["Piano/Grand/B Lrg 6.0.npno"])
