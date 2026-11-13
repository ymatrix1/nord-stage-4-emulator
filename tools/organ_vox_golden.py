#!/usr/bin/env python3
"""Golden vectors for `crates/nord-core/src/organ/vox_golden.rs` (Vox note bank and output stage).

The vectors are derived values, no firmware data: the bit-exact SHARC models of `organ_post_ref.py`
(`scanner`, `smooth`, `output_stage`) are run on closed-form tables (the regenerated waveshaper of
`organ_shaper.blep_table`, `rate_k = 0x62C2E7 2^(k/12)`, `coef_k = 128.8 2^(-k/12)`, the note
low-pass coefficients rebuilt from `CUTOFF_RATIO`, the image output sections), so the Rust modules
`vox_bank.rs`, `vox_output.rs` and `vox_shaper.rs` are checked against the verified arithmetic.

    python tools/organ_vox_golden.py rust > crates/nord-core/src/organ/vox_golden.rs
    python tools/organ_vox_golden.py cutoffs SRAM/core1_sram.bin --exe target/release/sharc-chain
"""

from __future__ import annotations

import argparse
import math
import subprocess
import sys

import numpy as np
import organ_post_ref as pr
import organ_shaper as sh

FRAMES = 1200  # frames run per scenario
STRIDE = 25  # one record every STRIDE frames
NOTES_PLAIN = (0, 12, 36, 60, 83)
NOTES_VIB = (24, 40, 71)
DEPTH_WORD = 0x240B700  # Vox 3, `0x240B7 << 8`
FW_RATE = 43_156.4  # frame rate of the Rust bank (ASSUMED, lane_mix.rs)
RATE_BASE = 0x62C2E7
# EMPIRICAL: cutoff of the note low-pass per group of 4 notes in units of the cycles per frame of
# the group's lowest note; same list as `CUTOFF_RATIO` in vox_bank.rs (checked by the tests).
CUTOFF_RATIO = (
    1.9119, 1.8968, 1.8066, 1.9119, 1.8968, 1.8066, 1.4339, 1.3879, 1.3284, 1.0543, 1.2107,
    1.3686, 1.0863, 1.2933, 1.5055, 1.1949, 1.4226, 1.6605, 1.3179, 1.5134, 1.7108,
)  # fmt: skip
TABLE_INDICES = (
    0,
    1,
    2,
    100,
    250,
    340,
    400,
    500,
    511,
    512,
    513,
    530,
    600,
    683,
    700,
    800,
    1000,
    1024,
)
# image output sections (a0, a1, b0, b1), organ-model-paths.md section 4
SECTIONS = (
    (0.9891, -0.9944, 0.9944, 0.0),
    (0.9514, 0.0305, 0.0161, 0.0),
    (0.8348, 0.1035, 0.0546, 0.0),
)
OUT_FRAMES = 64


def cycles_per_frame(k: int) -> float:
    return 0.5 * RATE_BASE * 2 ** (k / 12) / 2**32


def cutoff_coefficient(k: int) -> float:
    """Bilinear low-pass coefficient `c` of note `k` at the firmware frame rate."""
    g = k // 4
    fc = CUTOFF_RATIO[g] * cycles_per_frame(4 * g)
    tan = math.tan(math.pi * fc)
    return tan / (1 + tan)


def table_values() -> np.ndarray:
    return sh.blep_table(*sh.FITTED)


def memory(depth_word: int) -> pr.Mem:
    """Post-stage memory with closed-form tables (zero phases, zero LFO, zero buffers)."""
    m = pr.Mem()
    m[pr.OFFSET] = 0x40000000
    m[pr.DEPTH] = depth_word
    table = table_values()
    for i in range(-512, 513):
        m[pr.TABLE + i] = pr.f2u(float(np.float32(table[i + 512])))
    for k in range(pr.NOTES):
        m[pr.RATE + k] = round(RATE_BASE * 2 ** (k / 12))
        m[pr.COEF + k] = pr.f2u(128.8 * 2 ** (-k / 12))
        g, j = divmod(k, 4)
        c = cutoff_coefficient(k)
        m[pr.FILT + 12 * g + j] = pr.f2u(c)  # cB
        m[pr.FILT + 12 * g + 4 + j] = pr.f2u(c)  # cA
        m[pr.FILT + 12 * g + 8 + j] = pr.f2u(1 - 2 * c)  # cC
    return m


def note_signals(depth_word: int, notes, frames: int = FRAMES, stride: int = STRIDE):
    """`records[r][i]` = filtered note `notes[i]` at frame `r * stride`."""
    m = memory(depth_word)
    out = []
    for f in range(frames):
        cur, prev = (pr.OUT_A, pr.OUT_B) if f % 2 == 0 else (pr.OUT_B, pr.OUT_A)
        dst, last = (pr.Y0, pr.Y1) if f % 2 == 0 else (pr.Y1, pr.Y0)
        pr.scanner(m, cur)
        pr.smooth(m, cur, prev, last, dst)
        if f % stride == 0:
            out.append([float(np.float32(float(pr.ld(m[dst + k])))) for k in notes])
    return out


def output_input() -> list[float]:
    """Input of the output stage golden: a step block, then a decaying two tone signal."""
    xs = []
    for n in range(OUT_FRAMES):
        v = 1.0 if 8 <= n < 20 else 0.0
        v += 0.5 * math.sin(0.31 * n) * math.exp(-n / 40) + 0.2 * math.sin(2.2 * n)
        xs.append(float(np.float32(v)))
    return xs


