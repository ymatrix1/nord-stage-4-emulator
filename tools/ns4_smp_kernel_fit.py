#!/usr/bin/env python3
"""Identify the 6-tap, 256-phase Q31 interpolation kernel of the SMP DSP image (DM 0xE3400).

Result (docs/research/smp-kernel-fit.md): the 1,536 words are reproduced bit-exactly (1,535 of
1,536 by plain rounding, the last is the saturated peak) by

    d        = (2 - tap) + phase / 256                    tap 0..5, phase 0..255
    u(d)     = sinc(d) * (0.54 + 0.46 * cos(pi * d / 3))  Hamming window, half width 3, fc = 1
    h[t][p]  = u + (1 - sum_t u) / 6                      DC gain forced to 1 by an additive term
    word     = min(round(h * 2^23), 2^23 - 1) << 8         Q31 with the low byte zero

The additive normalisation (not a division) is what made every plain window fit stall at rms
1e-4: it adds a per-phase constant that puts a slope jump of 0.00889 at every integer offset.

    python tools/ns4_dsp_raw.py dsp_image_4_SMP.bin --sram smp.bin      # flat SRAM image
    python tools/ns4_smp_kernel_fit.py smp.bin                          # verify, report
    python tools/ns4_smp_kernel_fit.py smp.bin --survey                 # other windows, same form

Do not commit the extracted table; this file keeps only the closed form.
"""

from __future__ import annotations

import argparse
import math
import struct
import sys
from collections.abc import Callable
from pathlib import Path

KERNEL_DM = 0xE3400
TAPS = 6
PHASES = 256
WORDS = TAPS * PHASES
CENTER_TAP = 2
Q_BITS = 23  # significant bits; the word is value << 8 (Q31, low byte zero)
Q_MAX = (1 << Q_BITS) - 1  # 0x7FFFFF: the peak 1.0 saturates here (word 0x7FFFFF00)
HAMMING_A = 0.54
HALF_WIDTH = 3.0

Window = Callable[[float], float]


def offset(tap: int, phase: int) -> float:
    """Sample offset `d` of table entry `[tap][phase]` (peak at tap 2, phase 0)."""
    return (CENTER_TAP - tap) + phase / PHASES


def sinc(d: float) -> float:
    if d == 0.0:
        return 1.0
    x = math.pi * d
    return math.sin(x) / x


def hamming(a: float = HAMMING_A, half_width: float = HALF_WIDTH) -> Window:
    """`a + (1 - a) cos(pi d / half_width)`: 1 at d = 0, `2a - 1` at the edge."""
    return lambda d: a + (1.0 - a) * math.cos(math.pi * d / half_width)


def model_real(window: Window, fc: float = 1.0) -> list[list[float]]:
    """`[tap][phase]` real kernel: windowed sinc plus an additive per-phase DC correction."""
    rows = [
        [sinc(fc * offset(t, p)) * window(offset(t, p)) for p in range(PHASES)] for t in range(TAPS)
    ]
    for p in range(PHASES):
        fix = (1.0 - sum(rows[t][p] for t in range(TAPS))) / TAPS
        for t in range(TAPS):
            rows[t][p] += fix
    return rows


def quantise(real: list[list[float]]) -> list[int]:
    """Row-major Q23 integers: round to nearest, saturate at `Q_MAX`."""
    return [min(round(v * (1 << Q_BITS)), Q_MAX) for row in real for v in row]


def to_words(q23: list[int]) -> list[int]:
    return [v << 8 for v in q23]


def model_words(window: Window | None = None) -> list[int]:
    return to_words(quantise(model_real(window or hamming())))


def read_table(sram: bytes, base: int = KERNEL_DM) -> list[int]:
    """The 1,536 signed 32-bit words at DM `base` of a flat SRAM image (byte `4 * dm_word`)."""
    raw = sram[4 * base : 4 * base + 4 * WORDS]
    if len(raw) != 4 * WORDS:
        raise ValueError("image too short for the kernel table")
    return list(struct.unpack(f"<{WORDS}i", raw))


def compare(table: list[int], model: list[int]) -> dict[str, float]:
    """Mismatch count and error statistics in LSB of the 23-bit value."""
    err = [(a - b) / 256 for a, b in zip(table, model, strict=True)]
    return {
        "mismatches": sum(1 for e in err if e != 0),
        "max_lsb": max(abs(e) for e in err),
        "rms_lsb": math.sqrt(sum(e * e for e in err) / len(err)),
    }


def mirror_errors(table: list[int]) -> int:
    """Entries violating `h[t][p] = h[5 - t][256 - p]` (phase 0 pairs with the next row at 256)."""
    bad = 0
    for t in range(TAPS):
        for p in range(1, PHASES):
            if table[t * PHASES + p] != table[(TAPS - 1 - t) * PHASES + PHASES - p]:
                bad += 1
    return bad


def phase_sums(table: list[int]) -> tuple[int, int]:
    """Smallest and largest per-phase tap sum, in Q23 units (exactly 2^23 would be unity)."""
    sums = [sum(table[t * PHASES + p] for t in range(TAPS)) // 256 for p in range(PHASES)]
    return min(sums), max(sums)


def survey_windows() -> dict[str, Window]:
    """Other windows tried under the same sinc + additive-DC form (all far worse than Hamming)."""
    return {
        "hamming 0.54/0.46": hamming(),
        "hann half width 3": lambda d: 0.5 + 0.5 * math.cos(math.pi * d / 3.0),
        "hamming half width 3.2": hamming(0.54, 3.2),
        "blackman half width 3": lambda d: (
            0.42 + 0.5 * math.cos(math.pi * d / 3.0) + 0.08 * math.cos(2 * math.pi * d / 3.0)
        ),
        "lanczos a=3": lambda d: sinc(d / 3.0),
    }


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument(
        "sram", type=Path, help="flat SRAM image of the SMP image (ns4_dsp_raw.py --sram)"
    )
    ap.add_argument("--survey", action="store_true", help="also report other window families")
    args = ap.parse_args(argv)
    table = read_table(args.sram.read_bytes())
    print(f"mirror violations {mirror_errors(table)}, phase sums (Q23) {phase_sums(table)}")
    families = survey_windows() if args.survey else {"hamming 0.54/0.46": hamming()}
    for name, win in families.items():
        r = compare(table, model_words(win))
        print(
            f"{name:26s} mismatches {r['mismatches']:4.0f} max {r['max_lsb']:10.2f} LSB "
            f"rms {r['rms_lsb']:10.3f} LSB"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
