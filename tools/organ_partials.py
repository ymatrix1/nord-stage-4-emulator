#!/usr/bin/env python3
"""Per-partial level error of the Rust B3 / Vox key paths against the oracle chord (`sharc-pass`).

Both signals are in firmware frames: the oracle `pno_full_pass.py run --out PREFIX` writes
`PREFIX.f32.npy`; the Rust side is `organ_ref --mode b3|vox --rate 43402.34375 --out PREFIX`
(columns `index,L,R`). The amplitude of every line (Hann window, peak over +-0.4 bin at the exact
line frequency of the signal's own tuning) is compared in absolute dB, no normalisation, so the
dry gain, the amplitudes and the output stage are all in the error.

    python tools/pno_full_pass.py run SRAM/core1_sram.bin --exe target/release/sharc-pass \
        --script b3_fb_m0.script --script b3_dry_gain.script --passes 12000 --out b3
    cargo run --release -p nord-tools --bin organ_ref -- --mode b3 --drawbars 888800000 \
        --rate 43402.34375 --frames 12000 --notes 60,64,67 --out rust_b3
    python tools/organ_partials.py b3 b3.f32.npy rust_b3.csv --keys 60,64,67 --drawbars 888800000

Line sets: B3 = the wheel notes `key + footage offset` of the drawbars that are up (fold into 12..88
wheels), harmonics 1..HARMONICS; Vox = the oscillator rows of the key-down flags of the default
program's level slots (`keydown-zones.md` section 5), harmonics 1..HARMONICS.
"""

from __future__ import annotations

import argparse
import sys

import numpy as np

HARMONICS = 4
FOOTAGE_OFFSETS = (-12, 7, 0, 12, 19, 24, 28, 31, 36)
# VERIFIED (b3-generator.md): phase ladder, fine words (image 0x3F000000, ARM 0x3FA02A00).
PHASE_INC = (
    0x31AD21,
    0x34A01A,
    0x37BD05,
    0x3B0C45,
    0x3E8DDA,
    0x424A27,
    0x4638C9,
    0x4A6A87,
    0x4ED6FD,
    0x53868F,
    0x58793D,
    0x5DBFCE,
)
FINE_IMAGE, FINE_ARM = 0x3F00_0000, 0x3FA0_2A00
FRAME_RATE = 43_402.34375
# VERIFIED (keydown-zones.md s5): Vox slot ladder and the default program's non-zero level slots.
VOX_SLOT_OFFSETS = (0, 7, 12, 19, 19, 24, 24, 28, 31, 31, 36, 36, 36, 40, 40, 43, 48, 48, 52, 60)
VOX_LEVEL_SLOTS = (0, 2, 5, 10)
VOX_RATE_BASE = 0x62C2E7  # VERIFIED, vox_bank.rs


