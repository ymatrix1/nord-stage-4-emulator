#!/usr/bin/env python3
"""Chorus hardware clip: scope edge separation of the video against the emulator chorus.

The clip (`docs/research/fx-chorus-hw-clip.md`) shows a phone filming a DAW oscilloscope with a
decaying sawtooth (period 11.45 ms, 87.31 Hz). Every period has a small step before the main
edge: the chorus copy delayed against the dry edge. The horizontal distance between the two
vertical edges, divided by the edge-to-edge period of the same trace, is the delay in periods
(`sep / period * 11.454 ms`).

Subcommands (video, audio and frames stay in ``--workdir``, never committed):

* ``scope CLIP.mp4 --out scope.csv``: per frame (4 per second) edge separation and period in
  pixels of each trace, plus the delay in ms.
* ``probe``: renders the same sawtooth through ``examples/fx_chorus_probe`` (firmware ``fw4`` /
  ``fw5`` and the engine chorus) and prints delay min / max / mean, LFO rate and edge height ratio.
* ``stats scope.csv``: min, max, mean delay and LFO rate of the clip rows.
"""

from __future__ import annotations

import argparse
import csv
import subprocess
import sys
from pathlib import Path
from typing import Sequence

import numpy as np

F0_HZ = 87.31  # EMPIRICAL: harmonic comb fit of L.wav 24..34 s (same pitch as the scope period)
PERIOD_MS = 1000.0 / F0_HZ
EDGE_MIN_RUN_PX = 40  # a vertical edge spans at least this many bright pixels
BRIGHT = 0.8  # trace pixels are near white
PAIR_DX_PX = (15, 130)  # the second edge follows the first within this distance
PERIOD_DX_PX = (350, 650)
FRAME_W, FRAME_H = 1080, 1920
EXE = Path("target/release/examples/fx_chorus_probe")
# (mode, rate word, amount) of the ``grid`` table: the clip rate (word 30 = 0.343 Hz) at several
# amounts, two other rates, and the engine chorus at 0.34 Hz.
GRID = [("fw4", 30, a) for a in (8, 16, 32, 48, 64, 90, 127)] + [
    ("fw4", 10, 127), ("fw4", 60, 127), ("fw5", 30, 127),
    ("engine", 0, 64), ("engine", 0, 127), ("engine5", 0, 127),
]  # fmt: skip
GRID_KEYS = ("min", "max", "mean", "rate_hz", "h_ratio")


def column_runs(mask: np.ndarray, min_run: int = EDGE_MIN_RUN_PX) -> list[tuple[int, int, int]]:
    """(x, y0, y1) of every vertical run of at least ``min_run`` set pixels."""
    out = []
    for x in range(mask.shape[1]):
        d = np.diff(np.concatenate([[0], mask[:, x].astype(np.int8), [0]]))
        for a, b in zip(np.where(d == 1)[0], np.where(d == -1)[0]):
            if b - a >= min_run:
                out.append((x, int(a), int(b)))
    return out


def edge_groups(mask: np.ndarray, min_run: int = EDGE_MIN_RUN_PX) -> list[tuple[float, int, int]]:
    """Vertical edges (mean x, y0, y1): runs of adjacent columns with overlapping y ranges."""
    groups: list[dict] = []
    for x, a, b in column_runs(mask, min_run):
        for g in groups:
            if x - g["x1"] <= 3 and min(b, g["b"]) - max(a, g["a"]) > 0.5 * (b - a):
                g.update(x1=x, a=min(g["a"], a), b=max(g["b"], b))
                g["xs"].append(x)
                break
        else:
            groups.append({"x1": x, "xs": [x], "a": a, "b": b})
    return [(float(np.mean(g["xs"])), g["a"], g["b"]) for g in groups]


def trace_pairs(edges: Sequence[tuple[float, int, int]]) -> list[dict]:
    """Edge pairs (first edge, second edge up and to the right) with a same-trace period partner.

    Returns dicts with ``y`` (screen row of the first edge), ``sep`` and ``period`` in pixels
    and ``h``, the height of the second edge over the first.
    """
    pairs = []
    for xa, a0, a1 in edges:
        for xb, b0, b1 in edges:
            if PAIR_DX_PX[0] < xb - xa < PAIR_DX_PX[1] and a0 - 260 < b0 < a0 - 20:
                pairs.append((xa, xb - xa, a0, (b1 - b0) / (a1 - a0)))
    out = []
    for xa, sep, y, hr in pairs:
        for xc, sep2, y2, hr2 in pairs:
            dx = xc - xa
            if PERIOD_DX_PX[0] < dx < PERIOD_DX_PX[1] and abs(y2 - y) < 40 and abs(sep2 - sep) < 25:
                out.append({"y": y, "sep": 0.5 * (sep + sep2), "period": dx, "h": 0.5 * (hr + hr2)})
    return out


