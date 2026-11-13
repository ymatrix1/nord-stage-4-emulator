"""Unit tests for tools/arm_piano_load.py (stubs and layout; no firmware needed)."""

from __future__ import annotations

import faulthandler
import gc
import struct

import arm_emu as ae
import arm_piano_load as pl
import pytest

pytest.importorskip("unicorn")
faulthandler.disable()

BIG = 0x600000  # covers FILE_READ and DIR_LOOKUP
STORAGE = ae.SRAM_BASE + 0x1000
BUF = ae.SRAM_BASE + 0x2000
OUT = ae.SRAM_BASE + 0x3000


def make_emu() -> ae.ArmEmu:
    emu = ae.ArmEmu(bytes(BIG), ram_size=BIG)
    emu.write32(STORAGE + 0xC, 1)  # volume id of the storage object
    return emu


@pytest.fixture(autouse=True)
def _release_emulators():
    yield
    gc.collect()


def sample_npno() -> bytes:
    raw = bytearray(0x100)
    raw[0x2C:0x30] = b"CNSP"
    raw[0x30:0x32] = struct.pack(">H", 0x0464)
    return bytes(raw)


def test_device_image_places_cnsp_and_type_word_where_the_loader_reads_them():
    image = pl.device_image(sample_npno(), "Piano.npno")
    assert image[:10] == b"Piano.npno"
    assert struct.unpack_from("<I", image, pl.TYPE_OFFSET)[0] == pl.PIANO_TYPE_WORD
    assert image[0xB0:0xB4] == b"CNSP"
    assert struct.unpack_from(">H", image, 0xB4)[0] == 0x0464
    assert len(image) == pl.DEVICE_HEADER + 0x100 - pl.BACKUP_CNSP_OFFSET


def test_file_read_stub_serves_bytes_by_entry_and_offset_and_logs():
    emu = make_emu()
    stub = pl.LibraryStub({7: bytes(range(32))}).install(emu)
    status = emu.call(pl.FILE_READ, STORAGE, 7, 4, BUF, 6)
    assert status == 0
    assert emu.read_bytes(BUF, 6) == bytes([4, 5, 6, 7, 8, 9])
    assert stub.reads == [(7, 4, 6)]


def test_file_read_stub_zero_pads_past_the_end_and_rejects_unknown_entries():
    emu = make_emu()
    pl.LibraryStub({1: b"\x01\x02"}).install(emu)
    assert emu.call(pl.FILE_READ, STORAGE, 1, 1, BUF, 4) == 0
    assert emu.read_bytes(BUF, 4) == b"\x02\0\0\0"
    assert emu.call(pl.FILE_READ, STORAGE, 9, 0, BUF, 4) == 1


def test_directory_lookup_stub_fills_the_descriptor():
    emu = make_emu()
    pl.LibraryStub({3: bytes(40)}, kind=pl.KIND_HEADER_ONLY).install(emu)
    assert emu.call(pl.DIR_LOOKUP, STORAGE, 3, OUT) == 0
    volume, entry, _a, _b, kind, size, _c = struct.unpack("<HHIIIII", emu.read_bytes(OUT, 24))
    assert (volume, entry, kind, size) == (1, 3, pl.KIND_HEADER_ONLY, 40)
    assert emu.call(pl.DIR_LOOKUP, STORAGE, 4, OUT) == 1


def test_find_words_returns_aligned_matches_only():
    emu = make_emu()
    base = ae.SRAM_BASE
    emu.write32(base + 0x40, pl.SOUND_VTABLE)
    emu.write32(base + 0x1F0, pl.SOUND_VTABLE)
    emu.write_bytes(base + 0x81, struct.pack("<I", pl.SOUND_VTABLE))  # unaligned
    hits = pl.find_words(emu, pl.SOUND_VTABLE, lo=base, hi=base + 0x100000, step=0x1000)
    assert hits == [base + 0x40, base + 0x1F0]


def test_sound_state_reads_the_mode_word():
    emu = make_emu()
    emu.write32(STORAGE + 0x64C0, 2)
    assert pl.sound_state(emu, STORAGE) == 2


def test_read_npno_reads_the_head_of_a_member(tmp_path):
    import zipfile

    path = tmp_path / "x.ns4b"
    with zipfile.ZipFile(path, "w") as archive:
        archive.writestr("Piano/A.npno", b"0123456789")
    assert pl.read_npno(path, "Piano/A.npno", limit=4) == b"0123"