def b3_step(note: int, fine: int) -> int:
    """Phase step per generator call of `note` (ssfr rounding, octave shift)."""
    base = (PHASE_INC[note % 12] * fine + (1 << 30)) >> 31
    return base << (note // 12)


def b3_cycles_oracle(note: int) -> float:
    """Cycles per frame of wheel `note` in the oracle (ARM fine word, 2 frames per call)."""
    return b3_step(note, FINE_ARM) / 2**32 / 2


def b3_cycles_rust(note: int) -> float:
    """Cycles per frame of wheel `note` in the Rust engine (`note_hz`: exact C anchor)."""
    c4 = 440.0 * 2 ** (-9 / 12)
    hz = c4 * b3_step(note, FINE_IMAGE) / b3_step(60, FINE_IMAGE)
    return hz / FRAME_RATE


def fold_wheel(wheel: int) -> int:
    """Fold a wheel (`note - 24`) into 12..88 (VERIFIED, keydown-zones.md s6.2)."""
    while wheel < 12:
        wheel += 12
    while wheel > 88:
        wheel -= 12
    return wheel


def b3_notes(keys: list[int], drawbars: str) -> list[int]:
    """Distinct wheel notes of the held keys at the drawbars that are up, sorted."""
    out = set()
    for key in keys:
        for off, level in zip(FOOTAGE_OFFSETS, drawbars):
            if level != "0":
                out.add(fold_wheel(key + off - 24) + 24)
    return sorted(out)


def vox_rows(keys: list[int]) -> list[int]:
    """Distinct oscillator rows of the held keys (fold into 0..95, VERIFIED), rows below 84 only."""
    out = set()
    for key in keys:
        for s in VOX_LEVEL_SLOTS:
            row = key - 36 + VOX_SLOT_OFFSETS[s]
            while row < 0:
                row += 12
            while row >= 96:
                row -= 12
            if row < 84:  # rows 84..95 are the unported sine bank
                out.add(row)
    return sorted(out)


def vox_cycles(row: int) -> float:
    """Cycles per frame of Vox oscillator `row` at pitch fraction 0.5 (`organ_vox_compare.py`)."""
    return 0.5 * VOX_RATE_BASE * 2 ** (row / 12) / 2**32


def line_amplitude(x: np.ndarray, cycles: float) -> float:
    """Amplitude of the sinusoid of `cycles` per sample in `x` (Hann window, +-0.4 bin peak)."""
    n = len(x)
    t = np.arange(n)
    win = 0.5 - 0.5 * np.cos(2 * np.pi * t / n)
    xw = x * win
    best = 0.0
    for d in np.linspace(-0.4, 0.4, 17) / n:
        w = np.exp(-2j * np.pi * (cycles + d) * t)
        best = max(best, 2 * abs(np.dot(xw, w)) / win.sum())
    return best


def table(x: np.ndarray, cycles: list[float], harmonics: int = HARMONICS) -> np.ndarray:
    """`(len(cycles), harmonics)` line amplitudes; lines above Nyquist are 0."""
    out = np.zeros((len(cycles), harmonics))
    for i, c in enumerate(cycles):
        for h in range(1, harmonics + 1):
            if h * c < 0.5:
                out[i, h - 1] = line_amplitude(x, h * c)
    return out


def db(a: np.ndarray) -> np.ndarray:
    with np.errstate(divide="ignore"):
        return 20 * np.log10(a)


def compare(oracle: np.ndarray, rust: np.ndarray, cyc_o: list[float], cyc_r: list[float]) -> dict:
    """Absolute per-line tables of both signals and the dB deviation Rust - oracle."""
    to, tr = table(oracle, cyc_o), table(rust, cyc_r)
    dev = db(tr) - db(to)
    valid = np.isfinite(dev) & (to > 1e-6 * to.max())
    return {"oracle": to, "rust": tr, "dev_db": dev, "valid": valid}


def load_csv(path: str) -> np.ndarray:
    """First lane of an `organ_ref --out PREFIX` CSV."""
    return np.loadtxt(path, delimiter=",", comments="#", skiprows=3, ndmin=2)[:, 1]


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("model", choices=("b3", "vox"))
    ap.add_argument("oracle")
    ap.add_argument("rust")
    ap.add_argument("--keys", default="60,64,67")
    ap.add_argument("--drawbars", default="888800000")
    ap.add_argument("--skip", type=int, default=4000)
    ap.add_argument("--frames", type=int, default=8192)
    ns = ap.parse_args(argv)
    keys = [int(k) for k in ns.keys.split(",")]
    lane = np.load(ns.oracle)[:, 0].astype(np.float64)
    rust = load_csv(ns.rust).astype(np.float64)
    sl = slice(ns.skip, ns.skip + ns.frames)
    if ns.model == "b3":
        lines = b3_notes(keys, ns.drawbars)
        co, cr = [b3_cycles_oracle(n) for n in lines], [b3_cycles_rust(n) for n in lines]
    else:
        lines = vox_rows(keys)
        co = cr = [vox_cycles(r) for r in lines]
    res = compare(lane[sl], rust[sl], co, cr)
    np.set_printoptions(precision=2, suppress=True, linewidth=160)
    print(f"{'line':>6}  " + "  ".join(f"h{h}: oracle  rust   dB" for h in range(1, HARMONICS + 1)))
    for i, n in enumerate(lines):
        cells = []
        for h in range(HARMONICS):
            if res["valid"][i, h]:
                o, r, d = res["oracle"][i, h], res["rust"][i, h], res["dev_db"][i, h]
                cells.append(f"{o:8.5f} {r:8.5f} {d:+6.2f}")
            else:
                cells.append(" " * 25)
        print(f"{n:>6}  " + "  ".join(cells))
    dev = res["dev_db"][res["valid"]]
    fund = res["dev_db"][:, 0][res["valid"][:, 0]]
    print(f"max |dB| fundamentals {np.abs(fund).max():.2f}, all lines {np.abs(dev).max():.2f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