def output_signals(xs: list[float]) -> list[float]:
    """`output_stage` (3 sections, gains, 0.090075) for weight 1 on note word 0."""
    m = pr.Mem()
    weights, params, states, record, i0 = (
        0xB1000,
        0xB1100,
        (0xB1200, 0xB1210, 0xB1220),
        0xB1320,
        0xB1400,
    )
    m[weights] = pr.f2u(1.0)
    m[record - 0x1F], m[record - 0x1E] = weights, params
    m[params], m[params + 1] = pr.f2u(1.0), pr.f2u(0.0)
    m[params + 4], m[params + 5] = pr.f2u(1.0), pr.f2u(1.0)  # g1 state
    m[params + 6], m[params + 7] = 0, 0  # g2 state
    m[params + 2], m[params + 3], m[params + 8] = states[1], states[2], states[0]
    m[params + 0xB] = pr.f2u(1.0)
    for st, (a0, a1, b0, b1) in zip(states, SECTIONS):
        for off, v in zip((4, 5, 6, 7), (a0, a1, b0, b1)):
            m[st + off] = pr.f2u(v)
    out = []
    for call in range(len(xs) // 2):
        m[pr.Y0] = pr.f2u(xs[2 * call])
        m[pr.Y1] = pr.f2u(xs[2 * call + 1])
        pr.output_stage(m, i0, record)
        out.append(float(np.float32(float(pr.ld(m[i0])))))
        out.append(float(np.float32(float(pr.ld(m[i0 + 2])))))
    return out


def fmt(v: float) -> str:
    s = str(np.float32(v))
    return s if any(c in s for c in ".en") else s + ".0"


def rust_file() -> str:
    table = table_values()
    lines = [
        "//! Golden vectors of the Vox note bank, waveshaper and output stage.",
        "//!",
        "//! Regenerate with `python tools/organ_vox_golden.py rust`: the bit-exact SHARC models",
        "//! of `tools/organ_post_ref.py` (verified against the oracle) run on closed-form",
        "//! tables. Derived values only, no firmware data. Provenance:",
        "//! `docs/specs/organ-engine.md` section 3.1.7.",
        "#![allow(clippy::approx_constant, clippy::excessive_precision)]",
        "",
        f"pub(super) const FRAMES: usize = {FRAMES};",
        f"pub(super) const STRIDE: usize = {STRIDE};",
        f"pub(super) const DEPTH: f32 = {DEPTH_WORD} as f32 / 2_147_483_648.0;",
        f"pub(super) const NOTES_PLAIN: [usize; {len(NOTES_PLAIN)}] = {list(NOTES_PLAIN)};",
        f"pub(super) const NOTES_VIB: [usize; {len(NOTES_VIB)}] = {list(NOTES_VIB)};",
        "pub(super) const TABLE_SAMPLES: [(usize, f32); %d] = [" % len(TABLE_INDICES),
    ]
    lines += [f"    ({i}, {fmt(table[i])})," for i in TABLE_INDICES]
    lines.append("];")
    for name, depth, notes in (
        ("Y_PLAIN", 0, NOTES_PLAIN),
        ("Y_VIB", DEPTH_WORD, NOTES_VIB),
    ):
        rec = note_signals(depth, notes)
        lines.append("#[rustfmt::skip]")
        lines.append(f"pub(super) const {name}: [[f32; {len(notes)}]; {len(rec)}] = [")
        lines += ["    [" + ", ".join(fmt(v) for v in r) + "]," for r in rec]
        lines.append("];")
    xs = output_input()
    for name, vals in (("OUT_INPUT", xs), ("OUT_EXPECTED", output_signals(xs))):
        lines.append("#[rustfmt::skip]")
        lines.append(f"pub(super) const {name}: [f32; {len(vals)}] = [")
        lines += [
            "    " + ", ".join(fmt(v) for v in vals[i : i + 6]) + ","
            for i in range(0, len(vals), 6)
        ]
        lines.append("];")
    lines += [
        "",
        "#[test]",
        "fn vectors_have_the_declared_shape() {",
        "    assert_eq!(Y_PLAIN.len(), FRAMES.div_ceil(STRIDE));",
        "    assert_eq!(Y_VIB.len(), Y_PLAIN.len());",
        "    assert_eq!(OUT_INPUT.len(), OUT_EXPECTED.len());",
        "    assert!(Y_PLAIN.iter().flatten().all(|v| v.is_finite() && v.abs() < 1.5));",
        "    assert!(OUT_EXPECTED.iter().any(|v| v.abs() > 0.01));",
        "}",
    ]
    return "\n".join(lines) + "\n"


def measured_cutoffs(exe: str, image: str) -> list[float]:
    """Cutoff ratios of the image low-pass coefficients (`DM(0x95D26)`, 21 groups)."""
    text = subprocess.run(
        [exe, image, "--calls", "1", "--dump", f"{pr.FILT:#x}:252"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    line = next(ln for ln in text.splitlines() if ln.startswith("DM("))
    words = np.array([int(w, 16) for w in line.split(":", 1)[1].split()], dtype="<u4")
    w = words.view("<f4").astype(np.float64)
    out = []
    for g in range(21):
        c = w[12 * g + 4]
        fc = math.atan(c / (1 - c)) / math.pi
        out.append(round(fc / cycles_per_frame(4 * g), 4))
    return out


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("rust")
    c = sub.add_parser("cutoffs")
    c.add_argument("image")
    c.add_argument("--exe", required=True)
    ns = ap.parse_args(argv)
    if ns.cmd == "rust":
        sys.stdout.write(rust_file())
    else:
        ratios = measured_cutoffs(ns.exe, ns.image)
        print(ratios)
        print("matches CUTOFF_RATIO:", tuple(ratios) == CUTOFF_RATIO)
    return 0


if __name__ == "__main__":
    sys.exit(main())
