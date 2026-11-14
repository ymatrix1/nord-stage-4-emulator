import io
import os
import sys
import zipfile

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import preset_check as t  # noqa: E402


def cbin(tag: bytes, size: int, schema: int, payload_fill: int = 7) -> bytes:
    """A well-formed CBIN file of `tag` with a valid checksum."""
    b = bytearray(size)
    b[0:4] = b"CBIN"
    b[4:8] = (1).to_bytes(4, "little")
    b[8:12] = tag
    b[20:24] = schema.to_bytes(4, "little")
    b[46:48] = schema.to_bytes(2, "big")
    b[48:] = bytes([payload_fill]) * (size - 48)
    b[24:28] = t.checksum(bytes(b)).to_bytes(4, "little")
    return bytes(b)


def organ() -> bytes:
    return cbin(b"ns4o", 183, 0x00CD)


def test_checksum_is_the_crc_of_the_bytes_from_0x2c():
    import zlib

    data = organ()
    assert t.checksum(data) == zlib.crc32(data[0x2C:])
    # bytes before 0x2C (bank, location, category) are outside the range
    other = bytearray(data)
    other[14] = 9
    assert t.checksum(bytes(other)) == t.checksum(data)


def test_a_valid_file_has_no_problems_and_other_files_are_skipped():
    assert t.check_file("a.ns4o", organ()) == []
    assert t.check_file("x", b"PK\x03\x04" + bytes(60)) is None
    assert t.check_file("x", b"short") is None


def test_wrong_size_checksum_schema_reserved_are_reported():
    assert "size" in t.check_file("a.ns4o", organ()[:-1])[0]
    bad = bytearray(organ())
    bad[48] ^= 1
    assert any("checksum" in p for p in t.check_file("a.ns4o", bytes(bad)))
    bad = bytearray(organ())
    bad[20] = 1
    assert any("schema id (LE)" in p for p in t.check_file("a.ns4o", bytes(bad)))
    bad = bytearray(organ())
    bad[30] = 1
    probs = t.check_file("a.ns4o", bytes(bad))
    assert any("reserved" in p for p in probs)


def test_check_counts_tags_over_an_archive():
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as z:
        z.writestr("Organ Preset/Bank 1/a.ns4o", organ())
        z.writestr("Piano Preset/Bank 1/b.ns4n", cbin(b"ns4n", 195, 0x00CB))
        z.writestr("meta.xml", b"<backup/>")
    counts, problems = t.check(zipfile.ZipFile(buf))
    assert counts == {"ns4o": 1, "ns4n": 1} and problems == []
    assert t.main(["preset_check.py"]) == 2
