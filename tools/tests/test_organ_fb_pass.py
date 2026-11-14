"""Unit tests for tools/organ_fb_pass.py (synthetic messages and signals; no firmware)."""

from __future__ import annotations

import numpy as np
import organ_fb_pass as fb


def msg(chip: str, core: int, path: int, dm: int, w2: int) -> dict:
    return {"chip": chip, "core": core, "path": path, "dm": f"{dm:#x}", "w2": f"{w2:#x}"}


def test_script_lines_keep_pno_core1_path2_in_send_order() -> None:
    rows = [
        msg("PNO", 1, 2, 0x951F1, 0x3C4AE700),
        msg("FX", 1, 2, 5, 6),
        msg("PNO", 2, 2, 7, 8),
        msg("PNO", 1, 2, 0xC01D6, 0x122296),
    ]
    assert fb.script_lines(rows) == ["0 0x951f1 0x3c4ae700", "0 0xc01d6 0x00122296"]


def test_lane_series_takes_alternate_words_of_one_pair() -> None:
    raw = np.zeros((5, 6, 3, 4), dtype=np.float32)
    raw[:, 2, 0, :] = [1, 2, 3, 4]  # L0 R0 L1 R1
    assert fb.lane_series(raw.reshape(-1), 2, 0, 0).tolist() == [1, 3] * 5
    assert fb.lane_series(raw.reshape(-1), 2, 0, 1).tolist() == [2, 4] * 5


def test_top_lines_find_the_tones_of_a_chord() -> None:
    n = 16384
    t = np.arange(n) / fb.DSP_FRAME_RATE
    x = np.sin(2 * np.pi * 261.6 * t) + 0.5 * np.sin(2 * np.pi * 329.6 * t)
    lines = fb.top_lines(x, 2)
    assert abs(lines[0].hz - 261.6) < 3 and abs(lines[1].hz - 329.6) < 3
    assert lines[0].level_db == 0 and -8 < lines[1].level_db < -4
    assert fb.top_lines(np.zeros(64)) == []


def test_lane_report_lists_only_audible_finite_lanes() -> None:
    n = 3000
    t = np.arange(2 * n) / fb.DSP_FRAME_RATE
    tone = (0.01 * np.sin(2 * np.pi * 440 * t)).astype(np.float32)
    raw = np.zeros((n, 6, 3, 4), dtype=np.float32)
    raw[:, 2, 0, 0::2] = tone.reshape(n, 2)  # set 2 pair 0 lane 0
    raw[:, 4, 1, 1] = np.nan  # non-finite lane is skipped
    text = fb.lane_report(raw.reshape(-1), skip=100)
    assert len(text) == 1 and text[0].startswith("set 2 pair 0 lane 0") and "441.5 Hz" in text[0]
    assert fb.lane_report(np.zeros(6 * 3 * 4 * 600, dtype=np.float32)) == []
