import os
import struct
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import ns4_synth_kernels as k  # noqa: E402


def test_window_and_sinc_basics():
    assert k.sinc(0) == 1.0 and abs(k.sinc(1)) < 1e-12 and abs(k.sinc(2)) < 1e-12
    assert abs(k.blackman(0) - 1.0) < 1e-12
    assert k.blackman(6.0) == 0.0 and k.blackman(-7.0) == 0.0
    assert abs(k.blackman(3.0) - 0.34) < 1e-12  # 0.42 + 0 + 0.08 cos(pi) -> 0.34


def test_model_block_layout():
    b = k.model_block(1.0)
    assert len(b) == k.BLOCK == 3072
    assert abs(b[5 * 256] - 1.0) < 1e-12  # tap 5, phase 0 is the sinc peak
    assert abs(b[6 * 256]) < 1e-12  # tap 6, phase 0 is a zero crossing
    assert abs(b[5 * 256 + 128] - k.sinc(0.5) * k.blackman(0.5)) < 1e-12


def test_fit_cutoff_recovers_each_cutoff():
    for fc in (1.10, 1.05, 1.0, 0.96, 0.925, 0.895):
        block = k.model_block(fc)
        assert abs(k.fit_cutoff(block) - fc) < 0.01
        assert k.rms_error(block, k.fit_cutoff(block)) < 0.005


def test_find_kernels_in_image_and_cli(tmp_path, capsys):
    blocks = [k.model_block(fc) for fc in (1.1, 1.0, 0.9)]
    floats = [0.5 + 0.001 * i for i in range(200)]  # smooth filler without the model shape
    data = struct.pack(f"<{len(floats)}f", *floats) + struct.pack("<f", 99.0)
    offset = len(data)
    data += b"".join(struct.pack(f"<{k.BLOCK}f", *b) for b in blocks) + struct.pack("<f", 99.0)
    found_off, found = k.find_kernels(data)
    assert found_off == offset and len(found) == 3
    f = tmp_path / "img.bin"
    f.write_bytes(data)
    assert k.main([str(f)]) == 0
    assert "3 kernels" in capsys.readouterr().out
    empty = tmp_path / "e.bin"
    empty.write_bytes(bytes(256))
    assert k.main([str(empty)]) == 1
