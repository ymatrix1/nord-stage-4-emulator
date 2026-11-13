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


def test_select_sound_calls_the_select_method_with_core_id_arg():
    emu = make_emu()
    emu.write32(pl.SELECT_SOUND, 0xE1A00001)  # mov r0, r1
    emu.write32(pl.SELECT_SOUND + 4, 0xE12FFF1E)  # bx lr
    emu.set_reg(13, ae.STACK_TOP - 0x2000)
    assert pl.select_sound(emu, 9, core=ae.SRAM_BASE) == 9


def test_bound_sound_reads_the_layer_slot():
    emu = make_emu()
    emu.write32(STORAGE + 4 + 8, 0x1234)
    assert pl.bound_sound(emu, core=STORAGE, layer=2) == 0x1234
    assert pl.bound_sound(emu, core=STORAGE, layer=0) == 0


def test_dsp_ready_sets_the_polled_bit_to_the_expected_level():
    emu = make_emu()
    pl.install_dsp_ready(emu)
    emu.set_reg(1, 1)
    emu.set_reg(12, 5)
    emu.set_reg(3, 0)
    emu.run(pl.GPIO_POLL_PC, pl.GPIO_POLL_PC + 4, max_insns=4)
    assert emu.reg(3) == 1 << 5


def test_level_index_matches_the_listing_formula():
    assert pl.level_index(36, 170, 0, 0, 0) == 36 + 340 + 60
    assert pl.level_index(66, 0, 10, 20, 10) == 86


def test_hook_level_calls_records_arguments_and_the_stored_output():
    emu = make_emu()
    # str r0, [out ptr loaded from the stack]: mov r0,#7 ; ldr r1,[sp,#8] ; str r0,[r1] ; bx lr
    code = [0xE3A00007, 0xE59D1008, 0xE5810000, 0xE12FFF1E]
    for i, word in enumerate(code):
        emu.write32(pl.LEVEL_PIANO + 4 * i, word)
    emu.write_bytes(OUT + 0x5E, struct.pack("<h", -3))
    calls = pl.hook_level_calls(emu)
    ret = ae.LOAD_BASE + 0x2000  # return site (zeroed code: harmless no-op)
    sp = ae.STACK_TOP - 0x2000
    emu.set_reg(13, sp)
    for i, word in enumerate((99, 0, BUF)):
        emu.write32(sp + 4 * i, word)
    for reg, value in ((0, 0), (1, 0), (2, OUT), (3, 60), (14, ret)):
        emu.set_reg(reg, value)
    emu.run(pl.LEVEL_PIANO, ret + 4, max_insns=20)
    assert calls == [pl.LevelCall(99, -3, 60, 7)]


# (velocity, X, rec5e, amplitude): the level calls of the Clavinet D6 sweep, key 60, per-key
# bytes summing to 40 (`docs/research/piano-sound-load.md` section 6).
LEVEL_SWEEP = [
    (1, 170, 36, 0x151CB),
    (32, 112, 36, 0x50A28),
    (64, 66, 36, 0xE966A),
    (88, 36, 56, 0x172803),
    (112, 9, 66, 0x267F2E),
    (127, 0, 66, 0x2F6526),
]


@pytest.mark.parametrize(("velocity", "x", "rec5e", "amplitude"), LEVEL_SWEEP)
def test_level_amplitude_reproduces_the_captured_levels(velocity, x, rec5e, amplitude):
    assert pl.level_amplitude(pl.level_index(rec5e, x, 15, 15, 10)) == amplitude


def test_level_amplitude_saturates_and_clamps():
    assert pl.level_amplitude(-5) == 0x7FFFFF
    assert pl.level_amplitude(60) == 0x400000
    assert pl.level_amplitude(600) == 0x800000 >> 10
