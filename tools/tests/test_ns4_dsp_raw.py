"""Tests for tools/ns4_dsp_raw.py (synthetic images, no firmware needed)."""

import struct
import sys

import ns4_dsp_raw as raw


def block(tag: int, count: int, address: int, payload: bytes = b"") -> bytes:
    out = struct.pack("<3I", tag, count, address) + payload
    return out + b"\x00" * (-len(out) % 4)


def make_image() -> bytes:
    kernel = bytearray(raw.KERNEL_BYTES)
    kernel[raw.RESET_OFFSET : raw.RESET_OFFSET + 6] = raw.RESET_CALL
    stream = (
        block(raw.CODE, 3, 0x1C0300, bytes.fromhex("aabbccddeeff"))  # 6 bytes, padded to 8
        + block(raw.ZERO, 2, 0x92180)
        + block(raw.DATA, 2, 0x92182, struct.pack("<2I", 7, 9))
    )
    ivt = bytearray(raw.IVT_BYTES)
    ivt[30:36] = (0x063E_001C_11E0).to_bytes(6, "little")  # JUMP 0x1c11e0 in reset slot 5
    return bytes(kernel) + stream + block(0, 0, 0) + bytes(ivt)


def test_recognises_the_kernel_and_rejects_other_data():
    assert raw.is_raw_image(make_image())
    assert not raw.is_raw_image(b"\x03\x50" + bytes(0x700))
    assert not raw.is_raw_image(b"")


def test_isa_words_are_little_endian_six_byte_words():
    words = raw.isa_words(make_image())
    assert len(words) == 256
    assert words[5] == 0x06BE_0408_C091  # call 0x8c091 (db), the reset vector of the kernel
    assert words[0] == 0


def test_blocks_are_aligned_to_four_bytes_and_stop_at_the_tail():
    blocks, end = raw.parse_blocks(make_image())
    assert [(b.tag, b.count, b.address) for b in blocks] == [
        (raw.CODE, 3, 0x1C0300),
        (raw.ZERO, 2, 0x92180),
        (raw.DATA, 2, 0x92182),
    ]
    assert blocks[0].next_offset - blocks[0].offset == 12 + 8, "6 payload bytes pad to 8"
    assert end == len(make_image()) - raw.IVT_BYTES - raw.HEADER


def test_truncated_payload_stops_the_walk():
    img = make_image()[: raw.KERNEL_BYTES + 12 + 4]
    assert raw.parse_blocks(img)[0] == []


def test_reset_target_and_tail():
    img = make_image()
    assert raw.reset_target(img) == 0x1C11E0
    assert len(raw.tail(img)) == raw.IVT_BYTES
    assert raw.reset_target(img[: raw.KERNEL_BYTES]) is None


def test_sram_image_places_code_data_and_vector_table():
    mem = raw.sram_image(make_image())
    assert len(mem) == raw.SRAM_BYTES
    assert mem[2 * 0x1C0300 : 2 * 0x1C0300 + 6] == bytes.fromhex("aabbccddeeff")
    assert struct.unpack_from("<2I", mem, 4 * 0x92182) == (7, 9)
    assert mem[4 * 0x92180 : 4 * 0x92182] == bytes(8)
    assert mem[2 * raw.IVT_SHORT + 30 : 2 * raw.IVT_SHORT + 36] == (0x063E_001C_11E0).to_bytes(
        6, "little"
    )


def test_common_prefix():
    assert raw.common_prefix(b"abcd", b"abxd") == 2
    assert raw.common_prefix(b"ab", b"abc") == 2


def test_summary_and_cli(tmp_path, monkeypatch, capsys):
    path = tmp_path / "img.bin"
    path.write_bytes(make_image())
    text = raw.summary(make_image())
    assert "3 blocks" in text and "reset vector jumps to 0x1c11e0" in text
    kernel, sram = tmp_path / "k.bin", tmp_path / "s.bin"
    monkeypatch.setattr(
        sys,
        "argv",
        [
            "ns4_dsp_raw.py",
            str(path),
            "--other",
            str(path),
            "--kernel",
            str(kernel),
            "--sram",
            str(sram),
        ],
    )
    assert raw.main() == 0
    assert kernel.stat().st_size == raw.KERNEL_BYTES and sram.stat().st_size == raw.SRAM_BYTES
    assert "common prefix" in capsys.readouterr().out
    path.write_bytes(b"nope")
    assert raw.main() == 1
