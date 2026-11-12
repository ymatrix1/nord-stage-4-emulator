#!/usr/bin/env python3
"""Find monotone conversion tables in a flat SHARC SRAM image and fit closed forms to them.

Input is a `core{1,2}_sram.bin` written by `python tools/ns4_dsp_ldr.py dsp_image_*.bin --sram DIR`.
The flat image holds one 32-bit little-endian word per DM address, so DM word `A` sits at byte
`4 * A` (checked on the ORG channel strips at `DM(0xE0000 + 62 k)`).

A table is a run of finite floats that moves in one direction (equal values at the end are the
interpolation guard entries and are dropped). Each table is fitted with these models; the model
with the smallest error wins, error is reported in cents of ratio (geometric kinds) or in percent
of the span (linear, power):

    linear      y = a + b i
    geometric   y = a r^i                         (r in dB per step and semitones per step)
    prewarp     atan(y) = a r^i                   (zero-delay filter `g = tan(pi f / fs)` over a
                                                  geometric frequency axis)
    power       y = y0 + (y1 - y0) (i / (n-1))^p

    python tools/ns4_param_curves.py scan core2_sram.bin
    python tools/ns4_param_curves.py scan core2_sram.bin --min-len 100 --lo 0x90000 --hi 0xE0000
    python tools/ns4_param_curves.py at core2_sram.bin 0xB0A63 213
"""

from __future__ import annotations

import argparse
import math
import statistics
import struct
import sys
from dataclasses import dataclass
from pathlib import Path

SEMITONE = 2.0 ** (1.0 / 12.0)
# Step ratios that deserve a name (label, ratio per step).
NAMED_RATIOS = (
    ("0.25 dB", 10 ** (0.25 / 20)),
    ("0.5 dB", 10 ** (0.5 / 20)),
    ("1 dB", 10 ** (1.0 / 20)),
    ("1.5 dB", 10 ** (1.5 / 20)),
    ("3 dB", 10 ** (3.0 / 20)),
    ("2^(1/12) = 1 semitone", SEMITONE),
    ("2^(1/24) = 6.02 dB/24", 2.0 ** (1.0 / 24.0)),
    ("2^(1/16)", 2.0 ** (1.0 / 16.0)),
    ("2^(1/8)", 2.0 ** (1.0 / 8.0)),
)


@dataclass(frozen=True)
class Fit:
    model: str
    err: float  # cents for geometric/prewarp, percent of span for linear/power
    params: dict[str, float]

    def describe(self) -> str:
        unit = "cents" if self.model in ("geometric", "prewarp") else "% span"
        body = " ".join(f"{k}={v:.6g}" for k, v in self.params.items())
        return f"{self.model:9s} err {self.err:.3g} {unit}  {body}"


@dataclass(frozen=True)
class Table:
    dm: int  # DM word address of entry 0
    values: tuple[float, ...]


