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