def cluster_traces(rows: Sequence[dict], gap_px: int = 100) -> list[dict]:
    """Merge ``trace_pairs`` rows into one median row per trace (split by screen row ``y``)."""
    rows = sorted(rows, key=lambda r: r["y"])
    clusters: list[list[dict]] = []
    for r in rows:
        if clusters and r["y"] - clusters[-1][-1]["y"] <= gap_px:
            clusters[-1].append(r)
        else:
            clusters.append([r])
    return [
        {
            "y": float(np.median([r["y"] for r in c])),
            "sep": float(np.median([r["sep"] for r in c])),
            "period": float(np.median([r["period"] for r in c])),
            "h": float(np.median([r["h"] for r in c])),
        }
        for c in clusters
    ]


def delay_ms(sep_px: float, period_px: float) -> float:
    """Edge separation in ms, calibrated by the sawtooth period."""
    return sep_px / period_px * PERIOD_MS


def scope_rows(clip: Path, t0: float, dur: float, fps: int = 4) -> list[dict]:
    """Per frame, per trace (0 = upper on screen) separation rows of the video."""
    cmd = [
        "ffmpeg", "-v", "error", "-ss", str(t0), "-t", str(dur), "-i", str(clip),
        "-vf", f"fps={fps},scale={FRAME_W}:{FRAME_H}", "-f", "rawvideo", "-pix_fmt", "rgb24", "-",
    ]  # fmt: skip
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE)
    assert proc.stdout is not None
    size = FRAME_W * FRAME_H * 3
    rows, i = [], 0
    while len(buf := proc.stdout.read(size)) == size:
        img = np.frombuffer(buf, np.uint8).reshape(FRAME_H, FRAME_W, 3)
        mask = img.min(2) > BRIGHT * 255
        mask[:420] = False
        mask[1750:] = False
        for k, tr in enumerate(cluster_traces(trace_pairs(edge_groups(mask)))):
            rows.append(
                {"t": t0 + i / fps, "trace": k, **tr, "ms": delay_ms(tr["sep"], tr["period"])}
            )
        i += 1
    return rows


def lfo_stats(t: np.ndarray, d: np.ndarray) -> dict:
    """Min, max, mean of ``d`` (5th / 95th percentile for min / max) and the LFO rate (Hz).

    The rate is the spectral peak (0.05 to 3 Hz, parabolic) of ``d`` resampled to 20 Hz
    (EMPIRICAL: needs at least two cycles in ``t``).
    """
    t, d = np.asarray(t, float), np.asarray(d, float)
    mean = float(d.mean())
    grid = np.arange(t[0], t[-1], 0.05)
    u = np.interp(grid, t, d) - mean
    n = 1 << 14
    spec = np.abs(np.fft.rfft(u * np.hanning(len(u)), n))
    fr = np.fft.rfftfreq(n, 0.05)
    k = int(np.argmax(np.where((fr > 0.05) & (fr < 3.0), spec, 0.0)))
    a0, b0, c0 = np.log(spec[k - 1 : k + 2] + 1e-30)
    shift = 0.5 * (a0 - c0) / (a0 - 2 * b0 + c0)
    rate = float((k + shift) * (fr[1] - fr[0]))
    return {
        "min": float(np.percentile(d, 5)), "max": float(np.percentile(d, 95)), "mean": mean,
        "rate_hz": rate, "n": len(d),
    }  # fmt: skip


def saw_decay(fs: float, seconds: float, f0: float = F0_HZ, tau: float = 3.0) -> np.ndarray:
    """Scope test signal: an upward edge each period, then the decay ``exp(-tau p)``."""
    ph = (np.arange(int(fs * seconds)) * f0 / fs) % 1.0
    return (0.5 * (np.exp(-tau * ph) - 0.3)).astype(np.float32)


