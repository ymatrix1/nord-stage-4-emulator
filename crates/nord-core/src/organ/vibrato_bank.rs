//! Pitch and gain programming of the firmware oscillator bank: exact 12-TET ladder, the detuned
//! vibrato/chorus ensemble copies and the group gains the ARM routine `0xC022D9F0` sends.
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1.2. Research: `docs/research/arm-organ-messages.md`
//! section 7 (ARM run of `0xC022D9F0`, `tools/arm_bank.py`), `docs/research/data/arm-bank-increments.csv`.
//!
//! Element `j = 2 w + c`: wheel `w` (0..91, the 92nd spare element is unused), copy `c`.

use super::types::NUM_TONEWHEELS;

/// Number of programmed bank elements (92 wheels x 2 copies).
pub const BANK_ELEMENTS: usize = 184;

/// MIDI note of wheel 0.
///
/// ASSUMED: wheel 0 = MIDI 24 (C1), the existing project mapping (`drawbar::foldback_tonewheel`).
/// The firmware ladder is 92 chromatic semitones (VERIFIED, step 100.00 cent) whose absolute
/// pitch depends on the DSP frame rate and the tune word, neither known from the image.
pub const WHEEL0_MIDI: i32 = 24;

/// Reference pitch of MIDI 69 before Fine Tune.
///
/// ASSUMED: 440 Hz (the instrument's concert pitch default).
pub const A4_HZ: f64 = 440.0;

/// Fine Tune range in cents: the firmware tune table (`bank0_CODE@0xC027D9D8`, step 0.78125
/// cent) spans index 0 = -400 cent to index 1000 = +381 cent.
///
/// VERIFIED: table ends from `docs/research/organ-tonewheel-bank.md` section 1; the mapping of
/// the Fine Tune parameter onto it (cents as `r1`) is ASSUMED.
pub const FINE_TUNE_RANGE_CENTS: (f32, f32) = (-400.0, 381.0);

/// Bank-programming gains of the vibrato-enable-off case (setter indices 5..8, flag 0).
///
/// VERIFIED: `0x7FFFFF, 0, 0x7FFFFF, 0` logged by the real `0xC022D9F0` run for levels 0, 2, 3
/// (`python tools/arm_bank.py`, `docs/research/arm-organ-messages.md` section 7).
pub const GAINS_EXACT: [u32; 4] = [0x7F_FFFF, 0, 0x7F_FFFF, 0];

/// Bank-programming gains of the vibrato-enable-on case (level 1, flag 0).
///
/// VERIFIED: `0x600000, 0x600000, 0x300000, 0x300000`, same run as [`GAINS_EXACT`].
pub const GAINS_DETUNED: [u32; 4] = [0x60_0000, 0x60_0000, 0x30_0000, 0x30_0000];

