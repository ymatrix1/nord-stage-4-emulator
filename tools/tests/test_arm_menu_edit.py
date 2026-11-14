"""Unit tests for tools/arm_menu_edit.py: menu tables, page walk and result summaries."""

from __future__ import annotations

import json
from pathlib import Path

import arm_menu_edit as me
import arm_screens as sc


def test_menu_table_matches_the_title_bar_page_counts() -> None:
    assert {n: p for n, (_, p) in me.MENUS.items()} == {
        "system": 6,
        "sound": 11,
        "auxkb": 3,
        "output": 6,
        "pedal": 7,
        "midi": 10,
        "extern": 13,
    }
    # Shift + Program n for the seven menus (Program 3 is Organize, not a settings menu).
    assert sorted(p for p, _ in me.MENUS.values()) == [1, 2, 4, 5, 6, 7, 8]
    assert set(me.PROGRAM_KEYS) == set(range(1, 9))
    assert me.PROGRAM_KEYS[1] == (0x10, 0, 2) and me.PAGE_KEY == (0x08, 0, 6)


def test_a_page_result_reports_a_change_only_for_new_value_strings() -> None:
    same = me.PageResult(1, base=["Memory Protect", "On"], up=["On"], down=[])
    assert not same.changed()
    moved = me.PageResult(1, base=["Memory Protect", "On"], up=["On"], down=["Off"])
    assert moved.changed()


class _FakeSession:
    """Records the calls `capture_menu` makes and answers with canned strings."""

    def __init__(self) -> None:
        self.calls: list[tuple[str, object]] = []
        self.link = type("L", (), {"panels": ()})()
        self.emu = self
        self.writes: list[tuple[int, int]] = []

    def write32(self, address: int, value: int) -> None:
        self.writes.append((address, value))

    def press(self, keys, hold=40, **_kw):  # noqa: ANN001, ANN003, ANN201
        self.calls.append(("press", tuple(keys)))
        return sc.Capture(([], []), [sc.TextEvent(0, 0, False, f"page{len(self.calls)}")])

    def turn(self, pair, detents, **_kw):  # noqa: ANN001, ANN003, ANN201
        self.calls.append(("turn", (pair, detents)))
        return sc.Capture(([], []), [sc.TextEvent(0, 0, False, f"v{detents}")])


def test_capture_menu_opens_with_shift_pages_and_edits_each_page() -> None:
    session = _FakeSession()
    results = me.capture_menu(session, "auxkb")
    assert [r.page for r in results] == [1, 2, 3]
    assert session.calls[0] == ("press", ((0x04, 0, 2), sc.SHIFT_KEY))
    presses = [c for c in session.calls if c[0] == "press"]
    assert presses[1:] == [("press", (me.PAGE_KEY,))] * 2
    turns = [c[1] for c in session.calls if c[0] == "turn"]
    assert turns == [(1, me.DETENTS), (1, -2 * me.DETENTS)] * 3
    assert results[0].up == [f"v{me.DETENTS}"] and results[0].down == [f"v{-2 * me.DETENTS}"]
    # every edit clears the modifier word first (arm_screens.edit)
    assert session.writes == [(sc.MODIFIER_WORD, 0)] * 6


def test_to_json_lists_every_page_of_every_menu() -> None:
    data = json.loads(me.to_json({"system": [me.PageResult(1, ["a"], ["b"], ["c"])]}))
    assert data == {"system": [{"page": 1, "base": ["a"], "up": ["b"], "down": ["c"]}]}


def test_save_screen_writes_png_and_ascii_for_both_panels(tmp_path: Path) -> None:
    import arm_oled as ao

    link = ao.PanelLink()
    link.panels[0].ram[0] = 0xFF
    session = type("S", (), {"link": link})()
    me.save_screen(session, tmp_path, "x")
    assert sorted(p.name for p in tmp_path.iterdir()) == [
        "x_d0.png",
        "x_d0.txt",
        "x_d1.png",
        "x_d1.txt",
    ]
    assert (tmp_path / "x_d0.png").read_bytes().startswith(b"\x89PNG")
