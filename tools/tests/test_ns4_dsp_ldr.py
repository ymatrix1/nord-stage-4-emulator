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
