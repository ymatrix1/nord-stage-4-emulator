#!/usr/bin/env python3
"""Measured line gain `G_s(f)` of the Farfisa processor `0x122B85`, per weighted sum.

The processor is linear and time invariant in the ring words and the key-down weights
(`docs/research/organ-farfisa-bass.md` s7), so a ring slot `j` driven through sum `s` (one weight
word 1.0, all others 0) leaves as `y = h_s * ring_j` and the gain at every harmonic of the slot is
`G_s(f) = |Y(f)| / |R_j(f)|`. `sharc-chain` runs the generator `0x122A71` and the processor per
call from the boot + install state (`organ_proc_oracle.py`), so the ring slot and the output are
recorded together.

    python tools/organ_farfisa_proc.py measure SRAM/core1_sram.bin SCRIPT out.csv \
        --exe target/release/sharc-chain

`SCRIPT` is boot (no sentinels) + model-2 install (`organ_proc_oracle.build_script`, no key-down).
Rows: `sum,hz,gain_db` (binned medians); `spread` rows go to stderr.
"""

from __future__ import annotations

import argparse
import csv
import os
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np

# ASSUMED (organ-farfisa-bass.md s3): the anchor frame rate of the generator.
RATE = 43_402.6
RING0, RING1 = 0xC2B52, 0xC2BB4
WEIGHTS, WEIGHT_WORDS, SUM_STRIDE = 0x95FFC, 0x178, 94
GEN = "0x122A71:0x122B84:I0=0xc00ea,I1=0xb52c7,M0=0,M1=1,M2=2,M7=0xffffffff"
PROC = "0x122B85:0x122EF5:I0=0xc4000,I1=0xb474c,M0=0,M1=1,M2=2,M7=0xffffffff"
SKIP, FFT = 1200, 8192
BIN_PER_OCT = 6


def f32_bits(x: float) -> int:
    return int(np.float32(x).view(np.uint32))


def line_gains(ring: np.ndarray, out: np.ndarray, rate: float = RATE) -> list[tuple[float, float]]:
    """`(Hz, gain_db)` at the spectral lines of `ring` (peaks 50 dB below the strongest or more)."""
    win = np.hanning(len(ring))
    r = np.abs(np.fft.rfft(ring * win))
    y = np.abs(np.fft.rfft(out * win))
    r[:3] = 0
    top = r.max()
    rows = []
    for i in range(2, len(r) - 1):
        if r[i] > r[i - 1] and r[i] >= r[i + 1] and r[i] > top * 3e-3 and y[i] > 0:
            rows.append((i * rate / len(ring), 20 * float(np.log10(y[i] / r[i]))))
    return rows


def bin_table(
    rows: list[tuple[float, float]], per_oct: int = BIN_PER_OCT
) -> list[tuple[float, float, float]]:
    """`(hz, median gain_db, p90 - p10 spread)` per `1/per_oct` octave bin (note independence)."""
    bins: dict[int, list[tuple[float, float]]] = {}
    for hz, db in rows:
        bins.setdefault(int(np.floor(np.log2(hz) * per_oct)), []).append((hz, db))
    return [
        (
            float(np.median([h for h, _ in v])),
            float(np.median([d for _, d in v])),
            float(np.percentile([d for _, d in v], 90) - np.percentile([d for _, d in v], 10)),
        )
        for _, v in sorted(bins.items())
    ]


def run_slot(exe: str, image: str, script: Path, s: int, idx: int, slot: int, weight: float):
    """Ring slot `slot` and output of one `sharc-chain` run (`CALLS` calls, 2 frames each)."""
    calls = (SKIP + FFT) // 2
    lines = [f"0 {WEIGHTS + a:#x} 0x0" for a in range(WEIGHT_WORDS)]
    lines.append(f"0 {WEIGHTS + SUM_STRIDE * s + idx:#x} {f32_bits(weight):#x}")
    with tempfile.TemporaryDirectory() as t:
        extra, op = os.path.join(t, "x"), os.path.join(t, "o")
        Path(extra).write_text("\n".join(lines), encoding="utf-8")
        cmd = [os.path.abspath(exe), image, "--calls", str(calls)]
        cmd += ["--script", str(script), "--script", extra, "--step", GEN, "--step", PROC]
        cmd += ["--range", "0xc4000:3", "--range", f"{RING0 + slot:#x}:1"]
        cmd += ["--range", f"{RING1 + slot:#x}:1", "--out", op]
        subprocess.run(cmd, check=True, capture_output=True)
        w = np.fromfile(op, dtype="<u4").reshape(calls, 5).view(np.float32)
    out = np.stack([w[:, 0], w[:, 2]], 1).reshape(-1)[SKIP:]
    ring = np.stack([w[:, 3], w[:, 4]], 1).reshape(-1)[SKIP:]
    return ring, out


def rust_table(rows: list[dict[str, str]]) -> str:
    """Source of `crates/nord-core/src/organ/farfisa_proc_gain.rs` constants from the CSV rows."""
    nl = chr(10)
    out = []
    for s in (0, 1):
        pts = [(float(r["hz"]), float(r["gain_db"])) for r in rows if int(r["sum"]) == s]
        body = nl.join(f"    ({hz:.2f}, {db:.2f})," for hz, db in pts)
        out.append(f"pub(super) const SUM{s}_DB: [(f32, f32); {len(pts)}] = [{nl}{body}{nl}];")
    return (nl + nl).join(out) + nl


def cmd_rust(a: argparse.Namespace) -> int:
    with open(a.csv, encoding="utf-8") as f:
        print(rust_table(list(csv.DictReader(f))), end="")
    return 0


def cmd_measure(a: argparse.Namespace) -> int:  # pragma: no cover - runs the oracle
    rows_out = []
    for s, offset in ((0, 0), (1, 8)):
        rows = []
        for idx in range(2, 90, 4):
            ring, out = run_slot(a.exe, a.image, a.script, s, idx, idx + offset, a.weight)
            rows += [(h, g - 20 * float(np.log10(a.weight))) for h, g in line_gains(ring, out)]
        table = bin_table(rows)
        rows_out += [(s, hz, db, sp) for hz, db, sp in table]
        print(f"sum {s}: {len(rows)} lines, {len(table)} bins", file=sys.stderr)
    with open(a.out, "w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(["sum", "hz", "gain_db", "spread_db"])
        w.writerows((s, f"{hz:.2f}", f"{db:.2f}", f"{sp:.2f}") for s, hz, db, sp in rows_out)
    return 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    m = sub.add_parser("measure")
    m.add_argument("image")
    m.add_argument("script", type=Path)
    m.add_argument("out", type=Path)
    m.add_argument("--exe", default="sharc-chain")
    m.add_argument("--weight", type=float, default=1.0)
    r = sub.add_parser("rust", help="print the Rust constant tables of a measured CSV")
    r.add_argument("csv", type=Path)
    ns = ap.parse_args(argv)
    return cmd_measure(ns) if ns.cmd == "measure" else cmd_rust(ns)


if __name__ == "__main__":
    sys.exit(main())
