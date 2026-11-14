"""Tests for tools/fx_leslie_ir.py: boot log parsing, image windows and the DTFT helper."""

import math
import struct

import fx_leslie_ir as li


def test_boot_state_keeps_the_last_value_per_word(tmp_path):
    p = tmp_path / "boot.script"
    p.write_text("0 0xb2f29 0x1237f8\n0 0xb2f29 0x12637b\n\n0 0xb2f2a 0xe359e\n", encoding="utf-8")
    assert li.boot_state(p) == {0xB2F29: 0x12637B, 0xB2F2A: 0xE359E}


def test_image_words_reads_word_a_at_byte_4a(tmp_path):
    p = tmp_path / "img.bin"
    p.write_bytes(struct.pack("<4I", 1, 2, 3, 4))
    assert li.image_words(p, 1, 3) == {1: 2, 2: 3}


def test_context_overlays_boot_words_inside_the_windows_only(tmp_path):
    img = tmp_path / "img.bin"
    img.write_bytes(bytes(4 * 0xE3700))
    script = tmp_path / "boot.script"
    script.write_text("0 0xe3580 0x3e4e7c20\n0 0xb2f29 0x1237f8\n", encoding="utf-8")
    mem = li.context(img, script)
    assert mem[0xE3580] == 0x3E4E7C20
    assert 0xB2F29 not in mem
    assert mem[0x96500] == 0 and len(mem) == sum(hi - lo for lo, hi in li.WINDOWS)


def test_magnitude_of_a_unit_impulse_is_flat_and_a_delay_keeps_it():
    freqs = [100.0, 1000.0, 5000.0]
    assert all(abs(m) < 1e-9 for m in li.magnitude_db([1.0], freqs))
    assert all(abs(m) < 1e-9 for m in li.magnitude_db([0.0, 0.0, 1.0], freqs))
    two = li.magnitude_db([1.0, 1.0], [0.0])[0]
    assert abs(two - 20 * math.log10(2.0)) < 1e-9


def test_constants_name_the_leslie_row():
    assert li.CTX == {"L": 0xE359E, "R": 0xE31A6}
    assert li.PNO_PC - 0x120000 == 0x37F8 and li.FX_PC == 0x122B1D
    assert li.f32(0x3F800000) == 1.0
