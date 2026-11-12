#!/usr/bin/env python3
"""Reference model of the PNO Core1 wavetable oscillator bank (pc 0x122620) and its ARM-side tables.

Spec: docs/research/organ-tonewheel-bank.md. The model reproduces the SHARC oracle word for word
(`sharc-bank`, crates/nord-sharc/src/bin/sharc_bank.rs):

    for each frame, for each of the 184 elements j (92 SIMD steps x lanes PEx/PEy, interleaved):
        phase[j] += fracmul(inc[j], tune)        # uint32 wrap; tune = DM(0xC27C5), 0x40000000 = 0.5
        idx       = phase[j] >> 20               # fext 12 bits at position 20
        x[j], y[j] = table[2*idx], table[2*idx+1]

    python tools/organ_bank_ref.py defaults  target/fw_unpacked/bank0_CODE.bin
    python tools/organ_bank_ref.py compare   SRAM/core1_sram.bin --exe target/release/sharc-bank
    python tools/organ_bank_ref.py spectrum  SRAM/core1_sram.bin --exe target/release/sharc-bank

`SRAM/core1_sram.bin` comes from `tools/ns4_dsp_ldr.py DIR/dsp_image_1_PNO.bin --sram SRAM`.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np

ELEMENTS = 184  # 92 SIMD steps x 2 lanes
WHEELS = 91  # elements 182, 183 are not read by the mix stage (oracle dependency test)
FRAMES_PER_CALL = 2
TABLE_ADDR = 0xC042B  # DM word address, 4096 (x, y) pairs
INC_ADDR = 0x955B6  # DM word address of inc[0]
TUNE_ADDR = 0xC27C5  # DM word address of the global rate factor
PHASE_ADDR = 0xC242D  # DM word address of phase[0] (PM)
OUT_X, OUT_Y = 0xC24E5, 0xC2655
ARM_BASE = 0xC0000000
ARM_INC_TABLE = 0xC0443EA8  # 184 u32 defaults written as DM(INC_ADDR + j) = v << 8
ARM_TUNE_TABLE = 0xC027D9D8  # u32 ratios, index (cents * 64 / 100 + base) + 0x1F3, value << 8
ARM_TUNE_BASE_INDEX = 0x1F3
DEFAULT_TUNE = 0x40000000

# Drawbar footage -> semitone offset of the wheel above the played note (Hammond layout).
# ASSUMED: textbook B3 harmonics (16', 5 1/3', 8', 4', 2 2/3', 2', 1 3/5', 1 1/3', 1'); the
# firmware weight writer is not decoded. 1 3/5' is 27.86 semitones, rounded to 28.
DRAWBAR_OFFSETS = (-12, 7, 0, 12, 19, 24, 28, 31, 36)


def read_words(blob: bytes, addr: int, count: int) -> np.ndarray:
    """`count` little-endian u32 words starting at DM word address `addr` of a flat SRAM image."""
    return np.frombuffer(blob, dtype="<u4", count=count, offset=4 * addr).copy()


def arm_words(arm: bytes, addr: int, count: int) -> np.ndarray:
    """`count` u32 words of the ARM image (loads at 0xC0000000)."""
    return np.frombuffer(arm, dtype="<u4", count=count, offset=addr - ARM_BASE).copy()


def f32(words: np.ndarray) -> np.ndarray:
    """Reinterpret u32 words as float32."""
    return np.asarray(words, dtype="<u4").view("<f4")


def fracmul(a: np.ndarray, b: int) -> np.ndarray:
    """Signed fractional multiply (SSFR): (a * b) / 2^31, rounded to nearest, as int32."""
    prod = np.asarray(a, dtype=np.uint32).view(np.int32).astype(object) * int(
        np.int32(np.uint32(b))
    )
    out = [(int(p) * 2 + (1 << 31)) >> 32 for p in prod]
    return np.array(out, dtype=np.int64).astype(np.uint32)


class ToneBank:
    """184 phase accumulators sharing one wavetable of 4096 (x, y) float pairs."""

    def __init__(self, table_words: np.ndarray, tune: int = DEFAULT_TUNE):
        self.table = np.asarray(table_words, dtype="<u4").reshape(4096, 2)
        self.tune = tune
        self.phase = np.zeros(ELEMENTS, dtype=np.uint32)

    def frame(self, inc_words: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
        """One frame: advance every phase by fracmul(inc, tune); return (x, y) words (u32)."""
        step = fracmul(inc_words, self.tune)
        self.phase = (self.phase.astype(np.uint64) + step.astype(np.uint64)).astype(np.uint32)
        idx = self.phase >> np.uint32(20)
        return self.table[idx, 0].copy(), self.table[idx, 1].copy()

    def call(self, inc_words: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
        """One routine call = two frames; returns (x, y) as 368-word arrays like the oracle."""
        frames = [self.frame(inc_words) for _ in range(FRAMES_PER_CALL)]
        return (np.concatenate([f[0] for f in frames]), np.concatenate([f[1] for f in frames]))


def default_increments(arm: bytes) -> np.ndarray:
    """The 184 values v the ARM writes as DM(0x955B6 + j) = v << 8 (loop at 0xC022DC5C)."""
    return arm_words(arm, ARM_INC_TABLE, ELEMENTS)


def tune_ratio(arm: bytes, index: int) -> int:
    """DM(0xC27C5) value for tune-table index `index`: table[index] << 8 (function 0xC022A8AC)."""
    return int(arm_words(arm, ARM_TUNE_TABLE + 4 * index, 1)[0]) << 8


def semitone_fit(values: np.ndarray) -> dict:
    """Fit v[2k], v[2k+1] to a 12-TET ladder v0 * 2^(k/12); report spreads in cents."""
    v = np.asarray(values, dtype=float)
    a, b = v[0::2], v[1::2]
    k = np.arange(len(a))
    mean_semis = 12 * np.log2((a + b) / 2)
    slope, offset = np.polyfit(k, mean_semis, 1)
    resid = (mean_semis - (slope * k + offset)) * 100
    pair = 1200 * np.log2(b / a)
    return {
        "slope_per_note": float(slope),
        "v0": float(2 ** (offset / 12)),
        "ladder_rms_cents": float(resid.std()),
        "pair_rms_cents": float(pair.std()),
        "pair_mean_cents": float(pair.mean()),
    }


def cycles_per_frame(inc_value: int, tune: int = DEFAULT_TUNE) -> float:
    """Phase advance per frame in cycles for ARM value v (inc word = v << 8)."""
    return (inc_value << 8) * tune / 2**31 / 2**32


def drawbar_weights(wheel: int, levels: tuple[int, ...], count: int = ELEMENTS // 2) -> np.ndarray:
    """Per-wheel weights of one note on wheel index `wheel`: drawbar level 0..8 at 3 dB per step.

    ASSUMED mapping (see DRAWBAR_OFFSETS); wheels outside 0..count-1 are dropped.
    """
    w = np.zeros(count)
    for off, lvl in zip(DRAWBAR_OFFSETS, levels, strict=True):
        if lvl > 0 and 0 <= wheel + off < count:
            w[wheel + off] += 10 ** (-3.0 * (8 - lvl) / 20)
    return w


def lane_sums(x_frames: np.ndarray, weights: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """Mix per lane: even elements (PEx) and odd elements (PEy) of each frame, weights per wheel.

    `x_frames` is (frames, 184) float32 values; `weights` has 92 entries applied to both copies.
    """
    even = x_frames[:, 0::2] @ weights
    odd = x_frames[:, 1::2] @ weights
    return even, odd


def tone_amplitude(signal: np.ndarray, cycles: float) -> float:
    """Amplitude of the sinusoid at `cycles` cycles/sample (Hann window, +-0.6 bin peak search)."""
    n = len(signal)
    win = np.hanning(n)
    s = (signal - signal.mean()) * win
    t = np.arange(n)
    best = 0.0
    for f in np.linspace(cycles - 0.6 / n, cycles + 0.6 / n, 25):
        best = max(best, abs(np.sum(s * np.exp(-2j * np.pi * f * t))))
    return 2 * best / win.sum()


def tone_level_db(signal: np.ndarray, cycles: float, harmonics: int) -> np.ndarray:
    """Level of harmonics 1..n of a tone at `cycles` cycles/sample, in dB below the fundamental."""
    amps = np.array([tone_amplitude(signal, h * cycles) for h in range(1, harmonics + 1)])
    return 20 * np.log10(np.maximum(amps, 1e-30) / amps[0])


def oracle_words(
    exe: str,
    image: str,
    incs: np.ndarray,
    calls: int,
    tune: int | None = None,
    updates: dict[int, dict[int, int]] | None = None,
) -> np.ndarray:
    """Run `sharc-bank`; return (calls, 2, 368) u32 words. `updates[call][addr] = value`."""
    lines = [f"0 0x{INC_ADDR + j:x} 0x{int(v):x}" for j, v in enumerate(incs)]
    if tune is not None:
        lines.append(f"0 0x{TUNE_ADDR:x} 0x{tune:x}")
    for call, pokes in (updates or {}).items():
        lines += [f"{call} 0x{a:x} 0x{v:x}" for a, v in pokes.items()]
    with tempfile.TemporaryDirectory() as tmp:
        script, out = Path(tmp) / "s.txt", Path(tmp) / "o.bin"
        script.write_text("\n".join(lines))
        subprocess.run(
            [
                str(Path(exe).resolve()),
                image,
                "--calls",
                str(calls),
                "--script",
                str(script),
                "--out",
                str(out),
            ],
            check=True,
            capture_output=True,
        )
        return np.fromfile(out, dtype="<u4").reshape(calls, 2, 368)


def cmd_defaults(args: argparse.Namespace) -> int:
    arm = Path(args.arm).read_bytes()
    v = default_increments(arm)
    fit = semitone_fit(v)
    print("first 8 values:", [hex(int(x)) for x in v[:8]])
    print({k: round(x, 4) for k, x in fit.items()})
    for idx in (0x1F3, 0x200):
        print(f"tune index {idx:#x}: DM(0xC27C5) = {tune_ratio(arm, idx):#x}")
    return 0


def cmd_compare(args: argparse.Namespace) -> int:
    image = args.sram
    blob = Path(image).read_bytes()
    arm = Path(args.arm).read_bytes() if args.arm else None
    rng = np.random.default_rng(args.seed)
    incs = (default_increments(arm) << 8) if arm else rng.integers(0, 2**23, ELEMENTS) << 8
    incs = incs.astype(np.uint32)
    odd = rng.integers(0, 2**32, ELEMENTS, dtype=np.uint64).astype(np.uint32)  # any sign, odd
    updates = {1: {INC_ADDR + j: int(odd[j]) for j in range(ELEMENTS)}}
    tune = 0x3FA02A00
    words = oracle_words(args.exe, image, incs, args.calls, tune=tune, updates=updates)
    bank = ToneBank(read_words(blob, TABLE_ADDR, 8192), tune=tune)
    bad = 0
    for c in range(args.calls):
        inc = odd if c >= 1 else incs
        x, y = bank.call(inc)
        bad += int((x != words[c, 0]).sum() + (y != words[c, 1]).sum())
    print(f"{args.calls} calls, {args.calls * 736} words compared, mismatches: {bad}")
    return 0 if bad == 0 else 1


def cmd_spectrum(args: argparse.Namespace) -> int:
    arm = Path(args.arm).read_bytes()
    incs = (default_increments(arm) << 8).astype(np.uint32)
    calls = args.frames // FRAMES_PER_CALL
    words = oracle_words(args.exe, args.sram, incs, calls)
    xs = f32(words[:, 0, :]).reshape(calls * 2, ELEMENTS)
    for j in (0, 40, 90):
        c = cycles_per_frame(int(default_increments(arm)[j]))
        print(
            f"element {j}: {c * 48000:.2f} Hz @48 kHz, harmonics dB:",
            np.round(tone_level_db(xs[:, j], c, 5), 2),
        )
    return 0


def cmd_chord(args: argparse.Namespace) -> int:
    """Weighted sum of oracle x outputs for a C-E-G triad with 8-8-8 drawbars."""
    arm = Path(args.arm).read_bytes()
    v = default_increments(arm)
    incs = (v << 8).astype(np.uint32)
    calls = args.frames // FRAMES_PER_CALL
    xs = f32(oracle_words(args.exe, args.sram, incs, calls)[:, 0, :]).reshape(-1, ELEMENTS)
    levels = (8, 8, 8, 0, 0, 0, 0, 0, 0)
    weights = sum(drawbar_weights(w, levels) for w in (args.root, args.root + 4, args.root + 7))
    even, odd = lane_sums(xs, weights)
    for wheel in (args.root, args.root + 4, args.root + 7, args.root - 12 + 4):
        c = cycles_per_frame(int(v[2 * wheel]))
        c2 = cycles_per_frame(int(v[2 * wheel + 1]))
        print(
            f"wheel {wheel}: weight {weights[wheel]:.3f}, lane0 amp "
            f"{tone_amplitude(even, c):.3f}, lane1 amp {tone_amplitude(odd, c2):.3f}"
        )
    return 0


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    d = sub.add_parser("defaults")
    d.add_argument("arm")
    d.set_defaults(fn=cmd_defaults)
    c = sub.add_parser("compare")
    c.add_argument("sram")
    c.add_argument("--exe", required=True)
    c.add_argument("--arm")
    c.add_argument("--calls", type=int, default=40)
    c.add_argument("--seed", type=int, default=1)
    c.set_defaults(fn=cmd_compare)
    s = sub.add_parser("spectrum")
    s.add_argument("sram")
    s.add_argument("--exe", required=True)
    s.add_argument("--arm", required=True)
    s.add_argument("--frames", type=int, default=16384)
    s.set_defaults(fn=cmd_spectrum)
    ch = sub.add_parser("chord")
    ch.add_argument("sram")
    ch.add_argument("--exe", required=True)
    ch.add_argument("--arm", required=True)
    ch.add_argument("--frames", type=int, default=16384)
    ch.add_argument("--root", type=int, default=34)
    ch.set_defaults(fn=cmd_chord)
    args = p.parse_args(argv)
    return args.fn(args)


if __name__ == "__main__":
    sys.exit(main())
