#!/usr/bin/env python3
"""Compare an oracle dump (`sharc-ref`) with a reference render (Rust engine, CSV or WAV).

Usage:
  python tools/ns4_oracle_compare.py ORACLE REF [--series L,R] [--max-lag N]
         [--tol-abs X] [--tol-snr DB] [--plot] [--quiet]

ORACLE and REF are CSV files (`# key: value` metadata lines, a header row `index,<series...>`,
one row per sample index) or WAV files (PCM 16/24/32 bit or 32-bit float, one series per channel
named `ch0`, `ch1`, ...). Series are matched by name (default: every name present in both files
that does not start with `src`; `--series a:b` maps oracle series `a` to reference series `b`).

Per series the report gives: sample count, max absolute error and its index, RMS error, SNR in dB
(`10 log10(sum ref^2 / sum err^2)`), the largest error in float32 ulps, the number of bit-exact
samples, and the estimated delay (cross-correlation lag in samples, parabolic refinement).
`--plot` adds text plots of the reference, the error and a coarse error profile.

Exit status: 0 when every checked tolerance holds, 1 when `--tol-abs` or `--tol-snr` is exceeded,
2 on usage or input errors.
"""

from __future__ import annotations

import argparse
import math
import struct
import sys
from pathlib import Path


def f32(x: float) -> float:
    return struct.unpack("<f", struct.pack("<f", x))[0]


def read_csv(
    path: str | Path, round32: bool = True
) -> tuple[dict[str, str], dict[str, list[float]]]:
    """Metadata (`# key: value`) and series of a dump CSV; empty cells end a column's data.

    The dumps print shortest round-trip float32 text, so with `round32` (default) every cell is
    rounded to the float32 it denotes; otherwise the decimal value is kept as a double.
    """
    meta: dict[str, str] = {}
    header: list[str] | None = None
    cols: dict[str, list[float]] = {}
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        if line.startswith("#"):
            key, sep, value = line[1:].strip().partition(": ")
            if sep:
                meta[key] = value
            continue
        if not line.strip():
            continue
        cells = line.split(",")
        if header is None:
            header = cells[1:]
            cols = {name: [] for name in header}
            continue
        for name, cell in zip(header, cells[1:]):
            if cell != "":
                cols[name].append(f32(float(cell)) if round32 else float(cell))
    return meta, cols


def read_wav(path: str | Path) -> tuple[dict[str, str], dict[str, list[float]]]:
    """PCM 16/24/32 or IEEE float 32 WAV into `ch0..` series, plus `rate` metadata."""
    data = Path(path).read_bytes()
    if data[:4] != b"RIFF" or data[8:12] != b"WAVE":
        raise ValueError(f"{path}: not a RIFF/WAVE file")
    pos, fmt, payload = 12, None, b""
    while pos + 8 <= len(data):
        tag, size = data[pos : pos + 4], struct.unpack("<I", data[pos + 4 : pos + 8])[0]
        body = data[pos + 8 : pos + 8 + size]
        if tag == b"fmt ":
            fmt = struct.unpack("<HHIIHH", body[:16])
        elif tag == b"data":
            payload = body
        pos += 8 + size + (size & 1)
    if fmt is None:
        raise ValueError(f"{path}: no fmt chunk")
    tag, channels, rate, _, _, bits = fmt
    if tag == 3 and bits == 32:
        values = list(struct.unpack(f"<{len(payload) // 4}f", payload))
    elif tag == 1 and bits == 16:
        values = [v / 32768.0 for v in struct.unpack(f"<{len(payload) // 2}h", payload)]
    elif tag == 1 and bits == 24:
        values = [
            int.from_bytes(payload[k : k + 3], "little", signed=True) / 8388608.0
            for k in range(0, len(payload) - 2, 3)
        ]
    elif tag == 1 and bits == 32:
        values = [v / 2147483648.0 for v in struct.unpack(f"<{len(payload) // 4}i", payload)]
    else:
        raise ValueError(f"{path}: unsupported WAV format tag {tag} / {bits} bits")
    cols = {f"ch{c}": values[c::channels] for c in range(channels)}
    return {"rate": str(rate)}, cols


def read_any(path: str | Path) -> tuple[dict[str, str], dict[str, list[float]]]:
    return read_wav(path) if str(path).lower().endswith(".wav") else read_csv(path)


def _ordered_bits(x: float) -> int:
    """float32 bit pattern mapped to a monotonic integer (so differences count ulps)."""
    b = struct.unpack("<i", struct.pack("<f", x))[0]
    return b if b >= 0 else -(b & 0x7FFFFFFF)


def ulp_distance(a: float, b: float) -> int:
    return abs(_ordered_bits(f32(a)) - _ordered_bits(f32(b)))


def estimate_delay(a: list[float], b: list[float], max_lag: int = 16) -> float:
    """Lag `d` (samples) maximising `sum a[n] b[n - d]`: positive when `a` is `b` delayed by `d`.

    The peak is refined with a parabola through its neighbours. Returns 0.0 for empty or
    zero-energy input.
    """
    n = min(len(a), len(b))
    if n == 0:
        return 0.0
    scores = {}
    for lag in range(-max_lag, max_lag + 1):
        lo, hi = max(0, lag), min(n, n + lag)
        scores[lag] = sum(a[k] * b[k - lag] for k in range(lo, hi))
    best = max(scores, key=lambda k: scores[k])
    if scores[best] <= 0.0:
        return 0.0
    if -max_lag < best < max_lag:
        y0, y1, y2 = scores[best - 1], scores[best], scores[best + 1]
        denom = y0 - 2.0 * y1 + y2
        if denom != 0.0:
            return best + 0.5 * (y0 - y2) / denom
    return float(best)


