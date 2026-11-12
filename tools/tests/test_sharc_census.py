import os
import struct
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import ns4_dsp_ldr as ldr  # noqa: E402
import sharc_census as c  # noqa: E402


def test_normalise_and_operations():
    assert c.normalise("IF EQ R1 = R2 * R3 (SSF)") == "X = X * X (SSF)"
    assert c.normalise("I5 = 0xc03d8") == "X = #"
    assert c.operations("R12 = DM(0, I5), F8 = PASS F8") == ["X = DM(0, X)", "X = PASS X"]


def stream_with_code():
    """A one-core loader stream whose code block holds many `I5 = 0xC0438` loads."""
    load = struct.pack("<3H", 0x0F15, 0x000C, 0x0438)
    code = load * 40

    def block(kind, address, count, payload=b"", aux=0):
        body = struct.pack("<3I", address, count, aux)
        tag3 = bytes([kind & 0xFF, (kind >> 8) & 0xFF, 0])
        x = 0
        for b in tag3 + body:
            x ^= b
        return tag3 + bytes([x]) + body + payload

    inner = block(ldr.DATA, 0x28240000, len(code), code) + block(ldr.FINAL, 0x120000, 0)
    return block(ldr.SECTION, 0x120000, 0, aux=len(inner)) + inner


def test_census_core_and_dir(tmp_path, capsys):
    img = stream_with_code()
    blocks, end = ldr.parse_blocks(img)
    assert end == len(img)
    core = ldr.split_cores(blocks)[0]
    types, ops, total, raw = c.census_core(bytes(ldr.sram_image(img, core)))
    assert types["17a"] >= 40 and total - raw >= 40 and ops["X = #"] >= 40
    (tmp_path / "dsp_image_0_FX.bin").write_bytes(img)
    (tmp_path / "dsp_image_3_SY1SY2.bin").write_bytes(bytes(64))  # no loader stream: skipped
    assert c.main([str(tmp_path), "--top", "3"]) == 0
    assert "1 core sections" in capsys.readouterr().out
    empty = tmp_path / "e"
    empty.mkdir()
    assert c.main([str(empty)]) == 1
