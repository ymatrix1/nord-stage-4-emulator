#!/usr/bin/env python3
"""Replay the ARM organ weight bursts into the PNO core 1 image and measure the wheel amplitudes.

Phases (each `PHASE_CALLS` calls of 2 frames, bursts poked before the first call of a phase):

    0  organ model 3 + keys down + drawbars A          (`--model3`, `--keys-a` scripts)
    1  drawbar change to B                             (`--keys-b`, update burst only)
    2  vibrato on  (detuned bank, gains 0.75 / 0.375)  (`--vibrato-on`)
    3  vibrato off (exact bank, `--model3` again)

    python tools/organ_weights_replay.py SRAM/core1_sram.bin --exe target/release/sharc-organ \
        --model3 docs/research/data/arm-replay-organ-model3.script \
        --keys-a docs/research/data/arm-replay-organ-keys-a.script \
        --keys-b docs/research/data/arm-replay-organ-keys-b.script \
        --vibrato-on docs/research/data/arm-replay-organ-vibrato-on.script --csv phases.csv

Method and results: `docs/research/organ-weights.md` section 6.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

import numpy as np
import organ_oracle_audio as oa

PHASE_CALLS = 3000
SETTLE_CALLS = 1500  # calls skipped at the start of a phase before measuring (smoothers, 1/a)
# ASSUMED: the level target L (0x95B42) is not sent by any captured ARM message; 1.0 as in
# organ-oracle-audio.md section 4.
LEVEL_POKE = (oa.LEVEL, oa.ONE)
LINE = re.compile(r"^\s*(\d+)\s+(0x[0-9a-fA-F]+|\d+)\s+(0x[0-9a-fA-F]+|\d+)")


def parse_script(text: str, shift: int = 0) -> list[tuple[int, int, int]]:
    """`CALL ADDR VALUE  # comment` lines to `(call + shift, addr, value)`; others are skipped."""
    out = []
    for line in text.splitlines():
        m = LINE.match(line)
        if m:
            out.append((int(m.group(1)) + shift, int(m.group(2), 0), int(m.group(3), 0)))
    return out


def gain_pokes(gains: tuple[int, int, int, int], call: int) -> list[tuple[int, int, int]]:
    """Group gain pokes (24-bit ints over `0x800000`) for set 0, as `build_pokes` writes them."""
    return [
        (call, addr, oa.f2u(g / 0x800000))
        for addr, g in zip((oa.GAIN_X, oa.GAIN_Y, oa.CROSS_YX, oa.CROSS_XY), gains, strict=True)
    ]


def compose(scripts: dict[str, str]) -> list[tuple[int, int, int]]:
    """Poke list of the four phases from the script texts (keys `model3`, `keys_a`, `keys_b`,
    `vibrato_on`)."""
    p = PHASE_CALLS
    pokes = parse_script(scripts["model3"]) + parse_script(scripts["keys_a"])
    pokes += [(0, *LEVEL_POKE)]
    pokes += parse_script(scripts["keys_b"], p)
    pokes += parse_script(scripts["vibrato_on"], 2 * p) + gain_pokes(oa.GAINS_DETUNED, 2 * p)
    pokes += parse_script(scripts["model3"], 3 * p)
    return pokes


def wheel_weights(pokes: list[tuple[int, int, int]], phase: int) -> np.ndarray:
    """`wB` of set 0 in force during `phase` (last poke at or before the phase start)."""
    w = np.zeros(oa.WHEELS)
    for call, addr, value in sorted(pokes, key=lambda x: x[0]):
        if call <= phase * PHASE_CALLS and oa.WB_BASE <= addr < oa.WB_BASE + oa.WHEELS:
            w[addr - oa.WB_BASE] = np.frombuffer(np.uint32(value).tobytes(), dtype="<f4")[0]
    return w


def measure(
    out: np.ndarray, weights: np.ndarray, incs: list[int], phase: int
) -> list[tuple[int, float, float, float]]:
    """`(wheel, wB, amplitude lane x, amplitude lane y)` in the settled part of `phase`."""
    lo = 2 * (phase * PHASE_CALLS + SETTLE_CALLS)
    seg = out[lo : 2 * (phase + 1) * PHASE_CALLS]
    rows = oa.partial_table(seg[:, 0], weights, incs)
    return [(i, wt, amp, oa.amplitude(seg[:, 1], c)) for i, wt, c, amp in rows]


def phase_csv(rows: dict[int, list[tuple[int, float, float, float]]]) -> str:
    lines = ["phase,wheel,wB,amp_x,amp_y,amp_x_per_wB"]
    for ph, rs in sorted(rows.items()):
        lines += [f"{ph},{i},{wt:.9g},{ax:.6g},{ay:.6g},{ax / wt:.6g}" for i, wt, ax, ay in rs]
    return "\n".join(lines) + "\n"


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image")
    ap.add_argument("--exe", required=True)
    for name in ("model3", "keys-a", "keys-b", "vibrato-on"):
        ap.add_argument(f"--{name}", type=Path, required=True)
    ap.add_argument("--csv", type=Path)
    ns = ap.parse_args(argv)
    texts = {
        "model3": ns.model3.read_text(encoding="utf-8"),
        "keys_a": ns.keys_a.read_text(encoding="utf-8"),
        "keys_b": ns.keys_b.read_text(encoding="utf-8"),
        "vibrato_on": ns.vibrato_on.read_text(encoding="utf-8"),
    }
    pokes = compose(texts)
    calls = 4 * PHASE_CALLS
    out = oa.run_oracle(ns.exe, ns.image, pokes, calls)
    rows = {}
    for ph in range(4):
        incs = oa.increments(detuned=ph == 2)
        rows[ph] = measure(out, wheel_weights(pokes, ph), incs, ph)
        seg = out[2 * ph * PHASE_CALLS : 2 * (ph + 1) * PHASE_CALLS]
        print(f"phase {ph}: peak {np.abs(seg).max():.5f}")
    if ns.csv:
        ns.csv.write_text(phase_csv(rows), encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
