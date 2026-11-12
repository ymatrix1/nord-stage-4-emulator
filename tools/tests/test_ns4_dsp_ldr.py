import os
import struct
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import ns4_dsp_ldr as ldr  # noqa: E402


def block(kind, address, count, aux=0, payload=b""):
    """A 16-byte header with a correct tag checksum, followed by the payload."""
    tag_low = struct.pack("<I", kind)[:3]  # bytes 0..2 of the tag
    body = struct.pack("<3I", address, count, aux)
    x = 0
    for b in tag_low + body:
        x ^= b
    return tag_low + bytes([x]) + body + payload


def core(entry, parts):
    inner = b"".join(parts) + block(ldr.FINAL, entry, 0)
    return block(ldr.SECTION, entry, 0, len(inner)) + inner


def image():
    c1 = core(
        0x1211E3,
        [
            block(ldr.DATA, 0x28240000, 8, payload=bytes(range(8))),
            block(ldr.ZERO, 0x20010000, 0x4000),
        ],
    )
    c2 = core(0x1211E3, [block(ldr.DATA, 0x28A40000, 4, payload=b"\xaa\xbb\xcc\xdd")])
    return c1 + c2


def test_checksum_and_block_sizes():
    b = block(ldr.DATA, 0x28240000, 8, payload=bytes(8))
    assert ldr.header_checksum(b) == b[3]
    img = image()
    blocks, end = ldr.parse_blocks(img)
    assert end == len(img) and all(x.checksum_ok for x in blocks)
    assert [ldr.KIND_NAMES[x.kind] for x in blocks][:3] == ["section", "data", "zero"]
    assert blocks[2].payload_size == 0 and blocks[1].payload_size == 8


def test_split_cores_and_load_map():
    blocks, _ = ldr.parse_blocks(image())
    cores = ldr.split_cores(blocks)
    assert len(cores) == 2 and cores[0][0].aux == sum(16 + b.payload_size for b in cores[0][1:])
    m = ldr.load_map(cores[0])
    assert m[0x28] == (1, 8, 0, 0, 0x28240000) and m[0x20] == (0, 0, 1, 0x4000, 0x20010000)
    assert ldr.address_range(cores[0], 0x20) == (0x20010000, 0x20014000)


def test_segments_and_bad_stream(tmp_path, capsys):
    img = image()
    blocks, _ = ldr.parse_blocks(img)
    cores = ldr.split_cores(blocks)
    assert ldr.segments(img, cores[1]) == [(0x28A40000, b"\xaa\xbb\xcc\xdd")]
    f = tmp_path / "img.bin"
    f.write_bytes(img)
    assert ldr.main([str(f), "--blocks", "--out", str(tmp_path / "o")]) == 0
    out = capsys.readouterr().out
    assert "2 core sections" in out and "failures: 0" in out
    assert len(list((tmp_path / "o").iterdir())) == 2
    bad = tmp_path / "bad.bin"
    bad.write_bytes(bytes(64))
    assert ldr.main([str(bad)]) == 1


def test_bad_checksum_is_flagged():
    b = bytearray(block(ldr.ZERO, 0x20010000, 64))
    b[3] ^= 0xFF
    blocks, end = ldr.parse_blocks(bytes(b))
    assert end == len(b) and blocks[0].checksum_ok is False


def test_address_mapping_helpers_and_sram_image():
    assert ldr.chip_of(0x28240000) == 0 and ldr.chip_of(0x28A40000) == 1
    assert ldr.short_word_range(0x28240000, 0x300) == (0x120000, 0x12017F)
    img = image()
    blocks, _ = ldr.parse_blocks(img)
    cores = ldr.split_cores(blocks)
    mem = ldr.sram_image(img, cores[0])
    assert len(mem) == ldr.SRAM_BYTES and bytes(mem[0x240000:0x240008]) == bytes(range(8))
    mem2 = ldr.sram_image(img, cores[1])
    assert bytes(mem2[0x240000:0x240004]) == bytes(
        [170, 187, 204, 221]
    )  # 0x28A40000 -> same offset
    assert not any(mem[0x240008:0x240100])  # zeros elsewhere


def test_main_writes_sram_images(tmp_path):
    f = tmp_path / "img.bin"
    f.write_bytes(image())
    assert ldr.main([str(f), "--sram", str(tmp_path / "s")]) == 0
    assert (tmp_path / "s" / "core1_sram.bin").stat().st_size == ldr.SRAM_BYTES


def test_l2_image_places_blocks_by_offset(tmp_path):
    payload = bytes([1, 2, 3, 4])
    c1 = core(0x1211E3, [block(ldr.DATA, 0x20004160, 4, payload=payload)])
    img = c1 + core(0x1211E3, [block(ldr.DATA, 0x28A40000, 4, payload=bytes(4))])
    blocks, _ = ldr.parse_blocks(img)
    cores = ldr.split_cores(blocks)
    mem = ldr.l2_image(img, cores[0])
    assert len(mem) == ldr.L2_BYTES and bytes(mem[0x4160:0x4164]) == payload
    assert not any(ldr.l2_image(img, cores[1]))  # core 2 has no L2 block
    f = tmp_path / "img.bin"
    f.write_bytes(img)
    assert ldr.main([str(f), "--sram", str(tmp_path / "s")]) == 0
    assert (tmp_path / "s" / "core1_l2.bin").stat().st_size == ldr.L2_BYTES
