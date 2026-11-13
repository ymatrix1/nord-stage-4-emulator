#!/usr/bin/env python3
"""Run the complete PNO core 1 pass (`sharc-pass`) on ARM message scripts and compare its audio.

* `run`      scripts -> `sharc-pass` -> `(frames, 2)` float lanes of one output pair (`.npy`, WAV)
* `compare`  the same scripts through the single-routine harness `sharc-organ` (bank + mix stage
             only) against the full pass: best lag, best gain, SNR, per-partial amplitudes

    python tools/pno_full_pass.py run SRAM/core1_sram.bin --exe target/release/sharc-pass \
        --script vox.script --script keys.script --passes 3000 --out vox
    python tools/pno_full_pass.py compare SRAM/core1_sram.bin --pass-exe target/release/sharc-pass \
        --organ-exe target/release/sharc-organ --script model3.script --script bank.script \
        --script keys.script --passes 4000

Layout of one pass (VERIFIED, `docs/research/pno-full-pass.md` section 3): 2 frames per pass, the
output pair of pointer set `k` is read after the pass as `[L0 R0 L1 R1]`.
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np

# VERIFIED (`sharc-pass --raw`): output pointer sets x pairs x words per pass.
SETS, PAIRS, WORDS_PER_PAIR = 6, 3, 4
PASS_WORDS = SETS * PAIRS * WORDS_PER_PAIR
# VERIFIED (run, pipe chord): the final audio is in pointer set 2, pair 0.
DEFAULT_SET, DEFAULT_PAIR = 2, 0
# ASSUMED: level target `DM(0x95B42)`, no captured ARM message sets it (organ-oracle-audio.md s4).
LEVEL_WORD, ONE = 0x95B42, 0x3F800000


def parse_raw(data: bytes, set_index: int = DEFAULT_SET, pair: int = DEFAULT_PAIR) -> np.ndarray:
    """`(2 * passes, 2)` float32 frames (L, R) of one output pair from `sharc-pass --raw`."""
    words = np.frombuffer(data, dtype="<f4")
    per_pass = words.reshape(-1, PASS_WORDS)
    base = (set_index * PAIRS + pair) * WORDS_PER_PAIR
    quad = per_pass[:, base : base + WORDS_PER_PAIR]  # L0 R0 L1 R1
    return quad.reshape(-1, 2).copy()


def best_fit(ref: np.ndarray, sig: np.ndarray, max_lag: int = 8) -> tuple[int, float, float]:
    """`(lag, gain, snr_db)` of `sig` ~ gain * ref shifted by `lag` frames (least squares)."""
    best = (0, 0.0, -np.inf)
    n = min(len(ref), len(sig)) - 2 * max_lag
    for lag in range(-max_lag, max_lag + 1):
        a = ref[max_lag + lag : max_lag + lag + n].astype(np.float64).ravel()
        b = sig[max_lag : max_lag + n].astype(np.float64).ravel()
        den = float(a @ a)
        if den == 0.0:
            continue
        g = float(a @ b) / den
        err = float(((b - g * a) ** 2).sum())
        snr = 10 * np.log10(float((b**2).sum()) / err) if err > 0 else np.inf
        if snr > best[2]:
            best = (lag, g, float(snr))
    return best


def level_script() -> str:
    return f"0 {LEVEL_WORD:#x} {ONE:#010x}  # level L (ASSUMED 1.0)\n"


def run_pass(
    exe: str, image: str, scripts: list[str], passes: int, set_index: int, pair: int
) -> np.ndarray:
    """Runs `sharc-pass` and returns the `(2 * passes, 2)` lanes."""
    with tempfile.TemporaryDirectory() as tmp:
        raw = Path(tmp) / "out.raw"
        cmd = [os.path.abspath(exe), image, "--passes", str(passes), "--quiet", "--raw", str(raw)]
        for s in scripts:
            cmd += ["--script", s]
        subprocess.run(cmd, check=True, capture_output=True)
        return parse_raw(raw.read_bytes(), set_index, pair)


def script_pokes(paths: list[str]) -> list[tuple[int, int, int]]:
    """`(call, addr, value)` of script files in the `sharc-organ --script` format."""
    out = []
    for p in paths:
        for line in Path(p).read_text(encoding="utf-8").splitlines():
            f = line.split("#")[0].split()
            if len(f) == 3:
                out.append((int(f[0], 0), int(f[1], 0), int(f[2], 0)))
    return out


def organ_frames(exe: str, image: str, scripts: list[str], calls: int) -> np.ndarray:
    """Bank + mix stage only: `(2 * calls, 2)` (out_x, out_y) through `sharc-organ`."""
    import organ_oracle_audio as oa

    return oa.run_oracle(exe, image, script_pokes(scripts), calls)


def cmd_run(a: argparse.Namespace) -> int:
    scripts = [*a.script, *([a.level_script] if a.level_script else [])]
    frames = run_pass(a.exe, a.image, scripts, a.passes, a.set, a.pair)
    print(
        f"{len(frames)} frames, peak {np.abs(frames).max(axis=0)}, "
        f"rms {np.sqrt((frames.astype(np.float64) ** 2).mean(axis=0))}"
    )
    if a.out:
        np.save(f"{a.out}.f32.npy", frames)
    return 0


def cmd_compare(a: argparse.Namespace) -> int:
    with tempfile.TemporaryDirectory() as tmp:
        lvl = Path(tmp) / "level.script"
        lvl.write_text(level_script(), encoding="utf-8")
        scripts = [*a.script, str(lvl)]
        full = run_pass(a.pass_exe, a.image, scripts, a.passes, a.set, a.pair)
        mix = organ_frames(a.organ_exe, a.image, scripts, a.passes)
    skip = 2 * a.skip
    lag, gain, snr = best_fit(mix[skip:, :], full[skip:, :])
    print(f"full pass vs bank+mix: lag {lag} frames, gain {gain:.5f}, SNR {snr:.1f} dB")
    for name, x in (
        ("full L", full[skip:, 0]),
        ("full R", full[skip:, 1]),
        ("mix x", mix[skip:, 0]),
    ):
        rms = np.sqrt((x.astype(np.float64) ** 2).mean())
        print(f"{name}: peak {np.abs(x).max():.6f} rms {rms:.6f}")
    return 0


def read_rust_csv(path: str) -> np.ndarray:
    """`(frames, 2)` (L, R) of an `organ_ref --out PREFIX` CSV (`index,L,R`)."""
    rows = np.loadtxt(path, delimiter=",", comments="#", skiprows=3, ndmin=2)
    return rows[:, 1:3].astype(np.float32)


def cmd_rust(a: argparse.Namespace) -> int:
    with tempfile.TemporaryDirectory() as tmp:
        lvl = Path(tmp) / "level.script"
        lvl.write_text(level_script(), encoding="utf-8")
        full = run_pass(a.pass_exe, a.image, [*a.script, str(lvl)], a.passes, a.set, a.pair)
    rust = read_rust_csv(a.csv)
    skip = 2 * a.skip
    n = min(len(full), len(rust))
    lag, gain, snr = best_fit(full[skip:n, :1], rust[skip:n, :1], a.max_lag)
    print(f"rust vs full pass (lane L): lag {lag} frames, gain {gain:.5f}, SNR {snr:.1f} dB")
    for name, x in (("full L", full[skip:n, 0]), ("rust L", rust[skip:n, 0])):
        rms = np.sqrt((x.astype(np.float64) ** 2).mean())
        print(f"{name}: peak {np.abs(x).max():.6f} rms {rms:.6f}")
    return 0


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run")
    r.add_argument("image")
    r.add_argument("--exe", default="target/release/sharc-pass")
    r.add_argument("--script", action="append", default=[])
    r.add_argument("--level-script")
    r.add_argument("--passes", type=int, default=2000)
    r.add_argument("--set", type=int, default=DEFAULT_SET)
    r.add_argument("--pair", type=int, default=DEFAULT_PAIR)
    r.add_argument("--out")
    r.set_defaults(fn=cmd_run)
    c = sub.add_parser("compare")
    c.add_argument("image")
    c.add_argument("--pass-exe", default="target/release/sharc-pass")
    c.add_argument("--organ-exe", default="target/release/sharc-organ")
    c.add_argument("--script", action="append", default=[])
    c.add_argument("--passes", type=int, default=4000)
    c.add_argument("--skip", type=int, default=1500, help="passes dropped (smoother settling)")
    c.add_argument("--set", type=int, default=DEFAULT_SET)
    c.add_argument("--pair", type=int, default=DEFAULT_PAIR)
    c.set_defaults(fn=cmd_compare)
    u = sub.add_parser("rust")
    u.add_argument("image")
    u.add_argument("csv", help="`organ_ref --out PREFIX` PREFIX.csv")
    u.add_argument("--pass-exe", default="target/release/sharc-pass")
    u.add_argument("--script", action="append", default=[])
    u.add_argument("--passes", type=int, default=4000)
    u.add_argument("--skip", type=int, default=1500)
    u.add_argument("--max-lag", type=int, default=64)
    u.add_argument("--set", type=int, default=DEFAULT_SET)
    u.add_argument("--pair", type=int, default=DEFAULT_PAIR)
    u.set_defaults(fn=cmd_rust)
    ns = ap.parse_args(argv)
    return ns.fn(ns)


if __name__ == "__main__":
    sys.exit(main())
