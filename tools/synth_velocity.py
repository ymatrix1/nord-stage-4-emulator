#!/usr/bin/env python3
"""Synth key-down velocity byte: table, S-curve fit and the Rust table cross-check.

The ARM synth key handler sends byte 1 = `0x80 + g(vel)` (`docs/research/keydown-zones.md` s7).
The MIDI velocity first goes through the velocity curve table (curve 6 by default, value `c`),
`g` is a saturating function of `c`. This tool re-derives both from the committed CSVs:

    python tools/synth_velocity.py --check

and verifies that `KEY_VELOCITY_G` of `synth/key_velocity.rs` equals the ARM run.
"""

from __future__ import annotations

import argparse
import csv
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DATA = ROOT / "docs" / "research" / "data"
RUST_TABLE = ROOT / "crates" / "nord-core" / "src" / "synth" / "key_velocity.rs"
# EMPIRICAL: curve value at which the S-curve reaches 127 (grid search 560..660, 97 of 127 exact).
SATURATION = 600.0
CURVE_COLUMN = "curve6"


def load_g(path: Path = DATA / "keydown-synth-velocity.csv") -> list[int]:
    """`g(vel)` for velocity 1..127: byte 1 of the key word minus 0x80."""
    with path.open(encoding="utf-8") as fh:
        rows = list(csv.DictReader(fh))
    return [((int(r["value_on"], 16) >> 8) & 0xFF) - 0x80 for r in rows]


def load_curve(
    path: Path = DATA / "keydown-velocity-curve.csv", col: str = CURVE_COLUMN
) -> list[int]:
    """Velocity curve table values `c(vel)` for velocity 1..127."""
    with path.open(encoding="utf-8") as fh:
        return [int(r[col]) for r in csv.DictReader(fh)]


def smoothstep_g(c: float, saturation: float = SATURATION) -> int:
    """`floor(127 (3 x^2 - 2 x^3))`, `x = clamp(c / saturation)`."""
    x = min(max(c / saturation, 0.0), 1.0)
    return int(127.0 * x * x * (3.0 - 2.0 * x))


def fit_quality(g: list[int], curve: list[int]) -> tuple[int, int]:
    """`(exact matches, max abs error)` of the smoothstep fit over all velocities."""
    errs = [abs(smoothstep_g(c) - gv) for c, gv in zip(curve, g, strict=True)]
    return sum(e == 0 for e in errs), max(errs)


def parse_rust_table(text: str) -> list[int]:
    """The numbers of `KEY_VELOCITY_G: [u8; 127] = [ ... ];`."""
    m = re.search(r"KEY_VELOCITY_G:\s*\[u8;\s*127\]\s*=\s*\[(.*?)\];", text, re.S)
    if not m:
        raise ValueError("KEY_VELOCITY_G not found")
    return [int(x) for x in re.findall(r"\d+", m.group(1))]


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--check", action="store_true", help="print fit quality and Rust table match")
    ns = ap.parse_args(argv)
    g, curve = load_g(), load_curve()
    exact, worst = fit_quality(g, curve)
    print(f"{len(g)} velocities, smoothstep fit exact {exact}, max err {worst}")
    if ns.check:
        rust = parse_rust_table(RUST_TABLE.read_text(encoding="utf-8"))
        ok = rust == g
        print(f"Rust KEY_VELOCITY_G equals the ARM run: {ok}")
        return 0 if ok and worst <= 1 else 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