/// Detune of every bank element in cents: `1200 log2(detuned_word / exact_12tet_word)`.
///
/// VERIFIED: derived from the 184 increment words of the detuned table `bank0_CODE@0x443EA8`
/// against the exact 12-TET table `@0x444188` (`docs/research/data/arm-bank-increments.csv`,
/// `index`-ordered), rounded to 0.01 cent (word quantisation of the source is about 0.03 cent).
/// Mean +0.11 cent, rms 1.41 cent, range -2.49..+2.46; the two copies of one wheel differ by
/// 1.90 cent rms. Checked by `tests::detune_table_statistics` (statistics) and
/// `tests::detune_table_spot_values` (first elements). The role of the detune as a vibrato /
/// chorus ensemble effect is ASSUMED (it follows the vibrato-enable level in the ARM routine).
pub const DETUNE_CENTS: [f32; BANK_ELEMENTS] = [
    -0.20, -1.43, -2.19, 1.80, -0.30, 0.18, 2.46, 0.86, 1.73, -1.46, 0.97, -0.10, 1.64, -0.53,
    1.18, -0.77, -0.94, -1.24, 0.24, 2.10, 1.34, -2.06, 1.73, -1.62, -0.17, -2.49, 2.12, 0.29,
    1.97, -2.31, 2.46, -1.37, -1.59, -1.84, 2.06, 0.48, 1.20, -1.49, 0.34, -2.07, 1.61, 1.18, 1.78,
    1.82, -1.85, -0.40, -0.25, 1.77, -1.75, -0.61, 0.74, -0.06, 0.50, 2.12, 1.30, 1.89, 1.58, 1.30,
    0.41, 2.24, -2.01, -2.23, 0.05, 0.06, 0.80, -2.01, 0.66, 1.94, -1.40, 0.55, 1.29, 0.48, 2.02,
    1.84, -2.21, -2.28, -0.38, 2.20, 1.32, 0.73, -1.61, 2.42, 2.29, 1.66, -0.75, -1.16, -0.74,
    0.25, -0.24, 1.02, -1.23, 1.55, -0.94, 1.59, 0.66, -0.63, -1.40, 2.10, -2.15, -1.38, 0.06,
    0.32, 0.27, 0.97, -0.04, -0.05, -1.59, -1.47, -0.72, 0.60, -1.56, -0.16, 0.75, -2.20, -1.88,
    -0.25, -0.61, -0.43, 1.46, 0.99, 0.15, 1.35, -1.72, 1.53, 1.37, 1.65, 1.36, 0.32, 0.95, -2.12,
    0.35, 1.58, -2.03, -2.40, 0.31, 1.94, 1.21, 0.46, -0.47, 1.19, -0.51, -1.06, 1.39, 1.91, 1.02,
    0.91, 0.38, -0.04, 2.07, 1.19, -1.51, 2.35, -2.15, -1.15, 0.06, -0.39, 1.31, 0.21, -2.32,
    -0.23, -0.11, 1.34, 0.06, -0.62, -0.90, 0.52, 0.66, -2.44, 2.31, -1.93, -2.07, -1.48, 2.01,
    1.19, 0.82, -0.90, 1.82, -1.11, 1.97, -0.77, 0.03, -1.40, -1.08, -2.02,
];

/// Clamps Fine Tune to the firmware tune-table range; non-finite input gives `0.0`.
pub fn clamp_fine_tune(fine_cents: f32) -> f32 {
    if fine_cents.is_finite() {
        fine_cents.clamp(FINE_TUNE_RANGE_CENTS.0, FINE_TUNE_RANGE_CENTS.1)
    } else {
        0.0
    }
}

/// 12-TET frequency of wheel `w` in Hz with Fine Tune `fine_cents` applied.
///
/// VERIFIED ladder: the firmware exact table is 12-TET, copies identical, max deviation from
/// `2^(w/12)` 0.055 cent (word quantisation of the source); ASSUMED anchor `A4_HZ`, `WHEEL0_MIDI`.
pub fn wheel_hz(w: usize, fine_cents: f32) -> f64 {
    let semis = f64::from(WHEEL0_MIDI + w.min(NUM_TONEWHEELS - 1) as i32 - 69);
    A4_HZ * ((semis * 100.0 + f64::from(clamp_fine_tune(fine_cents))) / 1200.0).exp2()
}

/// Frequency ratio of element `(w, c)` against the exact ladder: `1.0` when exact, the table
/// detune when `detuned`.
pub fn copy_ratio(w: usize, c: usize, detuned: bool) -> f64 {
    if detuned {
        (f64::from(DETUNE_CENTS[2 * w.min(BANK_ELEMENTS / 2 - 1) + c.min(1)]) / 1200.0).exp2()
    } else {
        1.0
    }
}

