#!/usr/bin/env python3
"""Rust data files of the firmware synth filter family (`crates/nord-core/src/synth/fw_*`).

    python tools/synth_filter_golden.py tables > crates/nord-core/src/synth/fw_filter_tables.rs
    python tools/synth_filter_golden.py rates  > crates/nord-core/src/synth/fw_env_rates.rs
    python tools/synth_filter_golden.py golden > \
        crates/nord-core/src/synth/tests_fw_filter_golden.rs
    cargo fmt -p nord-core

`tables` and `rates` turn the derived coefficient curves of `docs/research/data/` (float32 values
read from the Core2 image by the research PR, no code or firmware bytes) into `u32` bit patterns.
`golden` runs the bit-exact model `synth_filter_model.py` (checked against the SHARC oracle:
`0x12566E`, `0x125913`, `0x125A50`) over fixed rows and a fixed input and prints the coefficient
words, the output words and the closed-form low-pass gains, so the Rust float32 filter is compared
with the model on identical inputs. Everything printed is derived from the model.
"""

from __future__ import annotations

import csv
import math
import struct
import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import sharc40 as s
import synth_filter_model as m

DATA = Path(__file__).resolve().parent.parent / "docs" / "research" / "data"
FILTER_CSV = DATA / "synth-filter-tables.csv"
RATE_CSV = DATA / "synth-env-rate-table.csv"

# Golden rows: (table position word W28, cutoff coefficient R). Index 64 is the oracle row; the
# second row mixes a fraction at a low index, the third drives the clip at a high resonance.
ROWS = ((0x40000000, 0x3E99999A), (0x18800000, 0x3DCCCCCD), (0x7F400000, 0x3F19999A))
SAMPLES = 48
RESPONSE_FREQS = (0.005, 0.02, 0.04, 0.06, 0.08, 0.1, 0.15, 0.3)  # cycles per sample
TYPES = ("lp", "bp", "hp")


def f2u(x: float) -> int:
    """Float32 bit pattern of `x`."""
    return struct.unpack("<I", struct.pack("<f", x))[0]


def read_csv(path: Path) -> list[list[float]]:
    """Rows of a data CSV as floats (header skipped)."""
    with path.open(newline="", encoding="utf-8") as fh:
        rows = list(csv.reader(fh))
    return [[float(v) for v in r] for r in rows[1:]]


def filter_tables() -> tuple[list[int], list[int], list[int]]:
    """`(A, C, B)` table words (129 entries each) from the CSV columns."""
    rows = read_csv(FILTER_CSV)
    return (
        [f2u(r[1]) for r in rows],
        [f2u(r[2]) for r in rows],
        [f2u(r[3]) for r in rows],
    )


def rate_tables() -> tuple[list[int], list[int]]:
    """`(attack, decay_release)` coefficient words for the signed index -64..64."""
    rows = read_csv(RATE_CSV)
    return [f2u(r[1]) for r in rows], [f2u(r[2]) for r in rows]


def input_words() -> list[int]:
    """Fixed input: an impulse, a decaying sine and a step, as float32 words."""
    xs = []
    for n in range(SAMPLES):
        x = 0.9 if n == 0 else 0.0
        x += 0.5 * (0.97**n) * math.sin(0.9 * n)
        x += 0.4 if n >= SAMPLES // 2 else 0.0
        xs.append(f2u(x))
    return xs


def run_row(w28: int, r_word: int, a: list[int], b: list[int], c: list[int]):
    """`{type: (coefficient words, output words)}` of the three stages for one row."""
    xs = input_words()
    lp_co = m.coefficients(w28, r_word, a, b)
    bp_co = m.coefficients(w28, r_word, c, b)
    return {
        "lp": (lp_co, m.process(m.Lane(), lp_co, xs)),
        "bp": (bp_co, m.process_bp(m.Lane(), bp_co, xs)),
        "hp": (lp_co, m.process_hp(m.Lane(), lp_co, xs)),
    }


