"""Unit tests for tools/organ_oracle_audio.py (synthetic tables; no firmware, no oracle binary)."""

from __future__ import annotations

import ns4_oracle_compare as cmpmod
import numpy as np
import organ_bank_ref as ob
import organ_oracle_audio as oa
import pytest


def test_foldback_wraps_both_ends_like_the_engine():
    assert oa.foldback(12) == 0
    assert oa.foldback(24) == 0
    assert oa.foldback(24 + 90) == 90
    assert oa.foldback(24 + 91) == 79  # one octave down


def test_drawbar_digits_validates():
    assert oa.drawbar_digits("888000000") == [8, 8, 8, 0, 0, 0, 0, 0, 0]
    with pytest.raises(ValueError):
        oa.drawbar_digits("88")
    with pytest.raises(ValueError):
        oa.drawbar_digits("999000000")


def test_chord_weights_for_c_major_888():
    w = oa.chord_weights([60, 64, 67], oa.drawbar_digits("888000000"))
    # C4: 16' -> 48 -> wheel 24, 5 1/3' -> 67 -> 43, 8' -> 36; G4 8' also lands on wheel 43
    assert w[24] == w[36] == 1.0
    assert w[43] == 2.0
    assert w.sum() == 9.0
    lin = oa.chord_weights([60], oa.drawbar_digits("040000000"), law="linear")
    assert lin[43] == 0.5
    fw = oa.chord_weights([60], oa.drawbar_digits("040000000"))
    assert fw[43] == pytest.approx(22 / 127)


def test_increment_tables_have_184_words_and_differ():
    exact, det = oa.increments(False), oa.increments(True)
    assert len(exact) == len(det) == 184
    assert exact[0] == 0x00635300 and det[0] == 0x00635000
    assert exact[0] == exact[1]  # identical copies in the exact table


def test_wheel_cycles_is_inc_over_2_pow_33_at_default_tune():
    assert oa.wheel_cycles(0, [0x00635300, 0]) == pytest.approx(0x635300 / 2**33)


def test_pokes_cover_increments_weights_gains_and_level():
    w = np.zeros(92)
    w[5] = 0.5
    pokes = oa.build_pokes(w, detuned=True, level=0.5)
    d = {(a): v for _, a, v in pokes}
    assert d[oa.INC_ADDR] == 0x00635000
    assert d[oa.A_BASE + 5] == oa.f2u(0.5)
    assert d[oa.WB_BASE + 5] == oa.ONE
    assert d[oa.GAIN_X] == oa.f2u(0.75) and d[oa.CROSS_YX] == oa.f2u(0.375)
    assert d[oa.LEVEL] == oa.f2u(0.5)
    s1 = {a: v for _, a, v in oa.build_pokes(w, detuned=False, s=1)}
    assert oa.A_BASE + oa.SET_STRIDE_A + 5 in s1
    assert s1[oa.GAIN_X + oa.SET_STRIDE_A] == oa.f2u(0x7FFFFF / 0x800000)


def test_wav_roundtrip_float32_stereo(tmp_path):
    data = np.stack([np.linspace(-1, 1, 64), np.linspace(1, -1, 64)], axis=1).astype("<f4")
    path = tmp_path / "x.wav"
    oa.write_wav_f32(path, data, 48000)
    _, cols = cmpmod.read_wav(path)
    assert np.allclose(cols["ch0"], data[:, 0]) and np.allclose(cols["ch1"], data[:, 1])


def test_amplitude_and_best_scale_on_a_sine():
    n = np.arange(8192)
    sig = 0.7 * np.sin(2 * np.pi * 0.0123 * n + 0.4)
    assert oa.amplitude(sig, 0.0123) == pytest.approx(0.7, rel=0.01)
    g, snr = oa.best_scale(sig, 0.07 * sig)
    assert g == pytest.approx(1 / 0.07) and snr > 100
    rows = oa.partial_table(sig, np.eye(92)[0] * 0.7, [int(0.0123 * 2**33), 0])
    assert rows[0][0] == 0 and rows[0][3] == pytest.approx(0.7, rel=0.01)


def test_smoother_matches_the_recurrence():
    s = 0.0
    seq = []
    for _ in range(50):
        s = 0.01 * 1.0 + 0.99 * s
        seq.append(s)
    assert np.allclose(oa.smoothed(1.0, 50), seq)


def test_model_render_exact_lanes_equal_and_detuned_mix_matrix():
    table = ob.x_table_words()
    w = np.zeros(92)
    w[3] = 1.0
    incs = [0x00635300] * 184
    exact = oa.model_render(table, incs, w, oa.GAINS_EXACT, 1.0, 64)
    assert np.allclose(exact[:, 0], exact[:, 1], atol=1e-6)
    det_incs = [0x00635300 if j % 2 == 0 else 0 for j in range(184)]  # copy 1 frozen at table[0]
    det = oa.model_render(table, det_incs, w, oa.GAINS_DETUNED, 1.0, 64)
    tx = det[:, 0] / oa.smoothed(0.75, 64)  # lane x of copy 0 only (+ constant copy 1 term)
    assert np.isfinite(tx).all()
    assert not np.allclose(det[:, 0], det[:, 1])


def test_golden_vectors_are_deterministic_and_modes_differ():
    a, b = oa.golden_mix(False), oa.golden_mix(False)
    assert a == b and len(a) == oa.GOLDEN_FRAMES
    assert oa.golden_mix(True) != a


def test_cli_golden_prints_rust_constants(capsys):
    assert oa.main(["golden"]) == 0
    out = capsys.readouterr().out
    assert "COPY_INCS" in out and "OUT_EXACT" in out and "OUT_DETUNED" in out
