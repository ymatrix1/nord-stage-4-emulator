"""Unit tests for tools/pno_full_pass.py (lane decoding, lag/gain fit, script parsing)."""

from __future__ import annotations

import numpy as np
import pno_full_pass as pfp


def raw_pass(set_index: int, pair: int, quad: tuple[float, float, float, float]) -> np.ndarray:
    words = np.zeros(pfp.PASS_WORDS, dtype="<f4")
    base = (set_index * pfp.PAIRS + pair) * pfp.WORDS_PER_PAIR
    words[base : base + 4] = quad
    return words


def test_parse_raw_reads_the_pair_as_l_r_frames() -> None:
    data = np.concatenate(
        [raw_pass(2, 0, (1, 2, 3, 4)), raw_pass(2, 0, (5, 6, 7, 8)), raw_pass(1, 0, (9, 9, 9, 9))]
    ).tobytes()
    frames = pfp.parse_raw(data)
    assert frames.shape == (6, 2)
    assert frames[:4].tolist() == [[1, 2], [3, 4], [5, 6], [7, 8]]
    assert frames[4:].tolist() == [[0, 0], [0, 0]]
    other = pfp.parse_raw(data, set_index=1)
    assert other[4:].tolist() == [[9, 9], [9, 9]]


def test_best_fit_finds_lag_gain_and_snr() -> None:
    rng = np.random.default_rng(1)
    ref = rng.standard_normal((400, 2)).astype(np.float32)
    sig = np.roll(ref, 2, axis=0) * 0.5
    lag, gain, snr = pfp.best_fit(ref, sig, max_lag=4)
    assert lag == -2 and abs(gain - 0.5) < 1e-6 and snr > 100
    noisy = ref * 2.0 + 0.01 * rng.standard_normal(ref.shape).astype(np.float32)
    lag, gain, snr = pfp.best_fit(ref, noisy, max_lag=2)
    assert lag == 0 and abs(gain - 2.0) < 0.01 and 30 < snr < 80
    assert pfp.best_fit(np.zeros((50, 2)), np.ones((50, 2)), 2)[2] == -np.inf


def test_script_pokes_and_the_level_script_parse() -> None:
    assert pfp.level_script() == "0 0x95b42 0x3f800000  # level L (ASSUMED 1.0)\n"


def test_script_pokes_read_files(tmp_path) -> None:
    f = tmp_path / "a.script"
    f.write_text("0 0x10 0x20  # c\n\n3 0x11 7\nbad line\n", encoding="utf-8")
    assert pfp.script_pokes([str(f)]) == [(0, 0x10, 0x20), (3, 0x11, 7)]


def test_read_rust_csv_skips_the_header_comments(tmp_path) -> None:
    f = tmp_path / "r.csv"
    f.write_text("# mode: layer\n# rate: 1\nindex,L,R\n0,0.5,0.25\n1,1.0,2.0\n", encoding="utf-8")
    out = pfp.read_rust_csv(str(f))
    assert out.tolist() == [[0.5, 0.25], [1.0, 2.0]]
