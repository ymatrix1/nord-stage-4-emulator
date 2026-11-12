#!/usr/bin/env python3
"""Census for the organ tone-generator hunt (docs/research/organ-tonegen.md).

Three independent checks on a flat SRAM image (`tools/ns4_dsp_ldr.py --sram DIR`, data word at
DM address `A` is the little-endian u32 at byte `4 * A`) or on an ADI listing (`tools/adi_dis.py`):

    python tools/organ_tonegen_census.py tables IMAGE        # semitone runs and sampled sinusoids
    python tools/organ_tonegen_census.py harmonics IMAGE 0xC042B --pairs 4096
    python tools/organ_tonegen_census.py loops LISTING --base 0x120200

* `tables`: runs of at least 12 consecutive words whose ratio is 2^(1/12) +- 0.4 % (note tables,
  read as float32 and as u32) and windows that obey the sinusoid recurrence
  `x[k+1] + x[k-1] = c x[k]` (sine tables, read as float32 and as int32).
* `harmonics`: spectrum of one cycle table stored as interleaved (x, y) pairs, in dB relative to
  the fundamental. A table built from a few harmonics at round dB values is a designed waveform.
* `loops`: counted loops whose body adds a phase, extracts the top bits with `fext` and reads a
  table with a modify register (`dm(mN, iM)`): the signature of a wavetable oscillator bank.

The counts are structural (EMPIRICAL): they locate candidates, they do not prove a role.
"""

from __future__ import annotations

import argparse
import cmath
import math
import re
import struct
import sys
from pathlib import Path

SEMITONE = 2 ** (1 / 12)
LCNTR = re.compile(r"lcntr=(0x[0-9a-f]+|\d+),\s*do")
LISTING = re.compile(r"^([0-9a-f]{8})\s+((?:[0-9a-f]{4} ?)+)\s{2,}(.*?)\s*$")


def load_words(path: Path) -> list[int]:
    """Little-endian u32 words of a flat image (trailing partial word ignored)."""
    data = path.read_bytes()
    n = len(data) // 4
    return list(struct.unpack(f"<{n}I", data[: n * 4]))


def as_f32(word: int) -> float:
    return struct.unpack("<f", struct.pack("<I", word))[0]


def as_i32(word: int) -> int:
    return word - (1 << 32) if word & 0x80000000 else word


def semitone_runs(
    values: list[float], min_len: int = 12, tol: float = 0.004
) -> list[tuple[int, int, float]]:
    """`(start, length, mean ratio)` of runs whose neighbour ratio is 2^(+-1/12) within `tol`."""
    out: list[tuple[int, int, float]] = []
    n = len(values)
    i = 0

    def ratio(k: int) -> float | None:
        a, b = values[k], values[k + 1]
        if not (math.isfinite(a) and math.isfinite(b)) or a == 0.0 or a * b <= 0.0:
            return None
        return b / a

    while i < n - 1:
        r = ratio(i)
        direction = None
        if r is not None and abs(r / SEMITONE - 1) < tol:
            direction = SEMITONE
        elif r is not None and abs(r * SEMITONE - 1) < tol:
            direction = 1 / SEMITONE
        if direction is None:
            i += 1
            continue
        j = i + 1
        while j + 1 < n:
            rj = ratio(j)
            if rj is None or abs(rj / direction - 1) >= tol:
                break
            j += 1
        length = j - i + 1
        if length >= min_len:
            out.append((i, length, (values[j] / values[i]) ** (1 / (length - 1))))
        i = j
    return out


def sine_windows(
    values: list[float], window: int = 24, tol: float = 1e-4, max_period_factor: float = 4.0
) -> list[tuple[int, int, float]]:
    """`(start, length, period)` of windows that are a sampled sinusoid.

    Within a window `x[k+1] + x[k-1] = c x[k]` holds with one `c = 2 cos(w)` (relative residual
    below `tol`^2). A window must cover at least `1 / max_period_factor` of a period, which
    rejects smooth curves (their `c` is just below 2 with an enormous period).
    """
    n = len(values)
    if n < window + 2:
        return []
    vals = [v if math.isfinite(v) else 0.0 for v in values]
    peak = max(abs(v) for v in vals) or 1.0
    xs = [vals[k] for k in range(1, n - 1)]
    ps = [vals[k + 1] + vals[k - 1] for k in range(1, n - 1)]
    sxx = spx = spp = 0.0
    hits: list[tuple[int, float]] = []
    for k in range(len(xs)):
        sxx += xs[k] * xs[k]
        spx += ps[k] * xs[k]
        spp += ps[k] * ps[k]
        if k >= window:
            o = k - window
            sxx -= xs[o] * xs[o]
            spx -= ps[o] * xs[o]
            spp -= ps[o] * ps[o]
        if k >= window - 1 and sxx > window * 1e-6 * peak * peak and spp > 0.0:
            c = spx / sxx
            resid = (spp - c * spx) / spp
            if resid < tol * tol and 0.01 < abs(c) < 1.99999:
                hits.append((k - window + 2, c))
    out: list[tuple[int, int, float]] = []
    s = 0
    while s < len(hits):
        e = s
        while e + 1 < len(hits) and hits[e + 1][0] == hits[e][0] + 1:
            e += 1
        start, c = hits[s]
        length = hits[e][0] - start + window
        period = 2 * math.pi / math.acos(max(-1.0, min(1.0, c / 2)))
        if period < max_period_factor * length:
            out.append((start, length, period))
        s = e + 1
    return out