def compare(oracle: list[float], ref: list[float], max_lag: int = 16) -> dict:
    """Error statistics of `oracle` against `ref` over their common length."""
    n = min(len(oracle), len(ref))
    err = [oracle[k] - ref[k] for k in range(n)]
    sig = sum(ref[k] * ref[k] for k in range(n))
    noise = sum(e * e for e in err)
    worst = max(range(n), key=lambda k: abs(err[k])) if n else 0
    return {
        "n": n,
        "len_oracle": len(oracle),
        "len_ref": len(ref),
        "max_abs": abs(err[worst]) if n else 0.0,
        "max_abs_index": worst,
        "rms": math.sqrt(noise / n) if n else 0.0,
        "snr_db": math.inf
        if noise == 0.0
        else (-math.inf if sig == 0.0 else 10.0 * math.log10(sig / noise)),
        "max_ulp": max((ulp_distance(oracle[k], ref[k]) for k in range(n)), default=0),
        "exact": sum(1 for k in range(n) if f32(oracle[k]) == f32(ref[k])),
        "delay": estimate_delay(oracle, ref, max_lag),
    }


def sparkline(values: list[float], width: int = 64) -> str:
    """One-line profile of `values` (max of |v| per bucket) with block characters."""
    if not values:
        return ""
    bucket = max(1, math.ceil(len(values) / width))
    peaks = [max(abs(v) for v in values[k : k + bucket]) for k in range(0, len(values), bucket)]
    top = max(peaks)
    if top == 0.0:
        return "_" * len(peaks)
    blocks = " .:-=+*#%@"
    return "".join(
        blocks[min(len(blocks) - 1, int(p / top * (len(blocks) - 1) + 0.5))] for p in peaks
    )


def text_plot(values: list[float], width: int = 64, height: int = 9) -> list[str]:
    """ASCII plot of `values` (column = bucket mean), `height` rows, zero line marked."""
    if not values:
        return []
    bucket = max(1, math.ceil(len(values) / width))
    cols = [
        sum(values[k : k + bucket]) / len(values[k : k + bucket])
        for k in range(0, len(values), bucket)
    ]
    lo, hi = min(cols + [0.0]), max(cols + [0.0])
    span = (hi - lo) or 1.0
    rows = []
    for r in range(height):
        level = hi - span * r / (height - 1)
        row = "".join(
            "*"
            if abs(c - level) <= span / (2 * (height - 1))
            else ("-" if abs(level) <= span / (2 * (height - 1)) else " ")
            for c in cols
        )
        rows.append(f"{level:+10.4g} |{row}")
    return rows


def pairs_for(
    oracle: dict[str, list[float]], ref: dict[str, list[float]], spec: str | None
) -> list[tuple[str, str]]:
    if spec:
        out = []
        for item in spec.split(","):
            a, _, b = item.partition(":")
            out.append((a, b or a))
        return out
    return [(k, k) for k in oracle if k in ref and not k.startswith("src")]


def report(name: str, stats: dict, oracle: list[float], ref: list[float], plot: bool) -> list[str]:
    lines = [
        f"series {name}: n={stats['n']} (oracle {stats['len_oracle']}, ref {stats['len_ref']})",
        f"  max |err|  {stats['max_abs']:.6e} at index {stats['max_abs_index']}",
        f"  rms err    {stats['rms']:.6e}",
        f"  SNR        {stats['snr_db']:.2f} dB",
        f"  max ulp    {stats['max_ulp']}   bit-exact {stats['exact']}/{stats['n']}",
        f"  delay      {stats['delay']:+.3f} samples (oracle relative to ref)",
    ]
    if plot:
        n = stats["n"]
        err = [oracle[k] - ref[k] for k in range(n)]
        lines.append("  reference:")
        lines += ["    " + r for r in text_plot(ref[:n])]
        lines.append("  error profile (max |err| per bucket): " + sparkline(err))
    return lines


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("oracle")
    ap.add_argument("ref")
    ap.add_argument("--series", help="comma list of names or oracle:ref pairs")
    ap.add_argument("--max-lag", type=int, default=16)
    ap.add_argument("--tol-abs", type=float, help="fail when max |err| exceeds this")
    ap.add_argument("--tol-snr", type=float, help="fail when SNR is below this many dB")
    ap.add_argument("--plot", action="store_true")
    ap.add_argument("--quiet", action="store_true", help="one summary line per series")
    args = ap.parse_args(argv)
    try:
        o_meta, o_cols = read_any(args.oracle)
        _, r_cols = read_any(args.ref)
    except (OSError, ValueError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
    pairs = pairs_for(o_cols, r_cols, args.series)
    if not pairs:
        print("error: no common series to compare", file=sys.stderr)
        return 2
    failed = False
    for a, b in pairs:
        if a not in o_cols or b not in r_cols:
            print(f"error: series {a}:{b} missing", file=sys.stderr)
            return 2
        stats = compare(o_cols[a], r_cols[b], args.max_lag)
        bad = (args.tol_abs is not None and stats["max_abs"] > args.tol_abs) or (
            args.tol_snr is not None and stats["snr_db"] < args.tol_snr
        )
        failed |= bad
        if args.quiet:
            print(
                f"{a}: n={stats['n']} max_abs={stats['max_abs']:.3e} rms={stats['rms']:.3e} "
                f"snr={stats['snr_db']:.1f}dB max_ulp={stats['max_ulp']} "
                f"delay={stats['delay']:+.3f}" + (" FAIL" if bad else "")
            )
        else:
            print("\n".join(report(a, stats, o_cols[a], r_cols[b], args.plot)))
            if bad:
                print("  FAIL: tolerance exceeded")
    if o_meta.get("scenario") and not args.quiet:
        print(f"scenario: {o_meta['scenario']}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
