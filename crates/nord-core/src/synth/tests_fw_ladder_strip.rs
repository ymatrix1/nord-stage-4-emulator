//! End-to-end Ladder24 cutoff law: strip word 3 -> pass control `R` -> stage `0x12549D`.
//!
//! Recorded by `python tools/synth_filter_ladder_strip.py --record` (ORG core 2 oracle with the
//! pass control block of the image left in place, frame read after pass 5): per strip word the
//! `R` word the control block wrote to `DM(-30)`, the resonance word `DM(-28)` and the four
//! coefficient words the stage stored. Lane 0 only (both lanes are equal in the recording).
//! `docs/research/data/synth-ladder-strip-cases.json` is the source. Do not edit by hand.

use super::fw_filter::TablePos;
use super::fw_ladder::LadderCoefs;
use super::fw_strip_cutoff::fw_strip_cutoff_r;

/// `(strip word, R word, resonance word, [b, a, c, kf])`.
pub(super) const CASES: [(u32, u32, u32, [u32; 4]); 7] = [
    (
        0x00000000,
        0x3eb8be4e,
        0x40000000,
        [0x3c6efdf8, 0x3b9bf288, 0xbf7b0c22, 0x3f0008a3],
    ),
    (
        0x08000000,
        0x3ed848f7,
        0x40000000,
        [0x3cba86f2, 0x3bf809bd, 0xbf783bb4, 0x3efffc55],
    ),
    (
        0x10000000,
        0x3ef7d3a1,
        0x40000000,
        [0x3d11b97f, 0x3c4238d6, 0xbf73db84, 0x3effee0c],
    ),
    (
        0x20000000,
        0x3f1b7479,
        0x40000000,
        [0x3daf2d82, 0x3ce9b2e8, 0xbf62ccb7, 0x3eff8c3d],
    ),
    (
        0x30000000,
        0x3f3aff23,
        0x40000000,
        [0x3e498f63, 0x3d87594e, 0xbf3cb0fc, 0x3efd2351],
    ),
    (
        0x40000000,
        0x3f5a89cc,
        0x40000000,
        [0x3ed5acc2, 0x3e153b85, 0xbedfb57a, 0x3eeeb58a],
    ),
    (
        0x50000000,
        0x3f7a1476,
        0x40000000,
        [0x3f495445, 0x3eb745ff, 0x3e13dd17, 0x3ea393c3],
    ),
];

fn close(got: f32, want: u32) -> bool {
    let w = f32::from_bits(want);
    (got - w).abs() <= 2e-6 * w.abs() + 1e-9
}

#[test]
fn strip_cases_match_the_stage() {
    for (word, r, q, want) in CASES {
        let res = TablePos::from_word(q);
        for co in [
            LadderCoefs::from_words(r, q),
            LadderCoefs::from_strip_r(f32::from_bits(r), res),
        ] {
            let got = [co.b, co.a, co.c, co.kf];
            for k in 0..4 {
                assert!(
                    close(got[k], want[k]),
                    "word {word:#x} coef {k}: {}",
                    got[k]
                );
            }
        }
    }
}

#[test]
fn the_cutoff_word_rises_with_the_strip_word() {
    let rs: Vec<f32> = CASES.iter().map(|c| f32::from_bits(c.1)).collect();
    assert!((rs[0] - 0.36083).abs() < 1e-5);
    assert!(rs.windows(2).all(|w| w[0] < w[1]));
}

/// Section pole frequency in Hz at 48 kHz of the Ladder24 stage for an ARM knob 0..117
/// (word top byte `knob - 52`, `docs/research/data/synth-filter-freq-law.csv`).
fn pole_hz(knob: i32) -> f32 {
    let word = ((knob - 52) & 0xFF) << 24;
    let r = fw_strip_cutoff_r(word);
    let co = LadderCoefs::from_strip_r(r, TablePos::from_word(0x4000_0000));
    -(-co.c).ln() / std::f32::consts::TAU * 48_000.0
}

#[test]
fn pole_frequency_follows_the_knob_exponentially() {
    // VERIFIED (coefficient words of the oracle, knob law of the ARM): 74 Hz .. 7.8 kHz
    for (knob, hz) in [
        (40, 73.8),
        (52, 149.2),
        (64, 301.8),
        (80, 772.1),
        (100, 2534.8),
        (117, 7843.5),
    ] {
        let got = pole_hz(knob);
        assert!((got - hz).abs() < 0.03 * hz, "knob {knob}: {got} vs {hz}");
    }
}
