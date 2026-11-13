#!/usr/bin/env python3
"""Frequency response of the B3 processor `0x121E2B` tail, measured in the oracle (`sharc-pass`).

The processor forms the dry sum `s = sum_n W[n] ring[n]` of the 80 ring words and adds filtered
branches (FIR / lattice sections programmed by the engine constructor `0xC002187C`). Seen from
the ring words the whole chain is linear and time invariant, so a sustained wheel line of
frequency `f` leaves with the gain `G(f) = Y(f) / (A W h_k(note))`, where `A`, `W` are the ARM
key-down words and `h_k` the wheel-table harmonic. This tool

* `subset`  writes an override script that keeps `W` of a clean subset of ring notes (no line of
            one note within `MIN_SEP_BINS` of a line of another) and zeroes the rest,
* `measure` reads the oracle lanes and prints `note,harmonic,hz,predicted,oracle,gain_db` rows,
* `table`   merges measured rows into the sorted `(hz, gain_db)` table of `b3-tail-gain.csv`.

    python tools/organ_b3_proc.py subset arm_t0.script sub0 --frames 16384
    python tools/pno_full_pass.py run SRAM/core1_sram.bin --script arm_t0.script \
        --script sub0.override.script --script dry_gain.script --passes 12000 --out sub0
    python tools/organ_b3_proc.py measure arm_t0.script sub0.override.json sub0.f32.npy

Method and results: `docs/research/organ-b3-tail.md`.
"""

from __future__ import annotations

import csv
import json
import sys
from pathlib import Path

import numpy as np
import organ_partials as op

W_BASE, W_COUNT = 0x951F1, 80  # ring notes 36..115 (VERIFIED, b3-generator.md s4)
A_BASE = 0x95122  # A word of ring note 36 (VERIFIED, keydown-zones.md s6.2)
FIRST_NOTE = 36
MIN_SEP_BINS = 3.0
MAX_HARMONIC = 6
# VERIFIED (b3_wheels.rs, FFT of the firmware tables): wheel-table harmonic amplitudes per class.
AMP = (
    (0.902800, 0.148024, 0.024071, 0.005588, 0.000974, 0.000255),
    (0.912761, 0.102303, 0.013053, 0.005274, 0.000488, 0.000397),
    (0.982516, 0.017472, 0.001747, 0.0, 0.0, 0.0),
)


def wheel_class(note: int) -> int:
    """Firmware table class of a ring note (`b3_wheels::wheel_table`)."""
    return 0 if note < 40 else 1 if note < 48 else 2


def f32(bits: int) -> float:
    return float(np.array([bits], dtype=np.uint32).view(np.float32)[0])


def final_words(script: str) -> dict[int, int]:
    """Last written value of every DM address of a `PASS ADDR VALUE` script."""
    out: dict[int, int] = {}
    for line in script.splitlines():
        f = line.split("#")[0].split()
        if len(f) == 3:
            out[int(f[1], 0)] = int(f[2], 0)
    return out


def note_gain(words: dict[int, int], note: int) -> float:
    """`A * W` of ring note `note` after the key-down (floats of the ARM words)."""
    a = f32(words.get(A_BASE + note - FIRST_NOTE, 0))
    w = f32(words.get(W_BASE + note - FIRST_NOTE, 0))
    return a * w


def line_cycles(note: int, h: int) -> float:
    """Cycles per frame of harmonic `h` of ring note `note` in the oracle."""
    return h * op.b3_cycles_oracle(note)


def clean_subset(
    notes: list[int], frames: int, min_bins: float = MIN_SEP_BINS, harmonics: int = 4
) -> list[int]:
    """Greedy subset of `notes` whose first `harmonics` lines stay `min_bins` apart."""
    chosen: list[int] = []
    lines: list[float] = []
    for n in notes:
        mine = [line_cycles(n, h) for h in range(1, harmonics + 1)]
        ok = all(abs(a - b) * frames >= min_bins for a in mine for b in lines)
        if ok:
            chosen.append(n)
            lines += mine
    return chosen


def active_notes(words: dict[int, int]) -> list[int]:
    """Ring notes with a non-zero key-down `A * W`."""
    return [n for n in range(FIRST_NOTE, FIRST_NOTE + W_COUNT) if note_gain(words, n) > 0]


