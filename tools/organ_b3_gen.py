#!/usr/bin/env python3
"""Model of the B3 tone generator `0x121BBE` (PNO core 1, slot 1) checked against the SHARC oracle.

Findings (`docs/research/b3-generator.md`): the generator writes one *tone wheel* per active drawbar
item into the ring `DM(0xB32E8 + note - 12)`. Each ring word is a table-lookup oscillator

    ring[note](t) = gain(note) * WHEEL((phase0 + t * f_note) mod 1)           (linear interpolation)

with `WHEEL` the 128 + 1 point table at DM `0xB324D` (`WHEEL_TABLE`) and

    f_note = FREQ_SCALE * PHASE_INC[note % 12] * 2 ** (note // 12)  cycles per call,

`PHASE_INC` the twelve words at DM `0x95172` (one octave of the Hammond-style gear ratios, within
0.05 % of 12-TET). The drawbar footages map to notes `key + (-12, 7, 0, 12, 19, 24, 28, 31, 36)`.

The module needs no firmware to import. The oracle comparison (`scan`) needs the PNO image and the
ARM captures of `organ_b3_pass.py`:

    uv run --with unicorn python tools/organ_b3_pass.py boot-messages CODE SRAM boot.jsonl
    uv run --with unicorn python tools/organ_b3_pass.py keys CODE SRAM k72.json --key 72 \
        --drawbars 888888888 --cache settled.snap
    python tools/organ_b3_gen.py scan SRAM/core1_sram.bin boot.jsonl k72.json --exe sharc-chain
"""

from __future__ import annotations

import argparse
import json
import struct
import sys
from dataclasses import dataclass
from pathlib import Path

import numpy as np

# VERIFIED: DM 0x95172..0x9517D of the PNO image (read with `image_u32`); 12-TET column
# `PHASE_INC[0] * 2^(k/12)` is within 0.05 % (`phase_inc_cents`), `docs/research/b3-generator.md`.
PHASE_INC = (
    0x31AD21, 0x34A01A, 0x37BD05, 0x3B0C45, 0x3E8DDA, 0x424A27,
    0x4638C9, 0x4A6A87, 0x4ED6FD, 0x53868F, 0x58793D, 0x5DBFCE,
)  # fmt: skip
PHASE_INC_DM = 0x95172
WHEEL_TABLE_DM, WHEEL_POINTS = 0xB324D, 129
# VERIFIED (`organ_b3_pass.py` ring, ARM item pointers): ring word = note - 12.
RING_BASE, RING_NOTE_OFFSET = 0xB32E8, 12
# EMPIRICAL (drawbar series, `b3-tone-source.md`): footage 16, 5 1/3, 8, 4, 2 2/3, 2, 1 3/5,
# 1 1/3, 1 as semitones relative to the key (8' = 0).
DRAWBAR_SEMITONES = (-12, 7, 0, 12, 19, 24, 28, 31, 36)


def phase_inc_cents() -> list[float]:
    """Deviation of every table entry from the exact 12-TET ladder, in cents."""
    return [1200 * np.log2(v / (PHASE_INC[0] * 2 ** (k / 12))) for k, v in enumerate(PHASE_INC)]


# VERIFIED (`scan`, 14 keys, ratios of 117 fitted frequencies to the model, max 4e-9): the fine
# tune word DM(0xB33B8) = 0x3F000000 is a fractional multiplier (`r8 = r0 * r7 (ssfr)`, 63/128).
FINE_WORD = 0x3F00_0000


def base_step(pitch_class: int, rounding: str = "round") -> int:
    """Phase step (units of 2**-32 cycle per call) of the base oscillator `pitch_class` (octave 0).

    `round`: `(inc * FINE + 2**30) >> 31`, the SHARC `ssfr` rounding; `floor` for comparison.
    """
    prod = PHASE_INC[pitch_class] * FINE_WORD
    return (prod + (1 << 30) >> 31) if rounding == "round" else prod >> 31