def harmonic_levels(
    samples: list[float], harmonics: int = 10
) -> list[tuple[int, float, float, float]]:
    """`(n, magnitude, dB relative to harmonic 1, phase in degrees against a sine)`."""
    count = len(samples)
    rows = []
    mags = []
    for n in range(1, harmonics + 1):
        acc = sum(s * cmath.exp(-2j * math.pi * n * k / count) for k, s in enumerate(samples)) * (
            2 / count
        )
        mags.append(acc)
    base = abs(mags[0]) or 1.0
    for n, acc in enumerate(mags, start=1):
        mag = abs(acc)
        db = 20 * math.log10(mag / base) if mag > 1e-12 else float("-inf")
        phase = math.degrees(cmath.phase(acc * 1j)) if mag > 1e-12 else 0.0
        rows.append((n, mag, db, phase))
    return rows


def interleaved_table(words: list[int], base: int, pairs: int) -> tuple[list[float], list[float]]:
    """De-interleave `pairs` (x, y) float pairs starting at DM address `base`."""
    seg = [as_f32(w) for w in words[base : base + 2 * pairs]]
    return seg[0::2], seg[1::2]


def find_phase_loops(listing: str, base: int = 0, body: int = 14) -> list[tuple[int, int]]:
    """`(pc, count)` of counted loops that look like one step of a wavetable oscillator bank."""
    rows = []
    for line in listing.splitlines():
        m = LISTING.match(line)
        if m and m.group(3) != "nop;":
            rows.append((int(m.group(1), 16) + base, m.group(3)))
    hits = []
    for i, (pc, text) in enumerate(rows):
        m = LCNTR.search(text)
        if not m:
            continue
        window = " ".join(t for _, t in rows[i + 1 : i + 1 + body])
        phase_store = re.search(r"pm\(i\d+,m\d+\)=r\d+", window)
        extract = "=fext r" in window
        table_read = re.search(r"=dm\(m\d+,i\d+\)", window)
        if phase_store and extract and table_read:
            hits.append((pc, int(m.group(1), 0)))
    return hits


def cmd_tables(path: Path, out) -> int:
    words = load_words(path)
    f32 = [as_f32(w) for w in words]
    for label, vals in (("f32", f32), ("u32", [float(w) for w in words])):
        for start, length, ratio in semitone_runs(vals):
            print(f"semitone {label} DM {start:#x} len {length} ratio {ratio:.6f}", file=out)
    i32 = [float(as_i32(w)) for w in words]
    for label, vals in (("f32", f32), ("i32", i32)):
        for start, length, period in sine_windows(vals):
            print(f"sinusoid {label} DM {start:#x} len {length} period {period:.1f}", file=out)
    return 0


def cmd_harmonics(path: Path, base: int, pairs: int, out) -> int:
    words = load_words(path)
    xs, ys = interleaved_table(words, base, pairs)
    for label, seg in (("x", xs), ("y", ys)):
        print(f"{label}: n magnitude dB phase", file=out)
        for n, mag, db, phase in harmonic_levels(seg):
            print(f"  {n:2d} {mag:.6f} {db:8.2f} {phase:8.1f}", file=out)
    return 0


def cmd_loops(path: Path, base: int, out) -> int:
    for pc, count in find_phase_loops(path.read_text(), base):
        print(f"oscillator-like loop at pc {pc:#x} count {count:#x} ({count})", file=out)
    return 0


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    t = sub.add_parser("tables", help="semitone runs and sampled sinusoids")
    t.add_argument("image", type=Path)
    h = sub.add_parser("harmonics", help="harmonic levels of an interleaved cycle table")
    h.add_argument("image", type=Path)
    h.add_argument("base", help="DM word address of the first pair, for example 0xC042B")
    h.add_argument("--pairs", type=int, default=4096)
    lo = sub.add_parser("loops", help="wavetable-oscillator loops in an ADI listing")
    lo.add_argument("listing", type=Path)
    lo.add_argument("--base", default="0x120200", help="pc of listing address 0")
    args = p.parse_args(argv)
    if args.cmd == "tables":
        return cmd_tables(args.image, sys.stdout)
    if args.cmd == "harmonics":
        return cmd_harmonics(args.image, int(args.base, 0), args.pairs, sys.stdout)
    return cmd_loops(args.listing, int(args.base, 0), sys.stdout)


if __name__ == "__main__":
    sys.exit(main())
