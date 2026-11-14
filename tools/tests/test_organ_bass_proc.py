"""Unit tests for tools/organ_bass_proc.py (script parsing and spectra; no oracle)."""

from __future__ import annotations

import numpy as np
import organ_bass_proc as bp


def test_script_words_keeps_the_last_value_and_skips_comments() -> None:
    words = bp.script_words(["0 0x10 0x1", "0 0x10 0x2  # later", "# c", "0 0x11 3"])
    assert words == {0x10: 2, 0x11: 3}


def test_zero_window_lines_cover_the_window() -> None:
    w1, w2 = bp.zero_window_lines(1), bp.zero_window_lines(2)
    assert len(w1) == bp.W1_WORDS and len(w2) == bp.W2_WORDS
    assert w1[0] == "0 0x951f1 0x0" and w2[-1] == f"0 {bp.W2_BASE + bp.W2_WORDS - 1:#x} 0x0"


def test_out_address_reads_i0_of_a_step() -> None:
    assert bp.out_address("0x122296:0x12261d:I0=0xc00ea,I1=0xb472c") == 0xC00EA


def test_output_frames_interleaves_words_0_and_2() -> None:
    w = np.array([[1, 9, 2, 0, 0], [3, 9, 4, 0, 0]], dtype=np.float32)
    assert list(bp.output_frames(w)) == [1, 2, 3, 4]


def test_top_lines_finds_the_tone_amplitude() -> None:
    n = 8192
    x = 0.25 * np.sin(2 * np.pi * 100 * np.arange(n) / n)
    hz, amp, db = bp.top_lines(x, 1)[0]
    assert abs(hz - 100 * bp.RATE / n) < 1e-6 and abs(amp - 0.25) < 0.01 and db == 0.0