def note_step(note: int, rounding: str = "round") -> int:
    """Phase step of MIDI `note`: base step shifted up by the octave (separate accumulators)."""
    return base_step(note % 12, rounding) << (note // 12)


def note_freq(note: int, rounding: str = "round") -> float:
    """Cycles per call of MIDI `note`."""
    return note_step(note, rounding) / 2**32


def partial_notes(key: int) -> list[int]:
    """Notes of the nine drawbar partials of `key`."""
    return [key + s for s in DRAWBAR_SEMITONES]


def ring_index(note: int) -> int:
    """Ring word offset (from `RING_BASE`) of `note`."""
    return note - RING_NOTE_OFFSET


def wheel(table: np.ndarray, phase: np.ndarray) -> np.ndarray:
    """Linear interpolation in a `len(table) - 1` point cyclic table; `phase` in cycles."""
    n = len(table) - 1
    x = (np.asarray(phase) % 1.0) * n
    i = np.minimum(np.floor(x).astype(int), n - 1)
    frac = x - i
    return table[i] * (1.0 - frac) + table[i + 1] * frac


def model_series(table: np.ndarray, freq: float, phase0: float, calls: int) -> np.ndarray:
    return wheel(table, phase0 + freq * np.arange(calls))


@dataclass(frozen=True)
class Fit:
    phase0: float
    gain: float
    rel_residual: float  # rms(residual) / rms(series)


def fit_gain_phase(series: np.ndarray, table: np.ndarray, freq: float) -> Fit:
    """Best `(phase0, gain)` of `gain * WHEEL(phase0 + t f)` for a known frequency.

    Gain is linear (least squares); phase0 comes from a coarse grid plus golden-section refinement.
    """
    series = np.asarray(series, dtype=np.float64)
    t = np.arange(len(series))

    def cost(p0: float) -> tuple[float, float]:
        m = wheel(table, p0 + freq * t)
        g = float(m @ series / (m @ m))
        if g <= 0:  # a negative gain is the same fit half a cycle away for odd tables
            return float("inf"), g
        return float(np.sum((series - g * m) ** 2)), g

    grid = np.linspace(0, 1, 129, endpoint=False)
    costs = [cost(p)[0] for p in grid]
    k = int(np.argmin(costs))
    lo, hi = grid[k] - 1 / 129, grid[k] + 1 / 129
    ratio = (np.sqrt(5) - 1) / 2
    for _ in range(60):
        a, b = hi - ratio * (hi - lo), lo + ratio * (hi - lo)
        if cost(a)[0] < cost(b)[0]:
            hi = b
        else:
            lo = a
    p0 = (lo + hi) / 2
    c, g = cost(p0)
    rms = float(np.sqrt(np.mean(series**2))) or 1.0
    return Fit(p0 % 1.0, g, float(np.sqrt(c / len(series))) / rms)


def fft_freq(series: np.ndarray) -> float:
    """Dominant frequency in cycles per sample (parabolic peak interpolation)."""
    x = np.asarray(series, dtype=np.float64)
    x = x - x.mean()
    sp = np.abs(np.fft.rfft(x * np.hanning(len(x))))
    k = int(np.argmax(sp[1:])) + 1
    if 1 <= k < len(sp) - 1:
        a, b, c = np.log(sp[k - 1] + 1e-30), np.log(sp[k] + 1e-30), np.log(sp[k + 1] + 1e-30)
        k = k + 0.5 * (a - c) / (a - 2 * b + c)
    return float(k / len(x))


def refine_freq(series: np.ndarray, table: np.ndarray, f0: float, span: float = 0.002) -> float:
    """Frequency (cycles/call) minimising the residual of the wheel model near `f0`."""
    lo, hi = f0 * (1 - span), f0 * (1 + span)
    ratio = (np.sqrt(5) - 1) / 2
    for _ in range(40):
        a, b = hi - ratio * (hi - lo), lo + ratio * (hi - lo)
        if (
            fit_gain_phase(series, table, a).rel_residual
            < fit_gain_phase(series, table, b).rel_residual
        ):
            hi = b
        else:
            lo = a
    return (lo + hi) / 2


def harmonics(table: np.ndarray, count: int = 8) -> tuple[np.ndarray, np.ndarray]:
    """Amplitude and phase of harmonics `1..count` so that `table(p) = sum a_h cos(2 pi h p + f_h)`.

    `table` has `n + 1` points (the last repeats the first); used to turn the firmware wheel
    tables into a handful of numbers without copying the table itself.
    """
    n = len(table) - 1
    spec = np.fft.rfft(np.asarray(table[:n], dtype=np.float64))
    return np.abs(spec[1 : count + 1]) * 2 / n, np.angle(spec[1 : count + 1])


def image_u32(image: Path, address: int, count: int) -> list[int]:
    data = Path(image).read_bytes()
    return [struct.unpack_from("<I", data, 4 * (address + i))[0] for i in range(count)]


def image_wheel(image: Path) -> np.ndarray:  # pragma: no cover - needs the firmware image
    words = image_u32(image, WHEEL_TABLE_DM, WHEEL_POINTS)
    return np.array([struct.unpack("<f", struct.pack("<I", w))[0] for w in words])


def scan_columns(ring: np.ndarray, table: np.ndarray, skip: int = 200) -> list[dict]:
    """Per active ring column: note, measured frequency, gain and model residual."""
    rows = []
    for c in range(ring.shape[1]):
        col = ring[skip:, c].astype(np.float64)
        if col.std() < 1e-4:
            continue
        f = refine_freq(col, table, fft_freq(col))
        fit = fit_gain_phase(col, table, f)
        rows.append(
            {
                "address": RING_BASE + c,
                "note": c + RING_NOTE_OFFSET,
                "freq": f,
                "gain": fit.gain,
                "phase0": fit.phase0,
                "residual": fit.rel_residual,
            }
        )
    return rows


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    import organ_b3_pass as bp

    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("cmd", choices=["scan"])
    ap.add_argument("image")
    ap.add_argument("boot", type=Path)
    ap.add_argument("keys", type=Path, nargs="+", help="`organ_b3_pass.py keys` captures")
    ap.add_argument("--exe", default="sharc-chain")
    ap.add_argument("--calls", type=int, default=2400)
    ap.add_argument("--json", type=Path, help="write rows (key file, note, freq, gain, ...)")
    ns = ap.parse_args(argv)
    boot = [json.loads(x) for x in ns.boot.read_text().splitlines() if x]
    table = image_wheel(Path(ns.image))
    out = []
    for kf in ns.keys:
        rows = list(boot)
        keys = json.loads(kf.read_text())
        for part in ("install", "update", "drawbar_items", "percussion_items"):
            rows += keys[part]
        data = bp.run(ns.exe, ns.image, bp.last_words(rows), ns.calls)
        for r in scan_columns(data[:, : bp.RING_WORDS], table):
            r["file"] = kf.name
            out.append(r)
            if r["residual"] > 1e-6:  # too few cycles in the window to fit
                continue
            pred = {m: note_freq(r["note"], m) for m in ("round", "floor")}
            print(
                f"{kf.name} {r['address']:#x} note {r['note']:3d} f {r['freq']:.9f} "
                f"rel(round) {r['freq'] / pred['round'] - 1:+.1e} "
                f"rel(floor) {r['freq'] / pred['floor'] - 1:+.1e} gain {r['gain']:.4f}"
            )
    if ns.json:
        ns.json.write_text(json.dumps(out), encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