def edge_separation(x: np.ndarray, y: np.ndarray, fs: float, f0: float = F0_HZ) -> np.ndarray:
    """Rows ``(t, sep_ms, h_dry, h_wet)`` of the dry and delayed edge of ``y`` per period of ``x``.

    The dry edge is the largest rise within 0.5 ms of the input edge; the delayed edge is the
    largest rise 0.25 to 9 ms after it (a step of at least 8 % of the dry height).
    """
    dx, dy = np.diff(x), np.diff(y)
    per = fs / f0
    lo, hi, w = int(0.25e-3 * fs), int(9e-3 * fs), int(0.5e-3 * fs)
    out = []
    for k in range(int(0.5 * f0) + 1, int(len(dx) / per) - 2):
        e = k * per
        a = int(e) - 3 + int(np.argmax(dx[int(e) - 3 : int(e) + 3]))
        dry = a - w + int(np.argmax(dy[a - w : a + w]))
        seg = dy[dry + lo : dry + hi]
        j = dry + lo + int(np.argmax(seg))
        if seg.max() < 0.08 * dy[dry]:
            continue

        # parabolic refinement of the two peaks, heights as the 3 sample sums
        def refine(i: int) -> float:
            a0, b0, c0 = dy[i - 1], dy[i], dy[i + 1]
            den = a0 - 2 * b0 + c0
            return i + (0.5 * (a0 - c0) / den if den else 0.0)

        out.append((dry / fs, (refine(j) - refine(dry)) / fs * 1000.0,
                    float(dy[dry - 1: dry + 2].sum()), float(dy[j - 1: j + 2].sum())))  # fmt: skip
    return np.array(out)


def run_probe(x: np.ndarray, mode: str, rate: int, amount: int, fs: float, hz: float, work: Path):
    """Stereo output of ``examples/fx_chorus_probe`` for the mono input ``x``."""
    work.mkdir(parents=True, exist_ok=True)
    x.astype("<f4").tofile(work / "in.f32")
    args = [str(EXE), str(work / "in.f32"), str(work / "out.f32"), mode, str(rate), str(amount)]
    subprocess.run([*args, str(fs), str(hz)], check=True)
    return np.fromfile(work / "out.f32", dtype="<f4").reshape(-1, 2)


def probe_summary(mode: str, rate: int, amount: int, hz: float, work: Path, fs: float = 48_000.0):
    """Delay statistics of the delayed edge of every output channel of one chorus setting."""
    x = saw_decay(fs, 14.0)
    y = run_probe(x, mode, rate, amount, fs, hz, work)
    res = {}
    for ch in (0, 1):
        r = edge_separation(x, y[:, ch], fs)
        if len(r) < 10:
            res[ch] = None
            continue
        res[ch] = {**lfo_stats(r[:, 0], r[:, 1]), "h_ratio": float(np.median(r[:, 3] / r[:, 2])),
                   "found": len(r)}  # fmt: skip
    return res


def main(argv: Sequence[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    sc = sub.add_parser("scope")
    sc.add_argument("clip", type=Path)
    sc.add_argument("--out", type=Path, required=True)
    sc.add_argument("--t0", type=float, default=22.5)
    sc.add_argument("--dur", type=float, default=13.0)
    st = sub.add_parser("stats")
    st.add_argument("csv", type=Path)
    gr = sub.add_parser("grid")
    gr.add_argument("--workdir", type=Path, required=True)
    gr.add_argument("--out", type=Path, required=True)
    pr = sub.add_parser("probe")
    pr.add_argument("--workdir", type=Path, required=True)
    pr.add_argument("--mode", default="fw4")
    pr.add_argument("--rate", type=int, default=30)
    pr.add_argument("--amount", type=int, default=127)
    pr.add_argument("--hz", type=float, default=0.35)
    a = ap.parse_args(argv)
    if a.cmd == "scope":
        rows = scope_rows(a.clip, a.t0, a.dur)
        with a.out.open("w", newline="") as f:
            w = csv.DictWriter(f, ["t", "trace", "y", "sep", "period", "h", "ms"])
            w.writeheader()
            w.writerows({k: round(v, 3) for k, v in r.items()} for r in rows)
        print(f"{len(rows)} rows")
    elif a.cmd == "stats":
        rows = list(csv.DictReader(a.csv.open()))
        for k in sorted({r["trace"] for r in rows}):
            sel = [r for r in rows if r["trace"] == k]
            print(
                k,
                lfo_stats(
                    np.array([float(r["t"]) for r in sel]), np.array([float(r["ms"]) for r in sel])
                ),
            )
    elif a.cmd == "grid":
        with a.out.open("w", newline="") as f:
            w = csv.writer(f)
            w.writerow(["mode", "rate", "amount", "ch", "min_ms", "max_ms", "mean_ms", "rate_hz",
                        "h_ratio"])  # fmt: skip
            for mode, rate, amount in GRID:
                for ch, v in probe_summary(mode, rate, amount, 0.34, a.workdir).items():
                    if v:
                        w.writerow([mode, rate, amount, ch, *(f"{v[k]:.3f}" for k in GRID_KEYS)])
    else:
        print(probe_summary(a.mode, a.rate, a.amount, a.hz, a.workdir))
    return 0


if __name__ == "__main__":
    sys.exit(main())
