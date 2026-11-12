from __future__ import annotations

import math
import random
import struct
from pathlib import Path

import fw_classify as fc

SIZE = 0x100000


def words(values: list[int]) -> bytes:
    return struct.pack(f"<{len(values)}I", *values)


def test_zero_and_text_blocks():
    assert fc.classify_block(bytes(4096), SIZE) == "zero"
    assert fc.classify_block((b"Hello world " * 400)[:4096], SIZE) == "text"


def test_arm_block_is_recognised_by_condition_field():
    arm = words([0xE1A00000 + (i & 0xFF) for i in range(1024)])
    assert fc.classify_block(arm, SIZE) == "arm"


def test_pointer_table_block():
    ptrs = words([fc.LOAD_BASE + 0x1000 + 4 * i for i in range(1024)])
    assert fc.classify_block(ptrs, SIZE) == "ptr"
    # pointers outside the image do not count
    assert (
        fc.classify_block(words([fc.LOAD_BASE + SIZE + 4 * i for i in range(1024)]), SIZE) != "ptr"
    )


def test_float_block():
    vals = [math.sin(i / 50.0) * 0.8 for i in range(1024)]
    block = struct.pack("<1024f", *vals)
    assert fc.classify_block(block, SIZE) == "float"


def test_pcm16_block_and_other_noise():
    pcm = struct.pack("<2048h", *[int(9000 * math.sin(i / 20.0)) for i in range(2048)])
    assert fc.classify_block(pcm, SIZE) == "pcm16"
    rng = random.Random(3)
    noise = bytes(rng.randrange(256) for _ in range(4096))
    assert fc.classify_block(noise, SIZE) == "other"


def test_entropy_bounds():
    assert fc.entropy(bytes(100)) == 0.0
    assert fc.entropy(bytes(range(256)) * 4) == 8.0


def test_classify_image_and_merge_runs():
    img = (
        bytes(4096) + (b"Hello world " * 400)[:4096] + (b"Hello world " * 400)[:4096] + bytes(4096)
    )
    labels = fc.classify_image(img)
    assert [c for _, c in labels] == ["zero", "text", "text", "zero"]
    assert fc.merge_runs(labels) == [
        (0, 4096, "zero"),
        (4096, 12288, "text"),
        (12288, 16384, "zero"),
    ]
    assert fc.merge_runs([]) == []


def test_cli_prints_runs_and_totals(tmp_path: Path, capsys):
    img = tmp_path / "code.bin"
    img.write_bytes(bytes(8192) + words([0xE1A00000 + i for i in range(1024)]) * 4)
    assert fc.main([str(img), "--min-kib", "4"]) == 0
    out = capsys.readouterr().out
    assert "zero" in out and "arm" in out and "totals:" in out
