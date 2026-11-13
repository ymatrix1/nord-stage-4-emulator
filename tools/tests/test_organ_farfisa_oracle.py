"""Unit tests for tools/organ_farfisa_oracle.py (pure helpers; the oracle itself is not run)."""

from __future__ import annotations

from pathlib import Path

import numpy as np
import organ_farfisa_oracle as fo


def test_u2f_and_image_word_round_trip() -> None:
    assert fo.u2f(0x3F800000) == 1.0
    data = (0x3F800000).to_bytes(4, "little") * 3
    assert fo.image_word(data, 2) == 0x3F800000


def test_slots_cover_98_ring_words_with_nine_rows_for_two_oscillators() -> None:
    s = fo.slots()
    assert len(s) == 98
    assert (8, 0) in s and (8, 1) in s and (8, 2) not in s and (7, 11) in s


def test_spectrum_error_is_zero_for_identical_and_positive_for_changed_signals() -> None:
    t = np.arange(4096)
    x = np.sin(2 * np.pi * 0.01 * t) + 0.01 * np.sin(2 * np.pi * 0.03 * t)
    assert fo.spectrum_error_db(x, x) < 1e-9
    y = x + 0.01 * np.sin(2 * np.pi * 0.01 * t)  # raises the main line by about 0.09 dB
    assert 0.05 < fo.spectrum_error_db(x, y) < 0.2


def test_read_rust_csv_places_values_by_frame_row_oscillator(tmp_path: Path) -> None:
    f = tmp_path / "r.csv"
    f.write_text("0,1,2,0.5\n3,8,1,-0.25\n9,0,0,1.0\n", encoding="utf-8")
    out = fo.read_rust_csv(str(f), 4)  # frame 9 is beyond the 4 requested frames
    assert out.shape == (4, 9, 12)
    assert out[0, 1, 2] == 0.5 and out[3, 8, 1] == -0.25 and out[:, 0, 0].sum() == 0


def test_report_prints_snr_and_spectral_error(capsys) -> None:  # type: ignore[no-untyped-def]
    rng = np.random.default_rng(1)
    ref = rng.uniform(-1, 1, (512, 9, 12)).astype(np.float32)
    fo.report("same", ref, ref.copy())
    text = capsys.readouterr().out
    assert "98 slots" in text and "max |diff| 0" in text


def test_golden_text_has_the_expected_shape(monkeypatch, capsys) -> None:  # type: ignore[no-untyped-def]
    fake = {f: np.full((9, 12), 0.5, dtype=np.float32) for f in fo.GOLDEN_FRAMES}
    monkeypatch.setattr(fo, "golden_rows", lambda: fake)
    assert fo.cmd_golden(None) == 0  # type: ignore[arg-type]
    text = capsys.readouterr().out
    assert "pub(super) const FRAMES: [usize; 3] = [200, 1999, 4001];" in text
    assert text.count("[0.5, 0.5, 0.5],") == 27
    assert "fn golden_vectors_are_bounded_pulses" in text
