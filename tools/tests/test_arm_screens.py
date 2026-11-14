"""Unit tests for tools/arm_screens.py: the pure helpers and the session glue on fakes."""

from __future__ import annotations

import json
import pickle
import zlib
from pathlib import Path

import arm_keys as ak
import arm_oled as ao
import arm_screens as sc
import pytest


def _frame(fill: int) -> bytes:
    return bytes([fill]) * ao.FRAME_BYTES


def test_distinct_keeps_order_and_drops_repeats() -> None:
    a, b = _frame(1), _frame(2)
    assert sc.distinct([a, b, a, b, a]) == [a, b]
    assert sc.distinct([]) == []


def test_parse_key_accepts_hex_and_decimal() -> None:
    assert sc.parse_key("0x01,0,3") == (1, 0, 3)
    assert sc.parse_key("0x40, 0 , 0x7") == (0x40, 0, 7)


def test_capture_final_frame_and_distinct_strings() -> None:
    texts = [
        sc.TextEvent(1, 0xC0, False, "Delay"),
        sc.TextEvent(2, 0xC0, True, "ON"),
        sc.TextEvent(1, 0xC0, False, "Delay"),
    ]
    cap = sc.Capture(([_frame(1), _frame(2)], []), texts)
    assert cap.final(0) == _frame(2)
    assert cap.final(1) is None
    assert cap.strings() == ["Delay", "ON"]


def test_split_regions_round_trips_a_snapshot_blob() -> None:
    raw = b"\x01\x02\x03\x04"
    blob = zlib.compress(
        pickle.dumps(([(0x1000, 4, 7, zlib.compress(raw))], {"regs": list(range(16))})), 1
    )
    regions, state = sc.split_regions(blob)
    assert regions == [(0x1000, 4, raw)]
    assert state["regs"][15] == 15


def test_save_capture_writes_pngs_ascii_and_strings(tmp_path: Path) -> None:
    cap = sc.Capture(([_frame(0xFF)], []), [sc.TextEvent(1, 0xC0, True, "A:11")])
    sc.save_capture(cap, tmp_path, "k")
    assert (tmp_path / "k_d0.png").read_bytes().startswith(b"\x89PNG")
    assert (tmp_path / "k_d0.txt").read_text().startswith("#")
    assert not (tmp_path / "k_d1.png").exists()
    rows = json.loads((tmp_path / "k.json").read_text())
    assert rows == [{"font": "0xc0", "upper": True, "text": "A:11"}]


class _FakeEmu:
    def read_bytes(self, address: int, size: int) -> bytes:
        assert address == sc.LATCH
        return bytes([0x36])


def test_sync_panels_starts_the_models_from_the_firmware_shadow() -> None:
    session = sc.Session.__new__(sc.Session)
    session.emu = _FakeEmu()
    session.link = ao.PanelLink()
    for panel in session.link.panels:
        panel.page, panel.col, panel._burst = 3, 9, 100
    session.sync_panels()
    assert session.link.latch == 0x36
    assert all(
        p.mode == 0 and p.page == 0 and p.col == 0 and p._burst == 0 for p in session.link.panels
    )


def test_press_holds_then_releases_and_slices_new_output() -> None:
    session = sc.Session.__new__(sc.Session)
    session.keys = ak.KeyMatrix()
    session.link = ao.PanelLink()
    session.texts = [sc.TextEvent(0, 0, False, "old")]
    seen: list[bool] = []

    def fake_run(ticks: int) -> None:
        seen.append((0x01, 0, 3) in session.keys.held)
        if ticks == 40 and not session.keys.held:  # after the release: a frame and a string arrive
            session.link.panels[0].frames.append(_frame(5))
            session.texts.append(sc.TextEvent(0, 0, False, "new"))

    session.run = fake_run  # type: ignore[method-assign]
    cap = session.press([(0x01, 0, 3)], hold=30, after=40, before=20)
    assert seen == [False, True, False]
    assert cap.frames[0] == [_frame(5)] and cap.frames[1] == []
    assert cap.strings() == ["new"]
    assert not session.keys.held


def test_reset_requires_a_loaded_state() -> None:
    session = sc.Session.__new__(sc.Session)
    session._saved = None
    with pytest.raises(AssertionError):
        session.reset()
