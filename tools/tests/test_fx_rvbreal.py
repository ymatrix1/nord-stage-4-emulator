"""Tests for tools/fx_rvbreal.py: the real instance layout and the response measures."""

import struct

import fx_rvbreal as rr
import numpy as np


def write_image(path, words):
    raw = bytearray(4 * 0xC0000)
    for a, w in words.items():
        struct.pack_into("<I", raw, 4 * a, w)
    path.write_bytes(bytes(raw))


def test_real_layout_reads_the_image_and_overlays_type_words_and_the_amount(tmp_path):
    img = tmp_path / "core.bin"
    write_image(img, {rr.RCTX + 39: 0x802F000, rr.RCTX + 40: 5000, rr.RCTX + 17: 0x3F7DC91D})
    mem = rr.real_layout(str(img), 0, 127)
    assert mem[rr.RCTX + 39] == 0x802F000 and mem[rr.RCTX + 40] == 5000
    assert mem[rr.RCTX + 43] == 0xFFFFF59E  # type 0 tap -2658 words from the ARM type words
    assert mem[rr.RCTX + 0x12] == 0x3F800000 and mem[rr.RCTX + 0x13] == 0  # amount 127: wet only
    low = rr.real_layout(str(img), 1, 64)
    assert low[rr.RCTX + 0x12] == 0x3EC00000  # 0.375 = x^2 (2 - x) at x = 1/2
    assert low[rr.RCTX + 43] != mem[rr.RCTX + 43] or low[rr.RCTX - 17] != mem[rr.RCTX - 17]


def test_t60_of_an_exponential_decay_and_echo_density_of_noise():
    fs = 48000
    t = np.arange(fs * 2) / fs
    h = np.exp(-6.9078 * t / 1.5)  # -60 dB in 1.5 s (amplitude)
    t60, edt = rr.t60_s(h, fs)
    assert abs(t60 - 1.5) < 0.05 and abs(edt - 1.5) < 0.05
    rng = np.random.default_rng(1)
    noise = rng.standard_normal(fs)
    dens = rr.echo_density(noise, fs)
    assert all(abs(v - 1.0) < 0.15 for _, v in dens)


def test_band_db_of_a_gain_of_one_half_is_minus_six_db():
    rng = np.random.default_rng(2)
    x = rng.standard_normal(48000)
    bands = ((100, 1000), (1000, 8000))
    assert all(abs(v + 6.02) < 0.01 for v in rr.band_db(x, 0.5 * x, 48000, bands))