/// Mix weights of the two copies, from the firmware group gains 5 and 6 normalised to unit sum.
///
/// The gains are VERIFIED ([`GAINS_EXACT`], [`GAINS_DETUNED`]); that groups 5 and 6 weight the
/// two x copies and groups 7, 8 another path (not modelled) is ASSUMED, as is the normalisation
/// that keeps the level equal across the vibrato switch (the raw gains would raise the detuned
/// case by up to 3.5 dB). Exact: `[1, 0]` (the identical second copy is not run), detuned
/// `[0.5, 0.5]`.
pub fn copy_weights(detuned: bool) -> [f32; 2] {
    let g = if detuned { GAINS_DETUNED } else { GAINS_EXACT };
    let (a, b) = (g[0] as f32, g[1] as f32);
    [a / (a + b), b / (a + b)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cents(a: f64, b: f64) -> f64 {
        1200.0 * (a / b).log2()
    }

    #[test]
    fn ladder_is_exact_12_tet_with_a4_anchor() {
        assert!((wheel_hz(45, 0.0) - 440.0).abs() < 1e-9);
        for w in 0..NUM_TONEWHEELS - 1 {
            let step = cents(wheel_hz(w + 1, 0.0), wheel_hz(w, 0.0));
            assert!((step - 100.0).abs() < 1e-9, "{w}: {step}");
        }
        assert!((wheel_hz(0, 0.0) - 32.703_195_66).abs() < 1e-6); // C1
        assert!((wheel_hz(12, 0.0) / wheel_hz(0, 0.0) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn fine_tune_scales_and_clamps() {
        let up = cents(wheel_hz(45, 25.0), wheel_hz(45, 0.0));
        assert!((up - 25.0).abs() < 1e-6, "{up}");
        assert_eq!(clamp_fine_tune(1000.0), 381.0);
        assert_eq!(clamp_fine_tune(-1000.0), -400.0);
        assert_eq!(clamp_fine_tune(f32::NAN), 0.0);
        assert_eq!(wheel_hz(45, f32::INFINITY), wheel_hz(45, 0.0));
    }

    #[test]
    fn detune_table_statistics() {
        let n = BANK_ELEMENTS as f64;
        let mean: f64 = DETUNE_CENTS.iter().map(|&c| f64::from(c)).sum::<f64>() / n;
        let rms = (DETUNE_CENTS
            .iter()
            .map(|&c| f64::from(c).powi(2))
            .sum::<f64>()
            / n)
            .sqrt();
        assert!((mean - 0.11).abs() < 0.01, "{mean}");
        assert!((rms - 1.41).abs() < 0.01, "{rms}");
        let (lo, hi) = DETUNE_CENTS
            .iter()
            .fold((f32::MAX, f32::MIN), |(l, h), &c| (l.min(c), h.max(c)));
        assert_eq!((lo, hi), (-2.49, 2.46));
        let (pairs, rest) = DETUNE_CENTS.as_chunks::<2>();
        assert!(rest.is_empty());
        let spread = (pairs
            .iter()
            .map(|p| f64::from(p[1] - p[0]).powi(2))
            .sum::<f64>()
            / pairs.len() as f64)
            .sqrt();
        assert!((spread - 1.90).abs() < 0.01, "{spread}");
    }

    #[test]
    fn detune_table_spot_values() {
        // csv rows 0..3: exact words 0x635300 / 0x693B00, detuned 0x635000, 0x633E00, 0x691900,
        // 0x695700 (the low byte is always 0, the increments are `value << 8`)
        let want = [
            cents(f64::from(0x6350u32), f64::from(0x6353u32)),
            cents(f64::from(0x633Eu32), f64::from(0x6353u32)),
            cents(f64::from(0x6919u32), f64::from(0x693Bu32)),
            cents(f64::from(0x6957u32), f64::from(0x693Bu32)),
        ];
        for (j, w) in want.iter().enumerate() {
            assert!((f64::from(DETUNE_CENTS[j]) - w).abs() < 0.006, "{j}");
        }
    }

    #[test]
    fn copy_ratio_and_weights_follow_the_vibrato_table() {
        assert_eq!(copy_ratio(5, 1, false), 1.0);
        let r = copy_ratio(1, 1, true);
        assert!((cents(r, 1.0) - f64::from(DETUNE_CENTS[3])).abs() < 1e-9);
        assert_eq!(copy_weights(false), [1.0, 0.0]);
        assert_eq!(copy_weights(true), [0.5, 0.5]);
        assert!((f64::from(GAINS_DETUNED[0]) / f64::from(GAINS_EXACT[0]) - 0.75).abs() < 1e-6);
        assert!((f64::from(GAINS_DETUNED[2]) / f64::from(GAINS_EXACT[0]) - 0.375).abs() < 1e-6);
    }
}
