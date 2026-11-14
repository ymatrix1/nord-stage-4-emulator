#!/usr/bin/env python3
"""Check the Farfisa generator model against the SHARC oracle (`sharc-chain`) and report errors.

Harness: `crates/nord-sharc/src/bin/sharc_chain.rs`; model: `organ_farfisa_ref.py`; results:
`docs/research/organ-farfisa-bass.md`.

    python tools/organ_farfisa_oracle.py verify SRAM/core1_sram.bin \
        --exe target/release/sharc-chain
    python tools/organ_farfisa_oracle.py tables SRAM/core1_sram.bin
    python tools/organ_farfisa_oracle.py compare SRAM/core1_sram.bin \
        --exe target/release/sharc-chain --frames 4000 [--rust rust_rows.csv]
    python tools/organ_farfisa_oracle.py golden > crates/nord-core/src/organ/farfisa_golden.rs

`verify` runs the Python model against the oracle on random scenarios and on the image state for
several consecutive calls (every captured word must be equal). `tables` fits the closed-form row
tables and stream words to the image. `compare` reports, per (row, oscillator) slot, the error of
the closed-form model (and of the Rust example `farfisa_ref` CSV) against the oracle ring series.
"""

from __future__ import annotations

import argparse
import math
import struct
import sys
from pathlib import Path

import numpy as np
import organ_farfisa_ref as fr
import organ_post_oracle as po

STEP = "0x122A71:0x122B84:I0=0xc00ea,I1=0xb52c7,M0=0,M1=1,M2=2,M7=0xffffffff"
WINDOWS = ((0xC27D0, 0x20), (0xC2950, 0x380))
RING_WINDOW = ((fr.RING[0], 2 * 0x62),)
ROWS, OSC = 9, 12


def u2f(word: int) -> float:
    return struct.unpack("<f", struct.pack("<I", word))[0]


def image_word(image: bytes, addr: int) -> int:
    return struct.unpack_from("<I", image, 4 * addr)[0]


def run_scenario(exe: str, image: str, pokes: dict[int, int], calls: int) -> list[int]:
    """Differing addresses between the model and the oracle after each of `calls` calls."""
    ranges = WINDOWS
    pre = po.snapshot(po.run_chain(exe, image, pokes, [], ranges)[0], ranges)
    rows = po.run_chain(exe, image, pokes, [STEP], ranges, calls)
    pre.update(pokes)
    m = fr.Mem(pre)
    diffs: list[int] = []
    for k in range(calls):
        fr.generator(m)
        want = po.snapshot(rows[k], ranges)
        diffs += [a for base, n in ranges for a in range(base, base + n) if m[a] != want[a]]
    return diffs


def cmd_verify(a: argparse.Namespace) -> int:
    bad = 0
    for seed in range(a.seeds):
        d = run_scenario(a.exe, a.image, fr.synthetic_inputs(seed), 1)
        print(f"random seed {seed}: {len(d)} differing words {[hex(x) for x in d[:4]]}")
        bad += bool(d)
    d = run_scenario(a.exe, a.image, {}, a.calls)
    print(f"image state, {a.calls} consecutive calls: {len(d)} differing words")
    bad += bool(d)
    print("OK: bit-exact" if not bad else f"FAILED: {bad} scenarios differ")
    return 1 if bad else 0


def cmd_tables(a: argparse.Namespace) -> int:
    image = Path(a.image).read_bytes()
    w = lambda ad: image_word(image, ad)  # noqa: E731
    counts = [w(0xC2C1A + j) for j in range(fr.PAIRS)]
    print("row counts", counts, "closed form", list(fr.IMAGE_COUNTS))
    model = fr.closed_form_inputs()
    worst = {"offset": 0.0, "slope": 0.0, "a": 0.0, "b": 0.0, "inc": 0, "phase_offset": 0}
    t, s = 0xC2950, 0xC2AD8
    for j, n in enumerate(counts):
        for lane in (0, 1):
            worst["inc"] = max(worst["inc"], abs(w(s + lane) - model[s + lane]))
        s += 4
        for r in range(n):
            for lane in (0, 1):
                worst["phase_offset"] = max(
                    worst["phase_offset"], abs(w(s + lane) - model[s + lane])
                )
                got = [u2f(w(t + 2 * q + lane)) for q in range(4)]
                want = [u2f(model[t + 2 * q + lane]) for q in range(4)]
                for key, g, x in zip(("offset", "slope", "a", "b"), got, want, strict=True):
                    err = abs(g - x) / (abs(g) if key == "slope" else 1.0)
                    worst[key] = max(worst[key], err)
            s, t = s + 2, t + 8
    print("worst error image vs closed form:", {k: f"{v:.3g}" for k, v in worst.items()})
    return 0


def oracle_rows(exe: str, image: str, frames: int) -> np.ndarray:
    """Oracle ring series `[frame, row, oscillator]` (image state, free running)."""
    calls = frames // 2
    words = po.run_chain(exe, image, {}, [STEP], RING_WINDOW, calls)
    ring = words.reshape(calls, 2, 0x62)
    out = np.zeros((frames, ROWS, OSC), dtype=np.float32)
    for r in range(ROWS):
        for n in range(OSC):
            idx = 12 * r + n
            if idx < 0x62:
                out[:, r, n] = ring[:, :, idx].reshape(-1).view("<f4")
    return out