def load_f32(data: bytes) -> list[float]:
    """Reinterpret a flat SRAM image as 32-bit floats (index = DM word address)."""
    return list(struct.unpack(f"<{len(data) // 4}f", data[: len(data) // 4 * 4]))


def _usable(x: float) -> bool:
    return x == x and abs(x) < 1e9


def _linfit(xs: list[float], ys: list[float]) -> tuple[float, float]:
    n = len(xs)
    mx, my = sum(xs) / n, sum(ys) / n
    sxx = sum((x - mx) ** 2 for x in xs)
    if sxx == 0:
        return my, 0.0
    b = sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / sxx
    return my - b * mx, b


def trim_guard(y: list[float]) -> list[float]:
    """Drop the repeated tail values (interpolation guard entries), keeping one."""
    out = list(y)
    while len(out) > 2 and out[-1] == out[-2]:
        out.pop()
    return out


def fit_linear(y: list[float]) -> Fit:
    idx = [float(i) for i in range(len(y))]
    a, b = _linfit(idx, y)
    span = abs(y[-1] - y[0]) or 1.0
    err = max(abs(a + b * i - v) for i, v in zip(idx, y)) / span * 100
    return Fit("linear", err, {"a": a, "b": b})


def _geometric(y: list[float], model: str, extra: dict[str, float]) -> Fit | None:
    if any(v <= 0 for v in y) or len(y) < 3:
        return None
    ly = [math.log(v) for v in y]
    idx = [float(i) for i in range(len(y))]
    a, b = _linfit(idx, ly)
    cents = max(abs(a + b * i - v) for i, v in zip(idx, ly)) / math.log(2) * 1200
    params = {"y0": math.exp(a), "ratio": math.exp(b), "db_step": 20 * b / math.log(10)}
    params["semitones_step"] = 12 * b / math.log(2)
    params.update(extra)
    return Fit(model, cents, params)


def fit_geometric(y: list[float]) -> Fit | None:
    """`y = y0 * ratio^i` by least squares on `ln y`."""
    return _geometric(y, "geometric", {})


def fit_prewarp(y: list[float]) -> Fit | None:
    """`atan(y) = u0 * ratio^i`: a tangent prewarp over a geometric frequency axis."""
    if any(v <= 0 for v in y):
        return None
    f = _geometric([math.atan(v) for v in y], "prewarp", {})
    if f is None:
        return None
    params = dict(f.params)
    params["u0"] = params.pop("y0")
    return Fit("prewarp", f.err, params)


def fit_power(y: list[float]) -> Fit:
    """`y = y0 + (y1 - y0) (i/(n-1))^p`, exponent by golden-section search on `[0.1, 10]`."""
    n = len(y)
    y0, y1 = y[0], y[-1]
    span = abs(y1 - y0) or 1.0

    def err(p: float) -> float:
        return max(abs(y0 + (y1 - y0) * (i / (n - 1)) ** p - v) for i, v in enumerate(y))

    lo, hi = 0.1, 10.0
    g = (math.sqrt(5) - 1) / 2
    c, d = hi - g * (hi - lo), lo + g * (hi - lo)
    for _ in range(80):
        if err(c) < err(d):
            hi, d = d, c
            c = hi - g * (hi - lo)
        else:
            lo, c = c, d
            d = lo + g * (hi - lo)
    p = (lo + hi) / 2
    return Fit("power", err(p) / span * 100, {"y0": y0, "y1": y1, "p": p})


def best_fit(y: list[float]) -> Fit:
    """The model with the smallest error. Cents and percent of span are compared after scaling
    1 percent of span to 10 cents so that a clean geometric table beats a rough linear one."""
    fits = [fit_linear(y), fit_power(y)]
    for f in (fit_geometric(y), fit_prewarp(y)):
        if f is not None:
            fits.append(f)

    def score(f: Fit) -> float:
        return f.err if f.model in ("geometric", "prewarp") else f.err * 10

    return min(fits, key=score)


def _prefix_fit(fn, y: list[float], tol: float, min_len: int) -> tuple[Fit, int] | None:
    """Longest prefix of `y` that `fn` fits within `tol` (binary search, monotone error)."""
    lo, hi = min_len, len(y)
    best: tuple[Fit, int] | None = None
    while lo <= hi:
        mid = (lo + hi) // 2
        f = fn(y[:mid])
        if f is not None and f.err <= tol:
            best, lo = (f, mid), mid + 1
        else:
            hi = mid - 1
    return best


@dataclass(frozen=True)
class Segment:
    start: int
    length: int
    fit: Fit


def segments(y: list[float], tol: float = 3.0, min_len: int = 8) -> list[Segment]:
    """Greedy piecewise description: at each position the model (geometric, prewarp or linear)
    that stays within `tol` (cents, or percent of span x 10) for the most entries. Entries that fit
    nothing for `min_len` values are skipped one at a time."""
    out: list[Segment] = []
    i = 0
    while i < len(y) - min_len + 1:
        rest = y[i:]
        cands = []
        for fn, scale in ((fit_geometric, 1.0), (fit_prewarp, 1.0), (fit_linear, 10.0)):
            r = _prefix_fit(lambda s, fn=fn: fn(s), rest, tol / scale, min_len)
            if r is not None:
                cands.append(r)
        if not cands:
            i += 1
            continue
        fit, n = max(cands, key=lambda c: c[1])
        out.append(Segment(i, n, fit))
        i += n
    return out


def name_ratio(ratio: float, tol: float = 3e-5) -> str:
    """Label of the named step ratio within `tol` (relative), or empty."""
    for label, r in NAMED_RATIOS:
        if abs(ratio / r - 1) < tol:
            return label
    return ""


def split_monotone(vals: list[float], lo: int, hi: int, min_len: int = 24) -> list[tuple[int, int]]:
    """`(start, end)` index ranges (end exclusive) of one-direction runs of at least `min_len`.

    A run ends at a direction reversal, at a non-finite value, or where one step exceeds 8 times
    the median of the previous eight steps (a new table starting).
    """
    runs: list[tuple[int, int]] = []
    i = lo
    while i < hi - 1:
        if not _usable(vals[i]):
            i += 1
            continue
        j, direction = i + 1, 0
        while j < hi and _usable(vals[j]):
            step = vals[j] - vals[j - 1]
            sign = (step > 0) - (step < 0)
            if sign:
                if direction and sign != direction:
                    break
                direction = sign
                prev = [abs(vals[k + 1] - vals[k]) for k in range(max(i, j - 9), j - 1)]
                if len(prev) >= 4 and abs(step) > 8 * statistics.median(prev) + 1e-9:
                    break
            j += 1
        if j - i >= min_len and direction:
            runs.append((i, j))
        i = max(j - 1, i + 1) if j > i + 1 else i + 1
    return runs


def scan(vals: list[float], lo: int, hi: int, min_len: int) -> list[Table]:
    """Monotone tables of at least `min_len` entries (guard entries trimmed) in DM `[lo, hi)`."""
    out = []
    for a, b in split_monotone(vals, lo, min(hi, len(vals)), min_len):
        lead = 0
        while lead < b - a - 1 and vals[a + lead] == 0:
            lead += 1  # zero padding in front of the table is not an entry
        y = trim_guard(vals[a + lead : b])
        a += lead
        # Denormal or all-zero runs are uninitialised memory, not tables.
        if len(y) >= min_len and max(abs(v) for v in y) > 1e-20:
            out.append(Table(a, tuple(y)))
    return out


def _t_neg_ln1m(y: float) -> float | None:
    return -math.log(1 - y) if 0 < y < 1 else None


def _t_odds(y: float) -> float | None:
    return y / (1 - y) if 0 < y < 1 else None


# Monotone maps tried by `relation_spread` (each turns a coefficient back into a "time-like" value).
TRANSFORMS = {
    "y": lambda y: y,
    "1/y": lambda y: 1 / y if y else None,
    "-ln(1-y)": _t_neg_ln1m,
    "y/(1-y)": _t_odds,
    "atan(y)": math.atan,
    "y^2": lambda y: y * y,
}


def relation_spread(
    y: list[float], known: list[float], transform: str, op: str = "ratio", flip: bool = False
) -> float | None:
    """Spread (max - min of the natural log) of `g(y_i) / k_i` (`op="ratio"`) or `g(y_i) * k_i`
    (`op="prod"`) over the shared length. 0 means `g(y)` is a constant multiple (or inverse
    multiple) of the known display curve; the tables are linked when the spread is a few percent."""
    n = min(len(y), len(known))
    ys = y[:n][::-1] if flip else y[:n]
    g = TRANSFORMS[transform]
    ratios = []
    for v, k in zip(ys, known[:n]):
        gv = g(v)
        if gv is None or gv <= 0 or k <= 0:
            return None
        ratios.append(gv / k if op == "ratio" else gv * k)
    logs = [math.log(r) for r in ratios]
    return max(logs) - min(logs)


def report_line(t: Table) -> str:
    y = list(t.values)
    f = best_fit(y)
    line = f"DM 0x{t.dm:05X} n={len(y):4d} {y[0]:>11.5g} .. {y[-1]:<11.5g} {f.describe()}"
    ratio = f.params.get("ratio")
    if ratio:
        label = name_ratio(ratio)
        if label:
            line += f"  [{label}/step]"
    return line


EXACT_CENTS = 1.2  # a geometric/prewarp segment this tight is a closed form, not a coincidence
EXACT_LINEAR_PCT = 1e-3
EXACT_MIN_LEN = 16


def is_exact(seg: Segment) -> bool:
    """True when the segment is long and fits to float32 rounding (`--exact` keeps only these)."""
    limit = EXACT_CENTS if seg.fit.model in ("geometric", "prewarp") else EXACT_LINEAR_PCT
    return seg.length >= EXACT_MIN_LEN and seg.fit.err <= limit


def segment_lines(t: Table, tol: float = 3.0, min_len: int = 8, exact: bool = False) -> list[str]:
    lines = []
    for seg in segments(list(t.values), tol, min_len):
        if exact and not is_exact(seg):
            continue
        label = name_ratio(seg.fit.params.get("ratio", 0.0))
        tag = f"  [{label}/step]" if label else ""
        lines.append(
            f"    [{seg.start:4d}..{seg.start + seg.length - 1:4d}] {seg.fit.describe()}{tag}"
        )
    return lines


def _int(s: str) -> int:
    return int(s, 0)


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("scan", help="list and fit every monotone table")
    s.add_argument("image", type=Path)
    s.add_argument("--min-len", type=int, default=64)
    s.add_argument("--segments", action="store_true", help="also print the piecewise fit")
    s.add_argument(
        "--exact", action="store_true", help="piecewise fit: only float32-exact segments"
    )
    s.add_argument("--lo", type=_int, default=0x90000)
    s.add_argument("--hi", type=_int, default=0x100000)
    r = sub.add_parser("relate", help="test tables against a known 128-value display curve")
    r.add_argument("image", type=Path)
    r.add_argument("known", type=Path, help="text file with the known values, whitespace separated")
    r.add_argument("--max-spread", type=float, default=0.12)
    r.add_argument("--lo", type=_int, default=0x90000)
    r.add_argument("--hi", type=_int, default=0x100000)
    a = sub.add_parser("at", help="fit the table at one DM address")
    a.add_argument("image", type=Path)
    a.add_argument("addr", type=_int)
    a.add_argument("count", type=int)
    a.add_argument("--segments", action="store_true", help="also print the piecewise fit")
    args = p.parse_args(argv)
    vals = load_f32(args.image.read_bytes())
    if args.cmd == "scan":
        tables = scan(vals, args.lo, args.hi, args.min_len)
        for t in tables:
            print(report_line(t))
            if args.segments:
                lines = segment_lines(t, exact=args.exact)
                if lines:
                    print(chr(10).join(lines))
        print(f"{len(tables)} tables")
        return 0 if tables else 1
    if args.cmd == "relate":
        known = [float(x) for x in args.known.read_text().split()]
        hits = 0
        for t in scan(vals, args.lo, args.hi, 100):
            for name in TRANSFORMS:
                for op in ("ratio", "prod"):
                    for flip in (False, True):
                        sp = relation_spread(list(t.values), known, name, op, flip)
                        if sp is not None and sp < args.max_spread:
                            hits += 1
                            rev = " reversed" if flip else ""
                            print(f"DM 0x{t.dm:05X} {name} {op}{rev} spread {sp:.3f}")
        print(f"{hits} relations")
        return 0 if hits else 1
    t = Table(args.addr, tuple(trim_guard(vals[args.addr : args.addr + args.count])))
    print(report_line(t))
    if args.segments:
        print(chr(10).join(segment_lines(t)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
