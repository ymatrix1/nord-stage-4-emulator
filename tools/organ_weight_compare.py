#!/usr/bin/env python3
"""Rust `WeightMode::Firmware` against the oracle driven with the ARM weight messages.

The oracle (`sharc-organ`, PNO core 1 image) gets the increments, the group gains, the level `L = 1`
and the weights the way the ARM sends them: at call 0 a key-down (`wB` = firmware target, `a`
0.0096406, `b` 0.9903594 on the wheels of the held keys; `wA` starts at 0 and is smoothed by the
DSP), at `--keyoff` calls a full update with `wB = 0` and `a` 0.0042715 on all 92 wheels. The Rust
side is `organ_ref --weights firmware` with the same notes, drawbars, velocity and key-off frame.

    python tools/organ_weight_compare.py SRAM/core1_sram.bin --oracle target/release/sharc-organ \
        --rust target/release/organ_ref [--notes 60,64,67] [--drawbars 888000000]

Reports the level ratio and SNR per lane over the steady part, the attack and release envelope
error (block rms against the oracle steady rms) and the full-signal SNR after one gain.
Method and results: `docs/research/organ-weights.md` section 8.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np
import organ_oracle_audio as oa

# VERIFIED (organ-weights.md s2): coefficient words the ARM sends.
A_KEY_DOWN, B_KEY_DOWN = 0x3C1DF380, 0x3F7D8832
A_UPDATE, B_UPDATE = 0x3B8BF800, 0x3F7EE810
BLOCK = 64  # frames per envelope block
# VERIFIED (organ-oracle-audio.md s3, sentinel run): smoother states of the mix stage. g5/g6 states,
# g8/g7 states, level states (x, y). The harness boots them at 0, the running firmware has them
# settled, so the scenario starts them on their targets.
STATE_G56, STATE_G87, STATE_LEVEL = (0x959B1, 0x959B2), (0x95B40, 0x95B41), (0x95B44, 0x95B45)


def velocity_term(velocity: int) -> int:
    """`(vel * 127) >> 4` of the weight sum (taper 127)."""
    return (velocity * 127) >> 4


def firmware_targets(notes: list[int], drawbars: list[int], velocity: int = 127) -> np.ndarray:
    """`wB[w] = 2^-23 sum d7 * ((vel * 127) >> 4)` per wheel `w` (92 wheels)."""
    sums = np.zeros(oa.WHEELS, dtype=np.int64)
    for n in notes:
        for lvl, off in zip(drawbars, oa.DRAWBAR_OFFSETS, strict=True):
            sums[oa.foldback(n + off)] += oa.DRAWBAR_CURVE[lvl] * velocity_term(velocity)
    return sums.astype(np.float64) / 2**23


def weight_pokes(
    wb: np.ndarray, wheels: np.ndarray, a_bits: int, b_bits: int, call: int
) -> list[tuple[int, int, int]]:
    """`a`, `b`, `wB` pokes of set 0 for `wheels` at `call`."""
    pokes = []
    for i in wheels:
        i = int(i)
        pokes += [
            (call, oa.A_BASE + i, a_bits),
            (call, oa.B_BASE + i, b_bits),
            (call, oa.WB_BASE + i, oa.f2u(float(wb[i]))),
        ]
    return pokes


def scenario_pokes(
    notes: list[int],
    drawbars: list[int],
    velocity: int,
    keyoff_call: int,
    detuned: bool = False,
    level: float = 1.0,
    update_call: int | None = None,
) -> list[tuple[int, int, int]]:
    """Increments, gains and level, key-down at call 0, optional full update, key-off."""
    wb = firmware_targets(notes, drawbars, velocity)
    base = oa.build_pokes(np.zeros(oa.WHEELS), detuned, level)
    wheel_ranges = (oa.A_BASE, oa.B_BASE, oa.WB_BASE)
    pokes = [p for p in base if not any(r <= p[1] < r + oa.WHEELS for r in wheel_ranges)]
    g = oa.GAINS_DETUNED if detuned else oa.GAINS_EXACT
    g5, g6, g7, g8 = (v / 0x800000 for v in g)
    for addr, val in zip(
        (*STATE_G56, *STATE_G87, *STATE_LEVEL), (g5, g6, g8, g7, level, level), strict=True
    ):
        pokes.append((0, addr, oa.f2u(val)))
    held = np.nonzero(wb)[0]
    pokes += weight_pokes(wb, held, A_KEY_DOWN, B_KEY_DOWN, 0)
    allw = np.arange(oa.WHEELS)
    if update_call is not None:
        pokes += weight_pokes(wb, allw, A_UPDATE, B_UPDATE, update_call)
    pokes += weight_pokes(np.zeros(oa.WHEELS), allw, A_UPDATE, B_UPDATE, keyoff_call)
    return pokes


def block_rms(x: np.ndarray, block: int = BLOCK) -> np.ndarray:
    """RMS of consecutive `block`-frame blocks."""
    n = len(x) // block
    return np.sqrt((x[: n * block].reshape(n, block) ** 2).mean(axis=1))


def snr_after_gain(oracle: np.ndarray, ref: np.ndarray) -> tuple[float, float]:
    """`(rust/oracle level, SNR dB)` with the least-squares gain of `ref` onto `oracle`."""
    g, snr = oa.best_scale(oracle.astype(np.float64), ref.astype(np.float64))
    return 1.0 / g, snr


def envelope_error(oracle: np.ndarray, ref: np.ndarray, lo: int, hi: int) -> tuple[float, float]:
    """Max and rms block-envelope difference over frames `lo..hi`, relative to the oracle's peak
    block rms of the whole signal (so a ramp error of 0.01 is 1 % of the steady level)."""
    ref_env = block_rms(oracle.astype(np.float64)).max()
    eo = block_rms(oracle[lo:hi].astype(np.float64))
    er = block_rms(ref[lo:hi].astype(np.float64))
    d = (er - eo) / ref_env
    return float(np.abs(d).max()), float(np.sqrt((d**2).mean()))


def read_rust_csv(path: Path) -> np.ndarray:
    """`organ_ref` CSV (`index,L,R`, `#` header lines) as `(frames, 2)`."""
    rows = [
        line.split(",")
        for line in path.read_text(encoding="utf-8").splitlines()
        if line and line[0].isdigit()
    ]
    return np.array(rows, dtype=np.float64)[:, 1:3]


def run_rust(
    exe: str, notes: str, drawbars: str, velocity: int, frames: int, keyoff: int, detuned: bool
) -> np.ndarray:
    with tempfile.TemporaryDirectory() as tmp:
        stem = str(Path(tmp) / "r")
        cmd = [
            exe, "--weights", "firmware", "--notes", notes, "--drawbars", drawbars,
            "--velocity", str(velocity), "--frames", str(frames), "--keyoff-at", str(keyoff),
            "--out", stem,
        ]  # fmt: skip
        if detuned:
            cmd.append("--detuned")
        subprocess.run(cmd, check=True, capture_output=True)
        return read_rust_csv(Path(stem + ".csv"))


def report(orc: np.ndarray, rust: np.ndarray, keyoff: int) -> list[str]:
    """Report lines: steady level/SNR per lane, attack and release envelope error, full SNR."""
    lines = []
    steady = slice(keyoff // 2, keyoff)
    for k, name in enumerate(("x/L", "y/R")):
        lvl, snr = snr_after_gain(orc[steady, k], rust[steady, k])
        lines.append(f"lane {name} steady: rust/oracle level {lvl:.4f}x, SNR {snr:.1f} dB")
    lvl, snr = snr_after_gain(orc[:, 0], rust[:, 0])
    lines.append(f"lane x whole signal (attack+release): level {lvl:.4f}x, SNR {snr:.1f} dB")
    for name, lo, hi in (("attack", 0, keyoff // 2), ("release", keyoff, len(orc))):
        # compare after removing the steady gain error so the envelope SHAPE is measured
        g = snr_after_gain(orc[steady, 0], rust[steady, 0])[0]
        mx, rms = envelope_error(orc[:, 0], rust[:, 0] / g, lo, hi)
        lines.append(f"{name} envelope error: max {100 * mx:.2f} %, rms {100 * rms:.2f} % of peak")
    return lines


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image")
    ap.add_argument("--oracle", required=True)
    ap.add_argument("--rust", required=True)
    ap.add_argument("--notes", default="60,64,67")
    ap.add_argument("--drawbars", default="888000000")
    ap.add_argument("--velocity", type=int, default=127)
    ap.add_argument("--frames", type=int, default=16384)
    ap.add_argument("--keyoff", type=int, default=8192, help="key-off frame (even)")
    ap.add_argument("--update-call", type=int, help="call of a full update after the key-down")
    ap.add_argument("--detuned", action="store_true")
    ns = ap.parse_args(argv)
    notes = [int(n) for n in ns.notes.split(",")]
    pokes = scenario_pokes(
        notes, oa.drawbar_digits(ns.drawbars), ns.velocity, ns.keyoff // 2, ns.detuned,
        update_call=ns.update_call,
    )  # fmt: skip
    orc = oa.run_oracle(ns.oracle, ns.image, pokes, ns.frames // 2)
    rust = run_rust(ns.rust, ns.notes, ns.drawbars, ns.velocity, ns.frames, ns.keyoff, ns.detuned)
    print("\n".join(report(orc, rust, ns.keyoff)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
