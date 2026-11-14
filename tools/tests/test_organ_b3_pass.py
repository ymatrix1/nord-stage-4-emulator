"""Unit tests for tools/organ_b3_pass.py (synthetic signals and messages; no firmware)."""

from __future__ import annotations

import struct
from pathlib import Path

import numpy as np
import organ_b3_pass as bp


def msg(chip: str, core: int, path: int, dm: int, w2: int) -> dict:
    return {"chip": chip, "core": core, "path": path, "dm": f"{dm:#x}", "w2": f"{w2:#x}"}


def test_last_words_keeps_only_pno_core1_path2_and_the_last_value() -> None:
    rows = [
        msg("PNO", 1, 2, 0xC01D6, 1),
        msg("PNO", 1, 2, 0xC01D6, 2),
        msg("PNO", 2, 2, 0xC01D6, 9),
        msg("SMP", 0, 1, 0xC01D6, 7),
    ]
    assert bp.last_words(rows) == {0xC01D6: 2}


def test_slot_steps_follow_the_slot_table_and_skip_idle_slots() -> None:
    words = {
        0xC01DA: 0x121BBE,
        0xC01DB: 0xC00F2,
        0xC01DC: 0xB52C7,
        0xC01DE: 0x121B33,
        0xC01E6: 0x121E2B,
        0xC01E7: 0xC00EA,
        0xC01E8: 0xB472C,
    }
    assert bp.slot_steps(words) == [
        "0x121bbe:0x121e28:I0=0xc00f2,I1=0xb52c7",
        "0x121e2b:0x122293:I0=0xc00ea,I1=0xb472c",
    ]


def test_slot_steps_fall_back_to_the_image_words() -> None:
    steps = bp.slot_steps({}, {0xC01D6: 0x122A71, 0xC01D7: 1, 0xC01D8: 2})
    assert steps == ["0x122a71:0x122b84:I0=0x1,I1=0x2"]


def test_script_and_command_shape() -> None:
    assert bp.script_text({0x20: 5, 0x10: 1}) == "0 0x10 0x1\n0 0x20 0x5"
    cmd = bp.build_command("exe", "img", "scr", ["a:b"], 3, "out")
    assert cmd[:6] == ["exe", "img", "--calls", "3", "--script", "scr"]
    assert "--step" in cmd and cmd[-2:] == ["--out", "out"]
    assert f"{bp.RING_BASE:#x}:{bp.RING_WORDS}" in cmd


def tones(bins: list[int], n: int = 4800, amp: float = 0.3) -> np.ndarray:
    t = np.arange(n)
    return np.stack([amp * np.sin(2 * np.pi * b * t / (n - 200)) for b in bins], axis=1)


def test_peaks_find_the_bin_of_each_tone_column() -> None:
    x = tones([27, 55, 82])
    found = bp.peaks(x, 0x100)
    assert [p.address for p in found] == [0x100, 0x101, 0x102]
    assert [p.bin for p in found] == [27, 55, 82]
    assert all(abs(p.rms - 0.3 / 2**0.5) < 0.01 for p in found)


def test_silent_columns_are_dropped() -> None:
    x = np.concatenate([tones([40]), np.zeros((4800, 1))], axis=1)
    assert [p.address for p in bp.peaks(x, 0)] == [0]


def test_the_hammond_drawbar_series_matches_its_own_partials() -> None:
    bins = [27, 55, 82, 110, 165, 220, 277, 329, 439]
    found = bp.peaks(tones(bins), 0)
    assert bp.matches_series(found, bp.HAMMOND_SERIES)
    assert [round(r, 1) for r in bp.ratios(found)][:4] == [1.0, 2.0, 3.0, 4.1]


def test_a_non_hammond_series_does_not_match() -> None:
    found = bp.peaks(tones([10, 20, 30, 40, 50, 60, 70, 80, 90]), 0)
    assert not bp.matches_series(found, bp.HAMMOND_SERIES)


def test_image_words_read_u32_at_four_times_the_word_address(tmp_path: Path) -> None:
    data = bytearray(4 * (bp.SLOT_TABLE_DM + 40))
    struct.pack_into("<I", data, 4 * bp.SLOT_TABLE_DM, 0x121BBE)
    p = tmp_path / "img.bin"
    p.write_bytes(bytes(data))
    assert bp.image_words(str(p))[bp.SLOT_TABLE_DM] == 0x121BBE
