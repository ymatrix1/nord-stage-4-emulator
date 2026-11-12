import os
import struct
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import fw_dsp_scan as s  # noqa: E402


def test_entropy_bounds():
    assert s.entropy(b"") == 0.0
    assert s.entropy(b"\x00" * 100) == 0.0
    assert abs(s.entropy(bytes(range(256)) * 4) - 8.0) < 1e-9


def test_period_contrast_flags_six_byte_words():
    word = bytes([0x0A, 0x00, 0x00, 0x00, 0x7F])
    sharc_like = b"".join(word + bytes([i % 7]) for i in range(600))
    assert s.period_contrast(sharc_like) > 0.5
    noise = bytes((i * 2654435761 >> 13) & 0xFF for i in range(3600))
    assert s.period_contrast(noise) < 0.2


def test_ldr_candidates_matches_header_shape():
    blob = struct.pack("<III", 0x1, 0x100, 0x90000) + struct.pack("<III", 5, 0, 0x90000)
    assert s.ldr_candidates(blob) == [0]
    assert s.ldr_candidates(struct.pack(">III", 1, 0x100, 0x90000), byteorder=">") == [0]


def test_windows_and_main(tmp_path, capsys):
    f = tmp_path / "x.bin"
    f.write_bytes(bytes(64))
    assert s.windows(bytes(64), 32)[0][1] == 0.0
    assert s.main([str(f), "--window", "32"]) == 0
    assert s.main([str(f), "--ldr"]) == 0
    assert "entropy" in capsys.readouterr().out
