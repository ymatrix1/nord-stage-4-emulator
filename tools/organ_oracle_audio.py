#!/usr/bin/env python3
"""Organ audio from the real PNO Core1 DSP code (bank 0x122620 + mix stage 0x12268F) in the oracle.

Method and results: docs/research/organ-oracle-audio.md. The harness is `sharc-organ`
(crates/nord-sharc/src/bin/sharc_organ.rs); this tool builds the data pokes, runs it, writes WAV
and analyses the partials.

    python tools/organ_oracle_audio.py render SRAM/core1_sram.bin --exe target/release/sharc-organ \
        --notes 60,64,67 --drawbars 888000000 --frames 16384 --out chord
    python tools/organ_oracle_audio.py report chord.f32.npy --notes 60,64,67 --drawbars 888000000

Layout used (all decoded by poking the real mix stage, see the note, section 3):

    weight of wheel i (0..91) = a[i] * wB[i] + b[i] * wA[i]       one value feeds elements 2i, 2i+1
    a  = DM(0x9594F + 0x65 s + i)      b  = DM(0x95AE2 + 0x64 s + i)
    wB = DM(0x957DE + 0x5C s + i)      wA = DM(0x9566E + 0x5C s + i)
    tx = sum_i weight[i] * x[2i],  ty = sum_i weight[i] * x[2i + 1]          (copy 0, copy 1)
    out_x = L * (g5 tx + g8 ty),  out_y = L * (g6 ty + g7 tx)      one-pole smoothed, 0.01 per frame
    g5 = DM(0x959AF)  g6 = DM(0x959B0)  g8 = DM(0x95B3E)  g7 = DM(0x95B3F)  L = DM(0x95B42)
"""

from __future__ import annotations

import argparse
import csv
import os
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

import ns4_oracle_compare as cmpmod
import numpy as np
import organ_bank_ref as ob

INC_ADDR = 0x955B6  # 184 phase increments, DM word = v << 8
WHEELS = 92  # weight index 0..91; element 2i, 2i+1 are the two copies of wheel i
SET_STRIDE_A, SET_STRIDE_B, SET_STRIDE_W = 0x65, 0x64, 0x5C
A_BASE, B_BASE, WB_BASE, WA_BASE = 0x9594F, 0x95AE2, 0x957DE, 0x9566E
GAIN_X, GAIN_Y, CROSS_YX, CROSS_XY, LEVEL = 0x959AF, 0x959B0, 0x95B3F, 0x95B3E, 0x95B42
ONE = 0x3F800000
# VERIFIED (arm-organ-messages.md s7): ARM group gains (setters 5..8) as 24-bit ints.
GAINS_EXACT = (0x7FFFFF, 0, 0x7FFFFF, 0)
GAINS_DETUNED = (0x600000, 0x600000, 0x300000, 0x300000)
# VERIFIED (arm-organ-messages.md s7): drawbar edit value 0..8 -> 7-bit value.
DRAWBAR_CURVE = (0, 3, 6, 13, 22, 39, 62, 87, 127)
# ASSUMED: textbook Hammond footage offsets in semitones above the played key.
DRAWBAR_OFFSETS = (-12, 7, 0, 12, 19, 24, 28, 31, 36)
WHEEL0_MIDI = 24  # ASSUMED (same as crates/nord-core/src/organ/vibrato_bank.rs)
INC_CSV = Path(__file__).resolve().parent.parent / "docs/research/data/arm-bank-increments.csv"


def f2u(x: float) -> int:
    """Bit pattern of a float32."""
    return struct.unpack("<I", struct.pack("<f", x))[0]


def increments(detuned: bool) -> list[int]:
    """The 184 increment words the ARM stores in DM (`v << 8`, as in the CSV) of one table."""
    col = "detuned_word" if detuned else "exact_12tet_word"
    with INC_CSV.open(encoding="utf-8") as fh:
        return [int(r[col], 16) for r in csv.DictReader(fh)]


def foldback(midi: int) -> int:
    """Wheel index of a target MIDI note with octave foldback at both ends (ASSUMED)."""
    while midi < WHEEL0_MIDI:
        midi += 12
    while midi > WHEEL0_MIDI + WHEELS - 2:  # same 91-wheel range as the Rust engine
        midi -= 12
    return midi - WHEEL0_MIDI


