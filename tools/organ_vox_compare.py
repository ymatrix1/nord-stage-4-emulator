#!/usr/bin/env python3
"""Per-partial comparison of the Rust Vox source with the oracle chord (`sharc-pass`).

Both signals are in firmware frames (the oracle `pno_full_pass.py run --out PREFIX` writes
`PREFIX.f32.npy`; the Rust side is `organ_ref --mode vox --rate 43402.34375 --out PREFIX`, columns
`index,L,R`). The amplitude of every partial `h` of every note is read with a Hann window at its
exact 12-TET frequency (`cycles_per_frame`), relative to the fundamental of the same note.

    python tools/pno_full_pass.py run SRAM/core1_sram.bin --exe target/release/sharc-pass \
        --script docs/research/data/pno-pass-install-vox.script \
        --script docs/research/data/pno-pass-vox-weights-c4e4g4.script --passes 20000 --out vox
    cargo run --release -p nord-tools --bin organ_ref -- --mode vox --rate 43402.34375 \
        --frames 40000 --out rust_vox
    python tools/organ_vox_compare.py vox.f32.npy rust_vox.csv
"""

from __future__ import annotations

import argparse
import sys

import numpy as np

RATE_BASE = 0x62C2E7  # VERIFIED, vox_bank.rs
MIDI_NOTES = (60, 64, 67)
OSC_OFFSET = 24  # ASSUMED oscillator = note - 24


def cycles_per_frame(k: int) -> float:
    """Cycles per frame of note oscillator `k` at `m = 0.5`."""
    return 0.5 * RATE_BASE * 2 ** (k / 12) / 2**32


def partial_amplitude(x: np.ndarray, cycles: float) -> float:
    """Amplitude of the sinusoid of `cycles` per sample in `x` (Hann window, peak over 0.4 bin)."""
    n = len(x)
    t = np.arange(n)
    win = 0.5 - 0.5 * np.cos(2 * np.pi * t / n)
    xw = x * win
    best = 0.0
    for d in np.linspace(-0.4, 0.4, 17) / n:
        w = np.exp(-2j * np.pi * (cycles + d) * t)
        best = max(best, 2 * abs(np.dot(xw, w)) / win.sum())
    return best


def partial_table(x: np.ndarray, notes=MIDI_NOTES, harmonics: int = 9) -> np.ndarray:
    """`(len(notes), harmonics)` amplitudes; harmonics above Nyquist are 0."""
    out = np.zeros((len(notes), harmonics))
    for i, note in enumerate(notes):
        f0 = cycles_per_frame(note - OSC_OFFSET)
        for h in range(1, harmonics + 1):
            if h * f0 < 0.5:
                out[i, h - 1] = partial_amplitude(x, h * f0)
    return out


LFO_CYCLES = 0x905A3 / 2**32  # VERIFIED: pitch LFO step per frame


def sideband_table(x: np.ndarray, notes=MIDI_NOTES) -> np.ndarray:
    """`(len(notes), 2)`: first vibrato sidebands (carrier -+ LFO) relative to the carrier."""
    out = np.zeros((len(notes), 2))
    for i, note in enumerate(notes):
        f0 = cycles_per_frame(note - OSC_OFFSET)
        carrier = partial_amplitude(x, f0)
        for j, sign in enumerate((-1, 1)):
            out[i, j] = partial_amplitude(x, f0 + sign * LFO_CYCLES) / carrier
    return out


def relative_db(table: np.ndarray) -> np.ndarray:
    """Harmonic levels in dB relative to each note's fundamental."""
    with np.errstate(divide="ignore"):
        return 20 * np.log10(table / table[:, :1])


def load_csv(path: str) -> np.ndarray:
    """`(frames, 2)` of an `organ_ref --out PREFIX` CSV."""
    return np.loadtxt(path, delimiter=",", comments="#", skiprows=3, ndmin=2)[:, 1:3]


def compare(oracle: np.ndarray, rust: np.ndarray, skip: int = 4000, frames: int = 32768) -> dict:
    """Partial tables of both lanes of the oracle and of the Rust output, and their deviation."""
    o = oracle[skip : skip + frames]
    r = rust[skip : skip + frames]
    ot = [partial_table(o[:, lane]) for lane in (0, 1)]
    rt = partial_table(r[:, 0])
    return {
        "oracle_L": ot[0],
        "oracle_R": ot[1],
        "rust": rt,
        "dev_db_L": relative_db(rt) - relative_db(ot[0]),
        "dev_db_R": relative_db(rt) - relative_db(ot[1]),
        "level_db_L": 20 * np.log10(rt[:, 0] / ot[0][:, 0]),
        "level_db_R": 20 * np.log10(rt[:, 0] / ot[1][:, 0]),
    }


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("oracle", help="PREFIX.f32.npy of pno_full_pass.py run")
    ap.add_argument("rust", help="organ_ref CSV")
    ns = ap.parse_args(argv)
    res = compare(np.load(ns.oracle), load_csv(ns.rust))
    np.set_printoptions(precision=1, suppress=True, linewidth=160)
    print("fundamental level, Rust vs oracle L / R (dB):", res["level_db_L"], res["level_db_R"])
    for name, key in (("oracle L", "oracle_L"), ("oracle R", "oracle_R"), ("rust", "rust")):
        print(f"{name} harmonics 1..9 relative to the fundamental (dB), rows C4 E4 G4:")
        print(relative_db(res[key]))
    print("deviation Rust - oracle L (dB):")
    print(res["dev_db_L"])
    np.set_printoptions(precision=3)
    print("first sidebands (-LFO, +LFO) relative to the carrier, rows C4 E4 G4:")
    print("oracle L", sideband_table(np.load(ns.oracle)[4000:, 0], MIDI_NOTES).tolist())
    print("rust    ", sideband_table(load_csv(ns.rust)[4000:, 0], MIDI_NOTES).tolist())
    return 0


if __name__ == "__main__":
    sys.exit(main())