def override(words: dict[int, int], keep: list[int]) -> str:
    """Script that zeroes every `W` except those of `keep` (their final values are re-poked)."""
    lines = []
    for i in range(W_COUNT):
        n = FIRST_NOTE + i
        v = words.get(W_BASE + i, 0) if n in keep else 0
        lines.append(f"0 {W_BASE + i:#x} {v:#010x}")
    return "\n".join(lines) + "\n"


def measure(words: dict[int, int], keep: list[int], lane: np.ndarray, skip: int, frames: int):
    """Rows `(note, h, hz, predicted, oracle, gain_db)` of every clean line of `keep`."""
    x = lane[skip : skip + frames].astype(np.float64)
    rows = []
    for n in keep:
        cls = wheel_class(n)
        for h in range(1, MAX_HARMONIC + 1):
            c = line_cycles(n, h)
            pred = note_gain(words, n) * AMP[cls][h - 1]
            if c >= 0.5 or pred < 3e-6:
                continue
            got = op.line_amplitude(x, c)
            if got <= 0:
                continue
            rows.append((n, h, c * op.FRAME_RATE, pred, got, 20 * np.log10(got / pred)))
    return rows


MIN_PREDICTED = 2e-5  # weaker lines are dominated by the measurement floor
BIN_SEMITONES = 0.5
OUTLIER_DB = 1.5


def merge_table(rows: list[tuple]) -> list[tuple[float, float]]:
    """Sorted `(hz, gain_db)` points from measured rows `(note, h, hz, pred, got, gain_db)`.

    Lines weaker than `MIN_PREDICTED` are dropped, the rest is averaged in half-semitone bins
    (frequency in the bin mean, gain in dB), then points more than `OUTLIER_DB` away from the
    straight line (in log frequency) between their neighbours are removed.
    """
    bins: dict[int, list[tuple[float, float]]] = {}
    for r in rows:
        if r[3] >= MIN_PREDICTED:
            key = round(12 * np.log2(r[2]) / BIN_SEMITONES)
            bins.setdefault(key, []).append((r[2], r[5]))
    pts = [
        (float(np.mean([h for h, _ in v])), float(np.mean([g for _, g in v])))
        for _, v in sorted(bins.items())
    ]
    keep = list(pts)
    while len(keep) > 2:
        worst, at = 0.0, -1
        for i in range(1, len(keep) - 1):
            (h0, g0), (h1, g1), (h2, g2) = keep[i - 1], keep[i], keep[i + 1]
            t = np.log2(h1 / h0) / np.log2(h2 / h0)
            dev = abs(g1 - (g0 + t * (g2 - g0)))
            if dev > worst:
                worst, at = dev, i
        if worst <= OUTLIER_DB:
            break
        del keep[at]
    return keep


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    a = sys.argv[1:] if argv is None else argv
    if len(a) < 3 or a[0] not in ("subset", "measure", "table"):
        print(__doc__)
        return 2
    if a[0] == "subset":
        words = final_words(Path(a[1]).read_text(encoding="utf-8"))
        frames = int(a[a.index("--frames") + 1]) if "--frames" in a else 16384
        keep = clean_subset(active_notes(words), frames)
        Path(f"{a[2]}.override.script").write_text(override(words, keep), encoding="utf-8")
        Path(f"{a[2]}.override.json").write_text(json.dumps(keep), encoding="utf-8")
        print("keep", keep)
        return 0
    if a[0] == "measure":
        words = final_words(Path(a[1]).read_text(encoding="utf-8"))
        keep = json.loads(Path(a[2]).read_text(encoding="utf-8"))
        lane = np.load(a[3])[:, 0]
        for r in measure(words, keep, lane, 4000, 16384):
            print(",".join(f"{v:.6g}" if isinstance(v, float) else str(v) for v in r))
        return 0
    rows = []
    for p in a[2:]:
        with Path(p).open(newline="", encoding="utf-8") as fh:
            rows += [tuple(float(v) for v in r) for r in csv.reader(fh)]
    with Path(a[1]).open("w", encoding="utf-8") as fh:
        fh.write("hz,gain_db\n")
        for hz, g in merge_table(rows):
            fh.write(f"{hz:.4f},{g:.3f}\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