def drawbar_digits(text: str) -> list[int]:
    """`888000000` -> [8, 8, 8, 0, 0, 0, 0, 0, 0]."""
    d = [int(c) for c in text]
    if len(d) != 9 or max(d) > 8:
        raise ValueError("drawbars are nine digits 0..8")
    return d


def chord_weights(notes: list[int], drawbars: list[int], law: str = "firmware") -> np.ndarray:
    """Per-wheel weight of held `notes` (ASSUMED footage offsets; level law firmware or linear)."""
    w = np.zeros(WHEELS)
    for n in notes:
        for lvl, off in zip(drawbars, DRAWBAR_OFFSETS, strict=True):
            if lvl:
                w[foldback(n + off)] += DRAWBAR_CURVE[lvl] / 127 if law == "firmware" else lvl / 8
    return w


def wheel_cycles(index: int, incs: list[int], copy: int = 0, tune: float = 0.5) -> float:
    """Cycles per frame of a wheel copy: `fracmul(inc, tune) / 2^32 = inc * tune / 2^32`."""
    return incs[2 * index + copy] * tune / 2**32


def build_pokes(
    weights: np.ndarray,
    detuned: bool,
    level: float = 1.0,
    gains: tuple[int, int, int, int] | None = None,
    s: int = 0,
) -> list[tuple[int, int, int]]:
    """Poke list `(call, addr, value)` for set `s`: increments, weights, group gains, level."""
    g = gains or (GAINS_DETUNED if detuned else GAINS_EXACT)
    pokes = [(0, INC_ADDR + j, v) for j, v in enumerate(increments(detuned))]
    for i, wt in enumerate(weights):
        pokes.append((0, A_BASE + SET_STRIDE_A * s + i, f2u(float(wt))))
        pokes.append((0, WB_BASE + SET_STRIDE_W * s + i, ONE))
    off = SET_STRIDE_A * s
    for addr, val in ((GAIN_X, g[0]), (GAIN_Y, g[1]), (CROSS_YX, g[2]), (CROSS_XY, g[3])):
        pokes.append((0, addr + off, f2u(val / 0x800000)))
    pokes.append((0, LEVEL + SET_STRIDE_B * s, f2u(level)))
    return pokes


def run_oracle(exe: str, image: str, pokes: list[tuple[int, int, int]], calls: int) -> np.ndarray:
    """Run `sharc-organ` for `calls` 2-frame calls; return (2 * calls, 2) float32 (out_x, out_y)."""
    with tempfile.TemporaryDirectory() as tmp:
        script, out = Path(tmp) / "s.txt", Path(tmp) / "o.bin"
        script.write_text("\n".join(f"{c} {a:#x} {v:#x}" for c, a, v in pokes), encoding="utf-8")
        cmd = [os.path.abspath(exe), image, "--calls", str(calls), "--script", str(script)]
        subprocess.run([*cmd, "--out", str(out)], check=True, capture_output=True)
        words = np.fromfile(out, dtype="<u4")
    return words.view("<f4").reshape(-1, 2).copy()


def write_wav_f32(path: Path, data: np.ndarray, rate: int) -> None:
    """Stereo/mono 32-bit float WAV (format 3)."""
    d = np.atleast_2d(np.asarray(data, dtype="<f4").T).T
    ch = d.shape[1]
    payload = d.tobytes()
    hdr = b"RIFF" + struct.pack("<I", 36 + len(payload)) + b"WAVEfmt "
    hdr += struct.pack("<IHHIIHH", 16, 3, ch, rate, rate * 4 * ch, 4 * ch, 32)
    path.write_bytes(hdr + b"data" + struct.pack("<I", len(payload)) + payload)


def amplitude(signal: np.ndarray, cycles: float) -> float:
    """Amplitude of the sinusoid at `cycles` per sample (Hann window, +-0.6 bin peak search)."""
    n = len(signal)
    win = np.hanning(n)
    s = (signal - signal.mean()) * win
    t = np.arange(n)
    best = 0.0
    for f in np.linspace(cycles - 0.6 / n, cycles + 0.6 / n, 25):
        best = max(best, abs(np.sum(s * np.exp(-2j * np.pi * f * t))))
    return 2 * best / win.sum()


