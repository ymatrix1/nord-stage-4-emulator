#!/usr/bin/env python3
"""Band-limited sawtooth of the Core2 stage library (PNO core 2 / ORG core 2 helper `0x12501F`).

The helper that the synth strip's word-0 stage `0x1215FB` calls twice turns a Q32 phase `p0` and a
Q32 phase increment `inc` into four consecutive samples `k = 1..4` of a falling saw with a
two-sample quadratic PolyBLEP step and a high-pitch fade (model measured against the SHARC oracle
in `docs/research/synth-voice-control.md` section 5):

    p_k  = frac(p0 + k inc)                      phase of sample k, in cycles
    t_k  = signed distance of p_k to the wrap in increments: p / i (p < 0.5), (p - 1) / i (p >= 0.5)
    y_k  = g(i) amp (1 - 2 p_k + d(t_k)),        d(t) = -(1 - t/2)^2 for 0 <= t < 2,
                                                       +(1 + t/2)^2 for -2 < t < 0, else 0
    g(i) = C (B + clip(A - 2 i, +-B)),           i = inc / 2^32 cycles per sample

with the constants of the image `A = 0x3EB0F34D = 0.34560624`, `B = 0x3DEBEF11 = 0.11520208`,
`C = 0x408AE2EA = 4.3401995` (`B C 2 = 1.0000`): `g = 1` up to `i = (A - B) / 2 = 0.1152` cycles per
sample, linear fade to 0 at `i = (A + B) / 2 = 0.2304`.

Check against the oracle (needs the local ORG core 2 SRAM image and `sharc-run`):

    python tools/synth_saw_model.py check core2_sram.bin target/release/sharc-run
"""

from __future__ import annotations

import argparse
import struct
import subprocess
import sys
from pathlib import Path

# VERIFIED: image constants of the helper prologue (`R14 = 0x3eb0f34d`, `0x3debef11`, `0x408ae2ea`,
# listing `0x125023, 0x125028, 0x12502E` of ORG core 2); the formula reproduces the oracle
# output of `check` on 512 phase/increment pairs, max abs error 5.9e-8 (float32 rounding).
A = struct.unpack("<f", struct.pack("<I", 0x3EB0F34D))[0]
B = struct.unpack("<f", struct.pack("<I", 0x3DEBEF11))[0]
C = struct.unpack("<f", struct.pack("<I", 0x408AE2EA))[0]
HELPER_PC = 0x12501F
HELPER_STEPS = 74


def clip(x: float, limit: float) -> float:
    """SHARC `CLIP x BY y`: `x` limited to `+-|y|`."""
    m = abs(limit)
    return max(-m, min(m, x))


def fade(inc: float) -> float:
    """High-pitch fade `g` for an increment of `inc` cycles per sample."""
    return C * (B + clip(A - 2.0 * inc, B))


def blep_step(t: float) -> float:
    """Quadratic wrap correction `d(t)`; `t` in increments after (positive) or before the wrap."""
    if 0.0 <= t < 2.0:
        return -((1.0 - t / 2.0) ** 2)
    if -2.0 < t < 0.0:
        return (1.0 + t / 2.0) ** 2
    return 0.0


def saw_sample(phase: float, inc: float, amp: float = 1.0) -> float:
    """One sample at phase `phase` (cycles, any real) for increment `inc` (cycles per sample)."""
    p = phase % 1.0
    t = p / inc if p < 0.5 else (p - 1.0) / inc
    return fade(inc) * amp * (1.0 - 2.0 * p + blep_step(t))


def saw_block(phase0: float, inc: float, amp: float = 1.0) -> list[float]:
    """The four samples `k = 1..4` the helper returns in `R0..R3`."""
    return [saw_sample(phase0 + k * inc, inc, amp) for k in range(1, 5)]


def run_helper(sram: Path, sharc_run: Path, phase: int, inc: int, amp_bits: int = 0x3F800000):
    """`(R0..R3 as float, R4)` after one pass of the helper in the oracle (`sharc-run`)."""
    cmd = [
        str(sharc_run),
        str(sram),
        hex(HELPER_PC),
        str(HELPER_STEPS),
        "--set",
        f"R12={inc:#x}",
        "--set",
        f"R8={phase:#x}",
        "--set",
        "R10=0",
        "--set",
        f"R6={amp_bits:#x}",
        "--regs",
    ]
    out = subprocess.run(cmd, capture_output=True, text=True, check=True).stdout
    line = next(ln for ln in out.splitlines() if ln.startswith("R:"))
    regs = [int(x, 16) for x in line.split()[1:]]
    floats = [struct.unpack("<f", struct.pack("<I", regs[i]))[0] for i in range(4)]
    return floats, regs[4]


def check(sram: Path, sharc_run: Path) -> float:  # pragma: no cover - needs firmware and oracle
    """Worst absolute difference between model and oracle over a grid of phases and increments."""
    worst = 0.0
    for inc_cycles in (1 / 512, 1 / 64, 0.09, 0.11, 0.125, 0.15, 0.2, 0.25):
        inc = int(inc_cycles * 2**32)
        for k in range(0, 64):
            phase = (k * (1 << 26) + 12345) & 0xFFFFFFFF
            got, _ = run_helper(sram, sharc_run, phase, inc)
            want = saw_block(phase / 2**32, inc / 2**32)
            worst = max(worst, max(abs(a - b) for a, b in zip(got, want, strict=True)))
    return worst


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("check", help="compare the model with the SHARC oracle")
    c.add_argument("sram", type=Path)
    c.add_argument("sharc_run", type=Path)
    ns = ap.parse_args(argv)
    worst = check(ns.sram, ns.sharc_run)
    print(f"worst absolute difference model vs oracle: {worst:.3e}")
    return 0 if worst < 1e-5 else 1


if __name__ == "__main__":
    sys.exit(main())
