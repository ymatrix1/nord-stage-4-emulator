"""Tests for tools/fx_mod_words.py: the instance words check on a synthetic image."""

import struct

import fx_mod_words as mw


def image(words: dict[int, int]) -> bytes:
    img = bytearray(4 * 0xC2000)
    for addr, w in words.items():
        struct.pack_into("<I", img, 4 * addr, w)
    return bytes(img)


def expected_image() -> dict[int, int]:
    return {base + off: w for base, words in mw.EXPECTED.values() for off, w in words.items()}


def test_a_matching_image_has_no_mismatch():
    assert mw.check(image(expected_image())) == []


def test_a_wrong_word_is_reported_with_its_address():
    words = expected_image()
    words[mw.PAN_BASE - 8] = 0
    bad = mw.check(image(words))
    assert len(bad) == 1 and "pan c[-8]" in bad[0] and "0xffffffe1" in bad[0]


def test_main_returns_the_status(tmp_path, capsys):
    good = tmp_path / "good.bin"
    good.write_bytes(image(expected_image()))
    assert mw.main([str(good)]) == 0
    assert "22/22" in capsys.readouterr().out
    bad = tmp_path / "bad.bin"
    bad.write_bytes(image({}))
    assert mw.main([str(bad)]) == 1
    assert mw.main([]) == 2


def test_word_reads_little_endian_by_dm_address():
    assert mw.word(image({5: 0x11223344}), 5) == 0x11223344