def response(w28: int, r_word: int, a: list[int], b: list[int]) -> list[int]:
    """Closed-form low-pass gains (float32 words) at `RESPONSE_FREQS` for one row."""
    idx, fr = m.split_word(w28)
    cin = float(m.interp(a[idx], a[idx + 1], fr))
    p = float(m.interp(b[idx], b[idx + 1], fr))
    r = float(s.ld(r_word))
    return [f2u(m.lp4_gain(r, p, f, cin)) for f in RESPONSE_FREQS]


def words(name: str, vals: list[int]) -> str:
    """`pub(super) const NAME: [u32; N] = [..];` with hex words."""
    body = ", ".join(f"{v:#010x}" for v in vals)
    return f"pub(super) const {name}: [u32; {len(vals)}] = [{body}];\n"


def grid(name: str, doc: str, rows: list[list[int]]) -> str:
    """A `[[u32; M]; N]` constant with a doc line."""
    body = ", ".join("[" + ", ".join(f"{v:#010x}" for v in r) + "]" for r in rows)
    return f"/// {doc}\npub(super) const {name}: [[u32; {len(rows[0])}]; {len(rows)}] = [{body}];\n"


def emit_tables() -> str:
    """Text of `fw_filter_tables.rs`."""
    a, c, b = filter_tables()
    return (
        FILTER_HEADER
        + words("TABLE_A_BITS", a)
        + words("TABLE_C_BITS", c)
        + words("TABLE_B_BITS", b)
        + FILTER_TAIL
    )


def emit_rates() -> str:
    """Text of `fw_env_rates.rs`."""
    att, dec = rate_tables()
    return RATE_HEADER + words("ATTACK_BITS", att) + words("DECAY_BITS", dec) + RATE_TAIL


def emit_golden() -> str:
    """Text of `tests_fw_filter_golden.rs`."""
    a, c, b = filter_tables()
    coefs, outs, resp = [], [], []
    for w28, r_word in ROWS:
        res = run_row(w28, r_word, a, b, c)
        for t in TYPES:
            coefs.append(list(res[t][0]))
            outs.append(res[t][1])
        resp.append(response(w28, r_word, a, b))
    rows = ", ".join(f"({w:#010x}, {r:#010x})" for w, r in ROWS)
    freqs = ", ".join(repr(f) for f in RESPONSE_FREQS)
    return "".join(
        [
            GOLDEN_HEADER,
            words("INPUT", input_words()),
            f"pub(super) const ROWS: [(u32, u32); {len(ROWS)}] = [{rows}];\n",
            f"pub(super) const RESPONSE_FREQS: [f32; {len(RESPONSE_FREQS)}] = [{freqs}];\n",
            grid("COEFS", "Per row and type (lp, bp, hp): `cin`, `g`, `k` words.", coefs),
            grid("OUTPUT", "Per row and type (lp, bp, hp): output words of the model.", outs),
            grid("LP_RESPONSE", "Per row: closed-form low-pass gain at `RESPONSE_FREQS`.", resp),
            GOLDEN_TAIL,
        ]
    )


FILTER_HEADER = """//! Resonance tables of the firmware synth filter family (derived values only).
//!
//! Generated by `python tools/synth_filter_golden.py tables` from
//! `docs/research/data/synth-filter-tables.csv` (float32 values read from the ORG core 2 image,
//! `docs/research/synth-stage-equations.md` section 3.1). 129 entries so that `idx + 1` of the
//! last index 127 exists. `A` at DM `0xB0D0F`, `C` at `0xB0D90`, `B` at `0xB1015`.
//!
//! VERIFIED: every word equals the CSV value (test `tables_equal_the_research_csv`); the CSV is
//! the model input of the bit-exact check `tools/tests/test_synth_filter_model.py` against the
//! SHARC oracle (0 mismatches over 9 + 6 + 6 rows x 64 samples).

"""

