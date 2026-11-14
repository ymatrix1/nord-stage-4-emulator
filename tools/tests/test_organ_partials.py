"""Unit tests for tools/organ_partials.py (synthetic signals; no oracle)."""

from __future__ import annotations

import numpy as np
import organ_partials as p
import pytest


def test_b3_cycles_follow_the_ladder_and_the_two_fine_words() -> None:
    # octave doubling is exact, the ARM word is 17.11 cent above the image word
    assert p.b3_cycles_oracle(72) == pytest.approx(2 * p.b3_cycles_oracle(60), rel=1e-12)
    cents = 1200 * np.log2(p.b3_cycles_oracle(60) / (p.b3_step(60, p.FINE_IMAGE) / 2**33))
    assert cents == pytest.approx(17.11, abs=0.01)
    # Rust anchor: note 60 is the exact C4 at the firmware frame rate
    assert p.b3_cycles_rust(60) * p.FRAME_RATE == pytest.approx(261.6256, abs=1e-3)


def test_wheel_notes_fold_and_follow_the_drawbars() -> None:
    assert p.b3_notes([60], "888000000") == [48, 60, 67]
    assert p.b3_notes([60], "000000008") == [96]
    assert p.b3_notes([24], "800000000") == [36]  # 16' of the lowest keys folds up


def test_vox_rows_follow_the_slot_ladder_and_drop_the_sine_buckets() -> None:
    assert p.vox_rows([60]) == [24, 36, 48, 60]
    assert p.vox_rows([108]) == [72]  # the other three footages fold to rows 84..95
    assert p.vox_cycles(0) == pytest.approx(0.5 * p.VOX_RATE_BASE / 2**32)


def test_line_amplitude_and_compare_find_a_level_error() -> None:
    n, c = 8192, 0.0123
    t = np.arange(n)
    ref = 0.01 * np.sin(2 * np.pi * c * t)
    assert p.line_amplitude(ref, c) == pytest.approx(0.01, rel=2e-3)
    res = p.compare(ref, 2 * ref, [c], [c])
    assert res["dev_db"][0, 0] == pytest.approx(6.02, abs=0.05)
    assert res["valid"][0, 0] and not res["valid"][0, 1]


def test_table_skips_lines_above_nyquist() -> None:
    t = p.table(np.zeros(1024), [0.3])
    assert t.shape == (1, p.HARMONICS) and not t.any()
