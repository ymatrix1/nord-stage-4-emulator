#!/usr/bin/env python3
"""B3 Bass processor `0x122296` in the oracle (`sharc-chain`): chord lines and line gain.

The Bass processor is linear in the generator ring `0xB32E8..` (written by `0x121BBE`) and in the
weights the ARM key-down sends (`DM 0x951F1..`): window 1 (`DM 0x951F1 + k`, 56 words, ring slot
`0xB32E8 + k`) and window 2 (`DM 0x95241 + k`, 68 words, ring slot `0xB32F4 + k`), both summed
(`docs/research/organ-farfisa-bass.md` s4.2 and s10). This tool

* `chain`   runs generator + processor per call from the boot + install + key-down script and prints
            the strongest output lines (`--zero 1` / `--zero 2` blank a weight window first),
* `measure` drives one ring slot through one weight word (all other weights zero) and writes the
            per-line gain `|Y| / |R|` binned to 1/6 octave (state dependent, see s10).

    python tools/organ_bass_proc.py chain SRAM/core1_sram.bin bass_chord.script \
        --exe target/release/sharc-chain
    python tools/organ_bass_proc.py measure SRAM/core1_sram.bin bass_boot_install.script out.csv \
        --exe target/release/sharc-chain
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
import organ_b3_pass as ob
import organ_farfisa_proc as fp

# VERIFIED (listing 0x122296..0x12261D, `adi_dis`): window bases and sizes, ring slot bases.
W1_BASE, W1_WORDS, RING1 = 0x951F1, 56, 0xB32E8
W2_BASE, W2_WORDS, RING2 = 0x95241, 68, 0xB32F4
RATE = fp.RATE
# VERIFIED (run, s11): output gain accumulator written by the drawbar key-down; the processor
# output is scaled by `1 - 0.84 G` (G = this word), so `measure` pins it to 0 for a state free gain.
G_WORD = 0xB44C9
# VERIFIED (run): the processor reads frame 1 of the ring `0x68` words after frame 0.
FRAME1 = 0x68
SKIP, FFT = 1200, 8192
GEN, PROC = 0x121BBE, 0x122296


def script_words(lines: list[str]) -> dict[int, int]:
    """Last written value of every DM address of `PASS ADDR VALUE` script lines."""
    out: dict[int, int] = {}
    for line in lines:
        f = line.split("#")[0].split()
        if len(f) == 3:
            out[int(f[1], 0)] = int(f[2], 0)
    return out


def bass_steps(words: dict[int, int], image: dict[int, int]) -> tuple[str, str]:
    """`--step` strings of the generator and the Bass processor (slot order of the script)."""
    steps = ob.slot_steps(words, image)
    pick = {int(s.split(":")[0], 16): s for s in steps}
    return pick[GEN], pick[PROC]


def zero_window_lines(window: int) -> list[str]:
    """Script lines that zero weight window 1 or 2."""
    base, n = (W1_BASE, W1_WORDS) if window == 1 else (W2_BASE, W2_WORDS)
    return [f"0 {base + k:#x} 0x0" for k in range(n)]


def out_address(step: str) -> int:
    """`I0` of a `--step ENTRY:STOP:I0=..,I1=..` string (the processor output stream)."""
    for part in step.split(":")[2].split(","):
        if part.startswith("I0="):
            return int(part[3:], 0)
    raise ValueError(step)


def run_chain(  # pragma: no cover - runs the oracle
    exe: str,
    image: str,
    scripts: list[Path],
    extra: list[str],
    steps: tuple[str, str],
    calls: int,
    slot: int = 0,
) -> np.ndarray:
    """`(calls, 5)` float32 words: output stream (3 at I0), ring slot `slot` of frames 0 and 1."""
    with tempfile.TemporaryDirectory() as t:
        xs, op = os.path.join(t, "x"), os.path.join(t, "o")
        Path(xs).write_text("\n".join(extra) + "\n", encoding="utf-8")
        cmd = [os.path.abspath(exe), image, "--calls", str(calls)]
        for s in scripts:
            cmd += ["--script", str(s)]
        cmd += ["--script", xs, "--step", steps[0], "--step", steps[1]]
        cmd += ["--range", f"{out_address(steps[1]):#x}:3", "--range", f"{RING1 + slot:#x}:1"]
        cmd += ["--range", f"{RING1 + FRAME1 + slot:#x}:1"]
        cmd += ["--out", op]
        subprocess.run(cmd, check=True, capture_output=True)
        return np.fromfile(op, dtype="<u4").reshape(calls, 5).view(np.float32)


def output_frames(w: np.ndarray) -> np.ndarray:
    """Two frames per call: words 0 and 2 of the output stream."""
    return np.stack([w[:, 0], w[:, 2]], 1).reshape(-1)


def top_lines(x: np.ndarray, count: int = 8) -> list[tuple[float, float, float]]:
    """`(Hz, amplitude, dB below the strongest)` of the strongest spectral maxima (Hann)."""
    win = np.hanning(len(x))
    spec = np.abs(np.fft.rfft((x - x.mean()) * win)) * 2 / win.sum()
    peaks = [i for i in range(2, len(spec) - 1) if spec[i] > spec[i - 1] and spec[i] >= spec[i + 1]]
    peaks.sort(key=lambda i: -spec[i])
    top = spec[peaks[0]] if peaks else 1.0
    return [
        (i * RATE / len(x), float(spec[i]), 20 * float(np.log10(spec[i] / top)))
        for i in peaks[:count]
    ]


def load(a: argparse.Namespace) -> tuple[list[str], dict[int, int], tuple[str, str]]:
    lines = a.script.read_text().splitlines()
    words = script_words(lines)
    return lines, words, bass_steps(words, ob.image_words(a.image))


def cmd_chain(a: argparse.Namespace) -> int:  # pragma: no cover - runs the oracle
    _lines, _words, steps = load(a)
    extra = [ln for w in (a.zero or []) for ln in zero_window_lines(w)]
    w = run_chain(a.exe, a.image, [a.script], extra, steps, (SKIP + FFT) // 2)
    for hz, amp, db in top_lines(output_frames(w)[SKIP:]):
        print(f"{hz:9.1f} Hz  amp {amp:.5f}  {db:6.1f} dB")
    return 0


def cmd_measure(a: argparse.Namespace) -> int:  # pragma: no cover - runs the oracle
    _lines, _words, steps = load(a)
    rows_out = []
    spec = {1: (W1_BASE, W1_WORDS, 0), 2: (W2_BASE, W2_WORDS, 12)}
    for window in a.window:
        base, words_n, ring = spec[window]
        rows = []
        for k in range(a.first_k, words_n - 1, a.step_k):
            extra = zero_window_lines(1) + zero_window_lines(2) + [f"0 {G_WORD:#x} 0x0"]
            extra.append(f"0 {base + k:#x} {fp.f32_bits(a.weight):#x}")
            slot = ring + k
            w = run_chain(a.exe, a.image, [a.script], extra, steps, (SKIP + FFT) // 2, slot)
            out = output_frames(w)[SKIP:]
            rin = np.stack([w[:, 3], w[:, 4]], 1).reshape(-1)[SKIP:]
            rows += [(h, g - 20 * float(np.log10(a.weight))) for h, g in fp.line_gains(rin, out)]
        table = fp.bin_table(rows)
        rows_out += [(window, hz, db, sp) for hz, db, sp in table]
        print(f"window {window}: {len(rows)} lines, {len(table)} bins", file=sys.stderr)
    with open(a.out, "w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(["window", "hz", "gain_db", "spread_db"])
        w.writerows((s, f"{hz:.2f}", f"{db:.2f}", f"{sp:.2f}") for s, hz, db, sp in rows_out)
    return 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("chain")
    c.add_argument("image")
    c.add_argument("script", type=Path)
    c.add_argument("--exe", default="sharc-chain")
    c.add_argument("--zero", type=int, action="append", choices=(1, 2))
    m = sub.add_parser("measure")
    m.add_argument("image")
    m.add_argument("script", type=Path)
    m.add_argument("out", type=Path)
    m.add_argument("--exe", default="sharc-chain")
    m.add_argument("--weight", type=float, default=1.0)
    m.add_argument("--first-k", dest="first_k", type=int, default=2)
    m.add_argument("--step-k", dest="step_k", type=int, default=4)
    m.add_argument("--window", type=int, action="append", choices=(1, 2), default=None)
    ns = ap.parse_args(argv)
    if ns.cmd == "measure" and ns.window is None:
        ns.window = [2]
    return cmd_chain(ns) if ns.cmd == "chain" else cmd_measure(ns)


if __name__ == "__main__":
    sys.exit(main())