FILTER_TAIL = """
#[cfg(test)]
mod tests {
    use super::*;

    const CSV: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/research/data/synth-filter-tables.csv"
    ));

    #[test]
    fn tables_equal_the_research_csv() {
        let rows: Vec<Vec<f32>> = CSV
            .lines()
            .skip(1)
            .map(|l| l.split(',').map(|v| v.trim().parse().unwrap()).collect())
            .collect();
        assert_eq!(rows.len(), TABLE_A_BITS.len());
        for (i, r) in rows.iter().enumerate() {
            assert_eq!(f32::from_bits(TABLE_A_BITS[i]), r[1], "A[{i}]");
            assert_eq!(f32::from_bits(TABLE_C_BITS[i]), r[2], "C[{i}]");
            assert_eq!(f32::from_bits(TABLE_B_BITS[i]), r[3], "B[{i}]");
        }
    }

    #[test]
    fn tables_have_129_entries_and_known_anchors() {
        assert_eq!(TABLE_A_BITS.len(), 129);
        assert_eq!(TABLE_B_BITS.len(), 129);
        assert_eq!(TABLE_C_BITS.len(), 129);
        assert_eq!(f32::from_bits(TABLE_B_BITS[0]), 2.0);
        assert!((f32::from_bits(TABLE_A_BITS[64]) - 0.99).abs() < 1e-6);
        assert!((f32::from_bits(TABLE_C_BITS[0]) - 1.98).abs() < 1e-6);
    }
}
"""

RATE_HEADER = """//! Envelope rate coefficient tables (derived values, no firmware bytes).
//!
//! Generated by `python tools/synth_filter_golden.py rates` from
//! `docs/research/data/synth-env-rate-table.csv`: the per-pass coefficient the Core2 installer
//! (`0x12720F..0x127281`) stores for each signed index -64..64 (top byte of an ADR strip word),
//! `docs/research/synth-stage-equations.md` section 7. Index `i` is at array position `i + 64`.
//!
//! VERIFIED: every word equals the CSV value (test `tables_equal_the_research_csv`); the CSV is
//! a sweep of the oracle pass (3 passes per index, coefficient words read from `DM(0xC0E21..)`).

"""

