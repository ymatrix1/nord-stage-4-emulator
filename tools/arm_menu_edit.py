#!/usr/bin/env python3
"""Edit the values of every menu page of the host firmware and record what the OLED shows.

Fifth pass answer to "menu values cannot be edited": the Program dial *does* edit the value of the
selected menu item. In the harness it paged instead because the modifier word the key handler
keeps (`arm_screens.MODIFIER_WORD`) has bit 0 set after a Shift press and release, and the menu
handler `0xC014EEC8` pages with the dial while that bit is set. `arm_screens.edit` clears the word
before each turn; no parameter store, DSP or SPI emulation is needed (research note s16).

The chain for one detent (all VERIFIED by hooks, addresses in `docs/research/arm-oled-render.md`
s16): dial event id 6 -> menu screen handler `0xC014EEC8` -> page container `0xC012DEC0` -> item
adjust `0xC0134FA0` -> settings adjust `0xC00DFA00` (current value + delta, clamped to the range
table) -> set `0xC003B590` -> the item redraws its value text.

    uv run --with unicorn python tools/arm_menu_edit.py target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --state target/oled/state.bin --menu system

Frames and strings are firmware output: they go under `target/` (git-ignored), never into git.
"""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass, field
from pathlib import Path

import arm_screens as sc

# VERIFIED (research s12.1): Shift + Program n opens the menu; the page counts are the `n/N` of the
# title bar (s12.3). The Page key steps to the next page.
PROGRAM_KEYS: dict[int, tuple[int, int, int]] = {
    1: (0x10, 0, 2),
    2: (0x10, 0, 3),
    3: (0x08, 0, 2),
    4: (0x04, 0, 2),
    5: (0x10, 0, 4),
    6: (0x10, 0, 5),
    7: (0x08, 0, 4),
    8: (0x04, 0, 4),
}
PAGE_KEY = (0x08, 0, 6)
MENUS: dict[str, tuple[int, int]] = {
    "system": (1, 6),
    "sound": (2, 11),
    "auxkb": (4, 3),
    "output": (5, 6),
    "pedal": (6, 7),
    "midi": (7, 10),
    "extern": (8, 13),
}
# EMPIRICAL: three detents change every value of the range tables read so far by at least one step.
DETENTS = 3


@dataclass
class PageResult:
    """Strings the OLED text widgets received for one menu page."""

    page: int
    base: list[str] = field(default_factory=list)
    up: list[str] = field(default_factory=list)
    down: list[str] = field(default_factory=list)

    def changed(self) -> bool:
        """True when editing produced a value string that the untouched page did not show."""
        return any(s not in self.base for s in self.up + self.down)


def capture_page(session, page: int, detents: int = DETENTS, out: Path | None = None) -> PageResult:  # noqa: ANN001
    """Edit the open menu page up and back down; `session` is an `arm_screens.Session`."""
    result = PageResult(page)
    up = sc.edit(session, 1, detents)
    result.up = up.strings()
    if out is not None:
        save_screen(session, out, f"p{page:02d}_up")
    down = sc.edit(session, 1, -2 * detents)
    result.down = down.strings()
    if out is not None:
        save_screen(session, out, f"p{page:02d}_down")
    return result


def save_screen(session, out: Path, name: str, scale: int = 2) -> None:  # noqa: ANN001
    """Write the current controller RAM of both panels (PNG + ASCII); partial updates included."""
    import arm_oled as ao

    out.mkdir(parents=True, exist_ok=True)
    for i, panel in enumerate(session.link.panels):
        ram = bytes(panel.ram)
        (out / f"{name}_d{i}.png").write_bytes(ao.to_png(ram, scale=scale))
        (out / f"{name}_d{i}.txt").write_text(ao.to_ascii(ram))


def capture_menu(session, name: str, out: Path | None = None) -> list[PageResult]:  # noqa: ANN001
    """Open menu `name`, visit all its pages and edit each; returns one result per page."""
    program, pages = MENUS[name]
    opened = session.press([PROGRAM_KEYS[program], sc.SHIFT_KEY], hold=100)
    results: list[PageResult] = []
    for page in range(1, pages + 1):
        if page > 1:
            paged = session.press([PAGE_KEY], hold=60)
            base = paged.strings()
        else:
            base = opened.strings()
        if out is not None:
            save_screen(session, out, f"p{page:02d}_base")
        res = capture_page(session, page, out=out)
        res.base = base
        results.append(res)
    return results


def to_json(results: dict[str, list[PageResult]]) -> str:
    rows = {
        name: [{"page": r.page, "base": r.base, "up": r.up, "down": r.down} for r in pages]
        for name, pages in results.items()
    }
    return json.dumps(rows, indent=1)


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - needs the local firmware
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path)
    ap.add_argument("--state", type=Path, default=Path("target/oled/state.bin"))
    ap.add_argument("--menu", action="append", choices=sorted(MENUS), help="default: all")
    ap.add_argument("--out", type=Path, default=Path("target/oled/menus"))
    ns = ap.parse_args(argv)
    session = sc.Session(ns.image, ns.sram)
    session.load_state(ns.state.read_bytes())
    results: dict[str, list[PageResult]] = {}
    for name in ns.menu or list(MENUS):
        session.reset()
        results[name] = capture_menu(session, name, ns.out / name)
        for r in results[name]:
            print(
                name, r.page, "base", r.base[:3], "up", r.up[-3:], "down", r.down[-3:], flush=True
            )
    ns.out.mkdir(parents=True, exist_ok=True)
    (ns.out / "menus.json").write_text(to_json(results))
    return 0


if __name__ == "__main__":
    sys.exit(main())
