"""Tests for tools/arm_leslie_speeds.py: the ARM speed law against the observed DSP words."""

import struct

import arm_leslie_speeds as als


def test_the_arm_law_reproduces_the_observed_words():
    assert als.speed_word(0.8) == 0x26000
    assert als.speed_word(0.73) == 0x22B00
    assert als.speed_word(0.86) == 0x28E00
    assert als.speed_word(0.74) == 0x23300


def test_the_third_word_follows_the_horn_word_in_both_observed_states():
    assert als.words(0.8, 0.73)[0xB2FA1] == 0x15D00
    assert als.words(0.86, 0.74)[0xB2FA1] == 0x17800


def test_fast_words_and_the_hz_they_mean_at_the_dsp_rate():
    fast = als.words(7.0, 6.4)
    assert fast[0xB2F26] == 0x14CE00 and fast[0xB2F25] == 0x130500
    assert abs(als.hz_of_word(0x26000) - 0.786) < 0.002
    assert abs(als.hz_of_word(0x22B00) - 0.718) < 0.002


def test_block_reader_names_the_doubles():
    mem = {0x9115D2B0 + 0x48: 0.8, 0x9115D2B0 + 0x50: 7.0, 0x9115D2B0 + 0xB8: 6.4}

    def read(a, n):
        return struct.pack("<d", mem.get(a, 0.0))

    blk = als.read_block(read)
    assert (blk["horn_slow"], blk["horn_fast"], blk["rotor_fast"]) == (0.8, 7.0, 6.4)