RATE_TAIL = """
/// Most negative / positive table index (`DM(0xC135A) = 0x40000000` is +0.5, byte 3 = 64).
pub const RATE_INDEX_MAX: i32 = 64;

/// Signed table index of an ADR strip word: the word is clipped to +-0x40000000 and byte 3 is the
/// index (`fext 24:8 (se)` of the installer `0x12720F..0x127281`).
///
/// VERIFIED: listing read plus the 12-pass diff of `docs/research/synth-stage-equations.md`
/// section 7 (poking `E001A..E0022` changes exactly the coefficient words).
#[inline]
pub fn rate_index_from_word(word: i32) -> i32 {
    word.clamp(-0x4000_0000, 0x4000_0000) >> 24
}

#[inline]
fn lookup(table: &[u32; 129], index: f32) -> f32 {
    let p = if index.is_nan() { 0.0 } else { index.clamp(-64.0, 64.0) + 64.0 };
    let i = (p as usize).min(127);
    let fr = p - i as f32;
    let (a, b) = (f32::from_bits(table[i]), f32::from_bits(table[i + 1]));
    a + (b - a) * fr
}

/// Attack coefficient for a (fractional) index in -64..=64 (one table for all three attack words).
///
/// VERIFIED at integer indexes (CSV words); linear interpolation between them is ASSUMED (the
/// firmware reads integer indexes only). Reads as the per-pass rate of a segment moving toward its
/// target; the segment generator is not located (`synth-stage-equations.md` section 7), so the
/// engine does not consume it yet.
#[inline]
pub fn attack_coeff(index: f32) -> f32 {
    lookup(&ATTACK_BITS, index)
}

/// Decay / release coefficient (`1 - c` is the per-pass decay of a one pole) for a (fractional)
/// index in -64..=64; same table for decay and release. Same tags as [`attack_coeff`].
#[inline]
pub fn decay_release_coeff(index: f32) -> f32 {
    lookup(&DECAY_BITS, index)
}

/// Time constant in passes of the attack rate, `1 / cA`.
///
/// ASSUMED: one pass is one sample (it may be two frames); 10.9 passes at -64 to 1.0e6 at +64.
#[inline]
pub fn attack_time_passes(index: f32) -> f32 {
    1.0 / attack_coeff(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_comes_from_byte_3_clipped_to_plus_minus_64() {
        assert_eq!(rate_index_from_word(0), 0);
        assert_eq!(rate_index_from_word(0x4000_0000), 64);
        assert_eq!(rate_index_from_word(i32::MAX), 64);
        assert_eq!(rate_index_from_word(-0x4000_0000), -64);
        assert_eq!(rate_index_from_word(i32::MIN), -64);
        assert_eq!(rate_index_from_word(0x0100_0000), 1);
        assert_eq!(rate_index_from_word(-0x0000_0001), -1);
    }

    #[test]
    fn coefficients_match_the_documented_anchors() {
        assert!((attack_coeff(-64.0) - 0.092_161_65).abs() < 1e-7);
        assert!((attack_coeff(0.0) - 4.505e-5).abs() < 1e-8);
        assert!((attack_coeff(64.0) - 1.0e-6).abs() < 1e-8);
        assert!((1.0 - decay_release_coeff(-64.0) - 0.068_29).abs() < 1e-5);
        assert!((1.0 - decay_release_coeff(0.0) - 1.4257e-4).abs() < 1e-8);
        assert!((attack_time_passes(-64.0) - 10.85).abs() < 0.1);
    }

    #[test]
    fn lookup_is_monotonic_clamped_and_interpolates() {
        assert_eq!(attack_coeff(-100.0), attack_coeff(-64.0));
        assert_eq!(attack_coeff(100.0), attack_coeff(64.0));
        assert_eq!(attack_coeff(f32::NAN), attack_coeff(-64.0));
        let mid = attack_coeff(10.5);
        assert!(mid < attack_coeff(10.0) && mid > attack_coeff(11.0));
        let mut prev = f32::INFINITY;
        for i in -64..=64 {
            let c = attack_coeff(i as f32);
            assert!(c <= prev, "attack not monotonic at {i}");
            prev = c;
        }
    }

    const CSV: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/research/data/synth-env-rate-table.csv"
    ));

    #[test]
    fn tables_equal_the_research_csv() {
        let rows: Vec<Vec<f32>> = CSV
            .lines()
            .skip(1)
            .map(|l| l.split(',').map(|v| v.trim().parse().unwrap()).collect())
            .collect();
        assert_eq!(rows.len(), ATTACK_BITS.len());
        for (i, r) in rows.iter().enumerate() {
            assert_eq!(r[0], i as f32 - 64.0, "index column");
            assert_eq!(f32::from_bits(ATTACK_BITS[i]), r[1], "attack[{i}]");
            assert_eq!(f32::from_bits(DECAY_BITS[i]), r[2], "decay[{i}]");
        }
    }
}
"""

GOLDEN_HEADER = """//! Golden words of the firmware filter family (`fw_filter.rs`).
//!
//! Generated by `python tools/synth_filter_golden.py golden`: the bit-exact Python model
//! `tools/synth_filter_model.py` (equal to the SHARC oracle for `0x12566E`, `0x125913`, `0x125A50`)
//! run over `ROWS` and `INPUT`. Derived values only.

use super::fw_filter::{FwCoefs, FwFilter, FwFilterType, TablePos};

const KINDS: [FwFilterType; 3] = [FwFilterType::Lp4, FwFilterType::Bp4, FwFilterType::Hp4];

"""