def partial_table(signal: np.ndarray, weights: np.ndarray, incs: list[int]) -> list[tuple]:
    """(wheel, weight, cycles/frame, measured amplitude) for every wheel with a weight."""
    rows = []
    for i in np.nonzero(weights)[0]:
        c = wheel_cycles(int(i), incs)
        rows.append((int(i), float(weights[i]), c, amplitude(signal, c)))
    return rows


# VERIFIED (poked mix stage, one-pole smoothers in the scalar areas): per frame the gain states move
# s = 0.01 * target + 0.99 * s (words 0x959AB..AE hold 0.01 / 0.99), the level state likewise.
SMOOTH_OLD = 0.99  # the new-sample coefficient is 1 - SMOOTH_OLD = 0.01


def smoothed(target: float, frames: int) -> np.ndarray:
    """State of a one-pole smoother after each frame (post-update, initial state 0)."""
    k = np.arange(1, frames + 1)
    return target * (1.0 - SMOOTH_OLD**k)


def model_render(
    table_words: np.ndarray,
    incs: list[int],
    weights: np.ndarray,
    gains: tuple[int, int, int, int],
    level: float,
    frames: int,
) -> np.ndarray:
    """Python model of bank + mix stage (float64 sums): (frames, 2) output of lanes x, y."""
    bank = ob.ToneBank(table_words)
    inc = np.array(incs, dtype=np.uint32)
    xs = np.concatenate(
        [ob.f32(bank.call(inc)[0]).reshape(2, ob.ELEMENTS) for _ in range(frames // 2)]
    )
    w = np.zeros(WHEELS)
    w[: len(weights)] = weights
    tx = xs[:, 0::2].astype(np.float64) @ w
    ty = xs[:, 1::2].astype(np.float64) @ w
    g5, g6, g7, g8 = (smoothed(g / 0x800000, frames) for g in gains)
    lv = smoothed(level, frames)
    return np.stack([lv * (g5 * tx + g8 * ty), lv * (g6 * ty + g7 * tx)], axis=1)


# Golden vectors for crates/nord-core/src/organ/bank_mix_golden.rs: synthetic closed-form table,
# six wheels, copy increments, weights; output lanes at steady-state gains (no smoothing).
GOLDEN_COPY_INCS = (
    (0x02000000, 0x02001000), (0x02468ACF, 0x02440000), (0x00633E00, 0x00635300),
    (0x0633E000, 0x06340000), (0x00001235, 0x00001300), (0x01234568, 0x01230000),
)  # fmt: skip
GOLDEN_WEIGHTS = (1.0, 0.5, 0.25, 0.8, 0.3, 0.1)
GOLDEN_FRAMES = 16


def golden_mix(detuned: bool) -> list[tuple[float, float]]:
    """(out_x, out_y) per frame with steady-state gains and level 1, float64 sums."""
    bank = ob.ToneBank(ob.x_table_words())
    inc = np.zeros(ob.ELEMENTS, dtype=np.uint32)
    for w, (c0, c1) in enumerate(GOLDEN_COPY_INCS):
        inc[2 * w], inc[2 * w + 1] = c0, c1
    g5, g6, g7, g8 = (g / 0x800000 for g in (GAINS_DETUNED if detuned else GAINS_EXACT))
    wt = np.zeros(WHEELS)
    wt[: len(GOLDEN_WEIGHTS)] = GOLDEN_WEIGHTS
    out = []
    for _ in range(GOLDEN_FRAMES // 2):
        x = ob.f32(bank.call(inc)[0]).reshape(2, ob.ELEMENTS).astype(np.float64)
        for f in x:
            tx, ty = f[0::2] @ wt, f[1::2] @ wt
            out.append((g5 * tx + g8 * ty, g6 * ty + g7 * tx))
    return out


def cmd_golden(_: argparse.Namespace) -> int:
    """Print the Rust constants of bank_mix_golden.rs."""
    print(f"pub(super) const COPY_INCS: [[u32; 2]; {len(GOLDEN_COPY_INCS)}] = [")
    for c0, c1 in GOLDEN_COPY_INCS:
        print(f"    [0x{c0:08X}, 0x{c1:08X}],")
    print("];")
    print(f"pub(super) const WEIGHTS: [f32; {len(GOLDEN_WEIGHTS)}] = {list(GOLDEN_WEIGHTS)};")
    for name, det in (("EXACT", False), ("DETUNED", True)):
        print(f"pub(super) const OUT_{name}: [[f32; 2]; {GOLDEN_FRAMES}] = [")
        for x, y in golden_mix(det):
            print(f"    [{np.float32(x)!s}, {np.float32(y)!s}],")
        print("];")
    return 0


def cmd_table(a: argparse.Namespace) -> int:
    """Fit the firmware x table (DM 0xC042B) to sin t + sum c_h cos(h t) and print the levels."""
    t = ob.f32(ob.read_words(Path(a.image).read_bytes(), ob.TABLE_ADDR, 8192)).reshape(4096, 2)
    x = t[:, 0].astype(np.float64)
    tt = 2 * np.pi * np.arange(4096) / 4096
    cols = [np.sin(tt)] + [f(h * tt) for h in range(2, 9) for f in (np.cos, np.sin)]
    c, *_ = np.linalg.lstsq(np.stack(cols, axis=1), x, rcond=None)
    print("sin t coefficient", round(c[0], 7))
    for i, h in enumerate(range(2, 9)):
        cc, ss = c[1 + 2 * i], c[2 + 2 * i]
        db = 20 * np.log10(max(np.hypot(cc, ss), 1e-12))
        print(f"h{h}: cos {cc:+.7f} sin {ss:+.2e}  {db:7.2f} dB")
    print("max fit error", float(np.abs(np.stack(cols, axis=1) @ c - x).max()))
    return 0


def cmd_render(a: argparse.Namespace) -> int:
    notes = [int(n) for n in a.notes.split(",")]
    weights = chord_weights(notes, drawbar_digits(a.drawbars), a.law)
    pokes = build_pokes(weights, a.detuned, a.level)
    out = run_oracle(a.exe, a.image, pokes, a.frames // 2)
    stem = Path(a.out)
    np.save(f"{stem}.f32.npy", out)
    write_wav_f32(Path(f"{stem}.wav"), out, a.rate)
    print(
        f"{len(out)} frames, peak {np.abs(out).max(axis=0)}, rms {np.sqrt((out**2).mean(axis=0))}"
    )
    return 0


def cmd_verify(a: argparse.Namespace) -> int:
    """Oracle against the Python model for the exact and the detuned bank."""
    notes = [int(n) for n in a.notes.split(",")]
    weights = chord_weights(notes, drawbar_digits(a.drawbars))
    table = ob.read_words(Path(a.image).read_bytes(), ob.TABLE_ADDR, 8192)
    status = 0
    for detuned in (False, True):
        gains = GAINS_DETUNED if detuned else GAINS_EXACT
        orc = run_oracle(a.exe, a.image, build_pokes(weights, detuned), a.frames // 2)
        mdl = model_render(table, increments(detuned), weights, gains, 1.0, a.frames)
        err = np.abs(orc - mdl)
        rel = np.sqrt((err**2).mean() / (mdl**2).mean())
        print(
            f"detuned={detuned}: peak {np.abs(orc).max():.4f}, "
            f"max abs err {err.max():.2e}, rel rms {rel:.2e}"
        )
        status |= int(rel > 1e-3)
    return status


def best_scale(oracle: np.ndarray, ref: np.ndarray) -> tuple[float, float]:
    """Least-squares gain `g` of `ref` onto `oracle` and the SNR in dB after applying it."""
    g = float(np.dot(oracle, ref) / np.dot(ref, ref))
    err = oracle - g * ref
    return g, float(10 * np.log10(np.dot(oracle, oracle) / max(np.dot(err, err), 1e-300)))


def cmd_compare(a: argparse.Namespace) -> int:
    """Oracle .npy against a Rust render (.wav from the `organ_ref` example)."""
    orc = np.load(a.npy)[a.skip :]
    _, cols = cmpmod.read_wav(a.wav)
    ref = np.stack([np.array(cols["ch0"]), np.array(cols["ch1"])], axis=1)[a.skip :]
    notes = [int(n) for n in a.notes.split(",")]
    weights = chord_weights(notes, drawbar_digits(a.drawbars), a.law)
    incs = increments(a.detuned)
    if a.lanes:
        for k, name in enumerate(("x/L", "y/R")):
            g, snr = best_scale(orc[:, k].astype(np.float64), ref[:, k].astype(np.float64))
            print(f"lane {name}: rust/oracle level {1 / g:.4f}x, SNR after best gain {snr:.1f} dB")
        return 0
    mid = orc.astype(np.float64).mean(axis=1)  # mono comparison: oracle lane mean vs Rust (L+R)/2
    rmean = ref.astype(np.float64).mean(axis=1)
    g, snr = best_scale(mid, rmean)
    print(f"mono: rust/oracle level {1 / g:.4f}x, SNR after best gain {snr:.1f} dB")
    print("wheel  weight  oracle  rust  rust/oracle(dB, minus overall)")
    rows = partial_table(mid, weights, incs)
    rust = [amplitude(rmean, c) for _, _, c, _ in rows]
    g0 = rust[0] / rows[0][3] if rows[0][3] else 1.0
    for (i, wt, _, amp), r in zip(rows, rust, strict=True):
        print(
            f"{i:5d}  {wt:6.3f}  {amp:6.4f}  {r:.5f}  "
            f"{20 * np.log10(max(r, 1e-12) / amp / g0):+6.2f}"
        )
    return 0


def cmd_report(a: argparse.Namespace) -> int:
    out = np.load(a.npy)[a.skip :]
    notes = [int(n) for n in a.notes.split(",")]
    weights = chord_weights(notes, drawbar_digits(a.drawbars), a.law)
    incs = increments(a.detuned)
    print("wheel  weight  cycles/frame  measured(L)  measured(R)")
    for i, wt, c, amp in partial_table(out[:, 0], weights, incs):
        print(f"{i:5d}  {wt:6.3f}  {c:.6f}  {amp:10.4f}  {amplitude(out[:, 1], c):10.4f}")
    return 0


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("render")
    r.add_argument("image")
    r.add_argument("--exe", required=True)
    r.add_argument("--notes", default="60,64,67")
    r.add_argument("--drawbars", default="888000000")
    r.add_argument("--law", choices=("firmware", "linear"), default="firmware")
    r.add_argument("--detuned", action="store_true")
    r.add_argument("--level", type=float, default=1.0)
    r.add_argument("--frames", type=int, default=16384)
    r.add_argument("--rate", type=int, default=48000)
    r.add_argument("--out", required=True)
    r.set_defaults(fn=cmd_render)
    v = sub.add_parser("verify")
    v.add_argument("image")
    v.add_argument("--exe", required=True)
    v.add_argument("--notes", default="60,64,67")
    v.add_argument("--drawbars", default="888000000")
    v.add_argument("--frames", type=int, default=8192)
    v.set_defaults(fn=cmd_verify)
    c = sub.add_parser("compare")
    c.add_argument("npy")
    c.add_argument("wav")
    c.add_argument("--notes", default="60,64,67")
    c.add_argument("--drawbars", default="888000000")
    c.add_argument("--law", choices=("firmware", "linear"), default="firmware")
    c.add_argument("--detuned", action="store_true")
    c.add_argument("--skip", type=int, default=2048)
    c.add_argument("--lanes", action="store_true", help="compare x/y against Rust L/R per lane")
    c.set_defaults(fn=cmd_compare)
    sub.add_parser("golden").set_defaults(fn=cmd_golden)
    tb = sub.add_parser("table")
    tb.add_argument("image")
    tb.set_defaults(fn=cmd_table)
    q = sub.add_parser("report")
    q.add_argument("npy")
    q.add_argument("--notes", default="60,64,67")
    q.add_argument("--drawbars", default="888000000")
    q.add_argument("--law", choices=("firmware", "linear"), default="firmware")
    q.add_argument("--detuned", action="store_true")
    q.add_argument("--skip", type=int, default=2048, help="settling frames to drop")
    q.set_defaults(fn=cmd_report)
    args = p.parse_args(argv)
    return args.fn(args)


if __name__ == "__main__":
    sys.exit(main())