def model_rows(frames: int) -> np.ndarray:
    """Closed-form Python model ring series `[frame, row, oscillator]`."""
    m = fr.Mem(fr.closed_form_inputs())
    out = np.zeros((frames, ROWS, OSC), dtype=np.float32)
    for k in range(frames // 2):
        fr.generator(m)
        for f, base in enumerate(fr.RING):
            for r in range(ROWS):
                for n in range(OSC):
                    idx = 12 * r + n
                    if idx < 0x62:
                        out[2 * k + f, r, n] = u2f(m[base + idx])
    return out


def spectrum_error_db(ref: np.ndarray, test: np.ndarray, floor_db: float = -60.0) -> float:
    """Max |dB difference| of the magnitude spectra over bins within `floor_db` of the peak."""
    win = np.hanning(len(ref))
    a = np.abs(np.fft.rfft((ref - ref.mean()) * win))
    b = np.abs(np.fft.rfft((test - test.mean()) * win))
    keep = a > a.max() * 10 ** (floor_db / 20)
    return float(np.max(np.abs(20 * np.log10((b[keep] + 1e-12) / a[keep]))))


def slots() -> list[tuple[int, int]]:
    return [
        (r, n)
        for r in range(ROWS)
        for n in range(OSC)
        if 12 * r + n < 0x62 and r < (9 if n < 2 else 8)
    ]


def report(name: str, ref: np.ndarray, test: np.ndarray) -> None:
    peak, worst_snr, worst_db = 0.0, 1e9, 0.0
    for r, n in slots():
        x, y = ref[:, r, n].astype(np.float64), test[:, r, n].astype(np.float64)
        peak = max(peak, float(np.max(np.abs(x - y))))
        rms = float(np.sqrt(np.mean((x - y) ** 2)))
        worst_snr = min(worst_snr, 20 * math.log10(float(np.std(x)) / max(rms, 1e-12)))
        worst_db = max(worst_db, spectrum_error_db(x, y))
    print(
        f"{name}: {len(slots())} slots, max |diff| {peak:.3g}, worst SNR {worst_snr:.1f} dB, "
        f"worst spectral error {worst_db:.2f} dB (bins within 60 dB of the peak)"
    )


def read_rust_csv(path: str, frames: int) -> np.ndarray:
    """`frame,row,osc,value` rows of `examples/farfisa_ref.rs` to `[frame, row, oscillator]`."""
    data = np.loadtxt(path, delimiter=",")
    out = np.zeros((frames, ROWS, OSC), dtype=np.float32)
    for f, r, n, v in data:
        if f < frames:
            out[int(f), int(r), int(n)] = v
    return out


def cmd_compare(a: argparse.Namespace) -> int:
    ref = oracle_rows(a.exe, a.image, a.frames)
    if not a.skip_model:
        report("closed-form Python model vs oracle", ref, model_rows(a.frames))
    if a.rust:
        report("Rust FarfisaWheels vs oracle", ref, read_rust_csv(a.rust, a.frames))
    return 0


GOLDEN_FRAMES = (200, 1999, 4001)
GOLDEN_OSC = (0, 5, 11)


def golden_rows(frames: int = 4002) -> dict[int, np.ndarray]:
    series = model_rows(frames)
    return {f: series[f] for f in GOLDEN_FRAMES}


def cmd_golden(_: argparse.Namespace) -> int:
    rows = golden_rows()
    out = [
        "//! Golden vectors of the Farfisa divider (`farfisa_wheels.rs`).",
        "//!",
        "//! Regenerate with `python tools/organ_farfisa_oracle.py golden`: the bit-exact SHARC",
        "//! model of `tools/organ_farfisa_ref.py` (verified against the oracle) runs on the",
        "//! closed-form tables. Derived values only, no firmware data. Provenance:",
        "//! `docs/specs/organ-engine.md` section 3.1.8.",
        "#![allow(clippy::approx_constant, clippy::excessive_precision)]",
        "",
        f"pub(super) const FRAMES: [usize; {len(GOLDEN_FRAMES)}] = {list(GOLDEN_FRAMES)};",
        f"pub(super) const OSCILLATORS: [usize; {len(GOLDEN_OSC)}] = {list(GOLDEN_OSC)};",
        "#[rustfmt::skip]",
        f"pub(super) const ROWS: [[[f32; {len(GOLDEN_OSC)}]; 9]; {len(GOLDEN_FRAMES)}] = [",
    ]
    for f in GOLDEN_FRAMES:
        out.append("    [")
        for r in range(9):
            vals = ", ".join(repr(float(rows[f][r, n])) for n in GOLDEN_OSC)
            out.append(f"        [{vals}],")
        out.append("    ],")
    out.append("];")
    out += [
        "",
        "#[test]",
        "fn golden_vectors_are_bounded_pulses() {",
        "    assert!(ROWS.iter().flatten().flatten().all(|v| v.abs() <= 1.0 + 1e-6));",
        "    assert!(ROWS.iter().flatten().flatten().any(|v| *v == 1.0));",
        "}",
    ]
    print("\n".join(out))
    return 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    for name in ("verify", "tables", "compare"):
        p = sub.add_parser(name)
        p.add_argument("image")
        if name != "tables":
            p.add_argument("--exe", default="sharc-chain")
        if name == "verify":
            p.add_argument("--seeds", type=int, default=4)
            p.add_argument("--calls", type=int, default=40)
        if name == "compare":
            p.add_argument("--frames", type=int, default=4000)
            p.add_argument("--rust")
            p.add_argument("--skip-model", action="store_true", help="skip the slow Python model")
    sub.add_parser("golden")
    ns = ap.parse_args(argv)
    return {
        "verify": cmd_verify,
        "tables": cmd_tables,
        "compare": cmd_compare,
        "golden": cmd_golden,
    }[ns.cmd](ns)


if __name__ == "__main__":
    sys.exit(main())