GOLDEN_TAIL = """
/// Worst relative difference of the Rust prologue words from the model words.
fn worst_coef_error() -> f32 {
    let mut worst = 0.0f32;
    for (row, &(w28, r_word)) in ROWS.iter().enumerate() {
        let (r, pos) = (f32::from_bits(r_word), TablePos::from_word(w28));
        for (t, kind) in KINDS.iter().enumerate() {
            let co = FwCoefs::new(*kind, r, pos);
            for (got, want) in [co.cin, co.g, co.k].iter().zip(COEFS[row * 3 + t]) {
                let want = f32::from_bits(want);
                worst = worst.max(((got - want) / want).abs());
            }
        }
    }
    worst
}

/// Worst absolute difference of the Rust output from the model output.
fn worst_output_error() -> f32 {
    let mut worst = 0.0f32;
    for (row, &(w28, r_word)) in ROWS.iter().enumerate() {
        let (r, pos) = (f32::from_bits(r_word), TablePos::from_word(w28));
        for (t, kind) in KINDS.iter().enumerate() {
            let co = FwCoefs::new(*kind, r, pos);
            let mut f = FwFilter::default();
            for (x, want) in INPUT.iter().zip(OUTPUT[row * 3 + t]) {
                let got = f.process(f32::from_bits(*x), &co, *kind);
                worst = worst.max((got - f32::from_bits(want)).abs());
            }
        }
    }
    worst
}

/// Worst relative difference of the measured impulse spectrum from the closed form (low pass).
fn worst_response_error() -> f32 {
    let mut worst = 0.0f32;
    for (row, &(w28, r_word)) in ROWS.iter().enumerate() {
        let (r, pos) = (f32::from_bits(r_word), TablePos::from_word(w28));
        let co = FwCoefs::new(FwFilterType::Lp4, r, pos);
        let mut f = FwFilter::default();
        let y: Vec<f32> = (0..1 << 16)
            .map(|n| f.process(if n == 0 { 1.0 } else { 0.0 }, &co, FwFilterType::Lp4))
            .collect();
        for (freq, want) in RESPONSE_FREQS.iter().zip(LP_RESPONSE[row]) {
            let w = std::f64::consts::TAU * f64::from(*freq);
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for (n, v) in y.iter().enumerate() {
                re += f64::from(*v) * (w * n as f64).cos();
                im -= f64::from(*v) * (w * n as f64).sin();
            }
            let got = (re * re + im * im).sqrt() as f32;
            let want = f32::from_bits(want);
            worst = worst.max(((got - want) / want).abs());
        }
    }
    worst
}

#[test]
fn rust_prologue_matches_the_model_words() {
    let worst = worst_coef_error();
    println!("worst relative coefficient error {worst:e}");
    assert!(worst < 5e-6, "worst relative coefficient error {worst}");
}

#[test]
fn rust_output_matches_the_model_words_within_float32_rounding() {
    let worst = worst_output_error();
    println!("worst absolute output error {worst:e}");
    assert!(worst < 2e-5, "worst absolute output error {worst}");
}

#[test]
fn rust_low_pass_response_matches_the_closed_form() {
    let worst = worst_response_error();
    println!("worst relative response error {worst:e}");
    assert!(worst < 2e-3, "worst relative response error {worst}");
}

#[test]
fn golden_data_has_the_documented_shape() {
    assert_eq!(INPUT.len(), 48);
    assert_eq!(COEFS.len(), ROWS.len() * 3);
    assert_eq!(OUTPUT.len(), COEFS.len());
    assert_eq!(LP_RESPONSE.len(), ROWS.len());
    // the model low pass has no feed through
    assert_eq!(OUTPUT[0][0], 0);
}
"""


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    sys.stdout.reconfigure(newline="\n")  # LF on Windows too
    mode = (argv if argv is not None else sys.argv[1:]) or ["golden"]
    emit = {"tables": emit_tables, "rates": emit_rates, "golden": emit_golden}[mode[0]]
    sys.stdout.write(emit())
    return 0


if __name__ == "__main__":
    sys.exit(main())
