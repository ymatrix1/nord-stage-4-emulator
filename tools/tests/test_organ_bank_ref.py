"""Unit tests for tools/organ_bank_ref.py (synthetic tables; the oracle is the model)."""

from __future__ import annotations

import numpy as np
import organ_bank_ref as ob
import pytest

HALF = 0x40000000


def f2u(x: float) -> int:
    return int(np.float32(x).view(np.uint32))


def sine_table(h2: float = 0.0) -> np.ndarray:
    """4096 (x, y) pairs: x = sin t + h2 sin 2t, y = cos t, as u32 words."""
    t = 2 * np.pi * np.arange(4096) / 4096
    x = (np.sin(t) + h2 * np.sin(2 * t)).astype(np.float32).view(np.uint32)
    y = np.cos(t).astype(np.float32).view(np.uint32)
    return np.stack([x, y], axis=1).reshape(-1)


def test_fracmul_halves_and_rounds_and_keeps_sign():
    a = np.array([0x100, 0x101, 0xFFFFFF00, 0x7FFFFFFF], dtype=np.uint32)
    out = ob.fracmul(a, HALF)
    assert out[0] == 0x80
    assert out[1] == 0x81  # 0x80.8 rounds up
    assert np.int32(out[2]) == -0x80
    assert out[3] == 0x40000000  # 0x3FFFFFFF.8 rounds up


def test_fracmul_by_negative_factor_flips_sign():
    out = ob.fracmul(np.array([0x100], dtype=np.uint32), 0xC0000000)  # -0.5
    assert np.int32(out[0]) == -0x80


def test_bank_index_is_top_twelve_bits_after_the_step():
    table = np.arange(8192, dtype=np.uint32)
    bank = ob.ToneBank(table)
    inc = np.zeros(ob.ELEMENTS, dtype=np.uint32)
    inc[3] = 5 << 20 << 1  # step = 5 << 20 per frame at tune 0.5
    x, y = bank.frame(inc)
    assert x[3] == 2 * 5 and y[3] == 2 * 5 + 1
    assert x[0] == 0 and y[0] == 1
    x, _ = bank.frame(inc)
    assert x[3] == 2 * 10


def test_bank_phase_wraps_at_2_pow_32():
    bank = ob.ToneBank(np.arange(8192, dtype=np.uint32))
    inc = np.full(ob.ELEMENTS, 0xFFFFFFFE, dtype=np.uint32)  # step = -1 (as 0xFFFFFFFF)
    x, _ = bank.frame(inc)
    assert np.all(x == 2 * 4095)
    assert np.all(bank.phase == 0xFFFFFFFF)


def test_call_is_two_frames_of_184_words():
    bank = ob.ToneBank(sine_table())
    inc = np.full(ob.ELEMENTS, 1 << 24, dtype=np.uint32)
    x, y = bank.call(inc)
    assert x.shape == (368,) and y.shape == (368,)
    assert np.array_equal(x[:184], bank.table[(1 << 23) >> 20, 0] * np.ones(184, np.uint32))


def test_cycles_per_frame_unity_tune_is_v_over_2_pow_25():
    assert ob.cycles_per_frame(1 << 25) == pytest.approx(1.0)
    assert ob.cycles_per_frame(1 << 25, 0x20000000) == pytest.approx(0.5)


def test_arm_tables_are_read_from_their_load_addresses():
    arm = bytearray(0x500000)
    for j in range(ob.ELEMENTS):
        arm[0x443EA8 + 4 * j : 0x443EA8 + 4 * j + 4] = (1000 + j).to_bytes(4, "little")
    arm[0x27D9D8 + 4 * 0x200 : 0x27D9D8 + 4 * 0x200 + 4] = (0x400000).to_bytes(4, "little")
    v = ob.default_increments(bytes(arm))
    assert v[0] == 1000 and v[183] == 1183
    assert ob.tune_ratio(bytes(arm), 0x200) == 0x40000000


def test_semitone_fit_recovers_ladder_and_pair_spread():
    k = np.arange(92)
    a = 25430 * 2 ** (k / 12)
    v = np.empty(184)
    v[0::2] = a * 2 ** (0.5 / 1200)
    v[1::2] = a * 2 ** (-0.5 / 1200)
    fit = ob.semitone_fit(v)
    assert fit["slope_per_note"] == pytest.approx(1.0, abs=1e-6)
    assert fit["v0"] == pytest.approx(25430, rel=1e-5)
    assert fit["pair_mean_cents"] == pytest.approx(-1.0, abs=1e-6)
    assert fit["ladder_rms_cents"] < 1e-3


def test_drawbar_weights_follow_footage_offsets_and_clip():
    w = ob.drawbar_weights(30, (0, 0, 8, 0, 0, 0, 0, 0, 0))
    assert np.flatnonzero(w).tolist() == [30] and w[30] == pytest.approx(1.0)
    w = ob.drawbar_weights(30, (8, 7, 8, 8, 0, 0, 0, 0, 8))
    assert np.flatnonzero(w).tolist() == [18, 30, 37, 42, 66]
    assert w[37] == pytest.approx(10 ** (-3 / 20))
    assert ob.drawbar_weights(2, (8, 0, 0, 0, 0, 0, 0, 0, 0)).sum() == 0  # -12 is off the bank


