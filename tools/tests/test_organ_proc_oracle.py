"""Unit tests for tools/organ_proc_oracle.py (synthetic scripts and signals; no firmware)."""

from __future__ import annotations

import numpy as np
import organ_proc_oracle as po


def test_farfisa_keydown_matches_the_recorded_key_60_words() -> None:
    # keydown-organ-models.csv, model 2, note 60 (tick order: weight, companion, ...)
    assert po.farfisa_keydown([60]) == [
        "0 0x96020 0x3caaaa80",
        "0 0xc2d58 0xbc1d5580",
        "0 0x96082 0x3caaaa80",
        "0 0xc2d59 0xbc1d5580",
        "0 0x960f0 0x3caaaa80",
        "0 0xc2d5c 0xbc1d5580",
    ]


def test_farfisa_keydown_matches_recorded_notes_36_and_84() -> None:
    assert po.farfisa_keydown([36])[::2] == [
        f"0 {a:#x} 0x3caaaa80" for a in (0x96008, 0x9606A, 0x960D8)
    ]
    assert po.farfisa_keydown([84])[::2] == [
        f"0 {a:#x} 0x3caaaa80" for a in (0x96038, 0x9609A, 0x96108)
    ]


def test_sentinels_are_stripped_and_other_words_kept() -> None:
    lines = ["0 0xb2ecf 0xbda3d70b", "0 0x95b3f 0x0", "0 0xb2ff0 0xbd851eb8"]
    assert po.strip_sentinels(lines) == ["0 0x95b3f 0x0"]


def test_build_script_farfisa_uses_boot_install_and_keydown() -> None:
    boot = ["0 0xb2ecf 0x1", "0 0x10 0x2"]
    capture = [*boot] + [f"0 {0x100 + i:#x} 0x0" for i in range(po.INSTALL_WORDS[2] + 5)]
    out = po.build_script(2, boot, capture, [60])
    assert out[0] == "0 0x10 0x2"
    assert len(out) == 1 + po.INSTALL_WORDS[2] + 6
    assert out[-6:] == po.farfisa_keydown([60])


def test_build_script_bass_keeps_the_captured_keydown() -> None:
    boot = ["0 0x10 0x2"]
    capture = [*boot, "0 0x951f1 0x3", "0 0x951f2 0x4"]
    assert po.build_script(5, boot, capture, [60]) == [
        "0 0x10 0x2",
        "0 0x951f1 0x3",
        "0 0x951f2 0x4",
    ]


def test_chord_lines_find_the_tones() -> None:
    n = 4096
    t = np.arange(n)
    x = np.sin(2 * np.pi * 64 * t / n) + 0.5 * np.sin(2 * np.pi * 128 * t / n)
    lines = po.chord_lines(x, 2)
    assert [round(h / (po.fb.DSP_FRAME_RATE / n)) for h, _ in lines] == [64, 128]
    assert abs(lines[1][1] + 6.0) < 0.5
