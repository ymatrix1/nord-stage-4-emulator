#!/usr/bin/env python3
"""The three Synth encoders: which pairs they are, and what the OSC WAVEFORM page shows for them.

Fifth pass (research note s16.4). Encoder pairs 2, 3 and 4 (event ids 15, 14, 13, `arm_keys`) are
the three dials under the Synth display. They only act while the Synth waveform page (key
`(0x01, 1, 6)`) is open; on every other screen the events are dispatched and ignored. On that page
they step the waveform selection of the three oscillator columns (a category, a wave within it
and a shape / mix value; strings such as `FM Harmonic A` / `Amount:`, `Sub Square Saw` / `Mix:`,
`Pure Square` are the widget texts) on display 1.

    uv run --with unicorn python tools/arm_synth_dials.py target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --state target/oled/state.bin

Frames and strings are firmware output: they go under `target/` (git-ignored).
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import arm_menu_edit as me
import arm_screens as sc

SYNTH_PAGE_KEY = (0x01, 1, 6)
# VERIFIED (research s16.4): encoder pair -> what it steps on the Synth waveform page.
SYNTH_DIAL_PAIRS: dict[int, str] = {2: "left", 3: "middle", 4: "right"}
DETENTS = 3


def dial_summary(strings: list[str]) -> list[str]:
    """Widget strings of a dial turn without the empty and one-glyph icon strings."""
    return [s for s in strings if len(s.strip()) > 1]


def capture_dials(session, out: Path | None = None, detents: int = DETENTS) -> dict[str, dict]:  # noqa: ANN001
    """Open the Synth page, turn each of the three dials up and down; strings and frames."""
    session.press([SYNTH_PAGE_KEY], hold=60)
    if out is not None:
        me.save_screen(session, out, "synth_base")
    result: dict[str, dict] = {}
    for pair, name in SYNTH_DIAL_PAIRS.items():
        up = sc.edit(session, pair, detents)
        if out is not None:
            me.save_screen(session, out, f"{name}_up")
        down = sc.edit(session, pair, -detents)
        if out is not None:
            me.save_screen(session, out, f"{name}_down")
        result[name] = {"up": dial_summary(up.strings()), "down": dial_summary(down.strings())}
    return result


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - needs the local firmware
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path)
    ap.add_argument("--state", type=Path, default=Path("target/oled/state.bin"))
    ap.add_argument("--out", type=Path, default=Path("target/oled/synth_dials"))
    ns = ap.parse_args(argv)
    session = sc.Session(ns.image, ns.sram)
    session.load_state(ns.state.read_bytes())
    result = capture_dials(session, ns.out)
    ns.out.mkdir(parents=True, exist_ok=True)
    (ns.out / "dials.json").write_text(json.dumps(result, indent=1))
    for name, r in result.items():
        print(name, r)
    return 0


if __name__ == "__main__":
    sys.exit(main())