def test_lane_sums_split_even_and_odd_elements():
    x = np.zeros((2, 184), dtype=np.float32)
    x[:, 0] = 1.0
    x[:, 3] = 2.0
    w = np.arange(92, dtype=float)
    even, odd = ob.lane_sums(x, w)
    assert even.tolist() == [0.0, 0.0] and odd.tolist() == [2.0, 2.0]
    x[:, 4] = 3.0
    assert ob.lane_sums(x, w)[0].tolist() == [6.0, 6.0]


def test_tone_level_db_measures_a_second_harmonic():
    cycles = 0.0123
    n = np.arange(8192)
    sig = np.sin(2 * np.pi * cycles * n) + 0.1 * np.sin(2 * np.pi * 2 * cycles * n)
    lev = ob.tone_level_db(sig, cycles, 3)
    assert lev[0] == pytest.approx(0.0)
    assert lev[1] == pytest.approx(-20.0, abs=0.3)
    assert lev[2] < -60


def test_tone_amplitude_of_a_sine():
    n = np.arange(4096)
    assert ob.tone_amplitude(0.7 * np.sin(2 * np.pi * 0.05 * n + 1), 0.05) == pytest.approx(
        0.7, abs=0.02
    )


def test_words_and_float_views_roundtrip():
    blob = np.array([1, 2, f2u(1.5)], dtype="<u4").tobytes()
    words = ob.read_words(blob, 1, 2)
    assert words.tolist() == [2, f2u(1.5)]
    assert ob.f32(words)[1] == 1.5
    assert ob.arm_words(blob, ob.ARM_BASE + 8, 1).tolist() == [f2u(1.5)]


def _fake_oracle(table_words: np.ndarray):
    """An `oracle_words` stand-in that runs the model (same call convention)."""

    def fake(exe, image, incs, calls, tune=None, updates=None):
        bank = ob.ToneBank(table_words, tune=ob.DEFAULT_TUNE if tune is None else tune)
        out = np.zeros((calls, 2, 368), dtype=np.uint32)
        cur = np.asarray(incs, dtype=np.uint32).copy()
        for c in range(calls):
            for addr, value in (updates or {}).get(c, {}).items():
                cur[addr - ob.INC_ADDR] = value
            out[c, 0], out[c, 1] = bank.call(cur)
        return out

    return fake


def _write_sram(path, table_words: np.ndarray) -> None:
    blob = np.zeros(ob.TABLE_ADDR + 8192, dtype="<u4")
    blob[ob.TABLE_ADDR : ob.TABLE_ADDR + 8192] = table_words
    path.write_bytes(blob.tobytes())


def _write_arm(path) -> None:
    arm = np.zeros(0x450000 // 4, dtype="<u4")
    base = (0x443EA8) // 4
    for j in range(ob.ELEMENTS):
        arm[base + j] = int(25430 * 2 ** ((j // 2) / 12))
    ti = 0x27D9D8 // 4
    arm[ti + 0x1F3] = 0x3FA02A
    arm[ti + 0x200] = 0x400000
    path.write_bytes(arm.tobytes())


def test_cli_defaults_prints_fit(tmp_path, capsys):
    arm = tmp_path / "arm.bin"
    _write_arm(arm)
    assert ob.main(["defaults", str(arm)]) == 0
    out = capsys.readouterr().out
    assert "v0" in out and "0x40000000" in out and "0x3fa02a00" in out


def test_cli_compare_passes_when_oracle_equals_model(tmp_path, monkeypatch, capsys):
    table = sine_table()
    sram = tmp_path / "s.bin"
    _write_sram(sram, table)
    monkeypatch.setattr(ob, "oracle_words", _fake_oracle(table))
    assert ob.main(["compare", str(sram), "--exe", "x", "--calls", "4"]) == 0
    assert "mismatches: 0" in capsys.readouterr().out


def test_cli_compare_fails_when_oracle_differs(tmp_path, monkeypatch):
    table = sine_table()
    sram = tmp_path / "s.bin"
    _write_sram(sram, table)
    monkeypatch.setattr(ob, "oracle_words", _fake_oracle(table + 1))
    assert ob.main(["compare", str(sram), "--exe", "x", "--calls", "2"]) == 1


def test_cli_spectrum_and_chord_use_the_oracle_words(tmp_path, monkeypatch, capsys):
    table = sine_table(h2=0.1)
    arm = tmp_path / "arm.bin"
    _write_arm(arm)
    sram = tmp_path / "s.bin"
    _write_sram(sram, table)
    monkeypatch.setattr(ob, "oracle_words", _fake_oracle(table))
    assert (
        ob.main(["spectrum", str(sram), "--exe", "x", "--arm", str(arm), "--frames", "4096"]) == 0
    )
    assert "harmonics dB" in capsys.readouterr().out
    argv = ["chord", str(sram), "--exe", "x", "--arm", str(arm), "--frames", "4096", "--root", "34"]
    assert ob.main(argv) == 0
    assert "wheel 34" in capsys.readouterr().out
