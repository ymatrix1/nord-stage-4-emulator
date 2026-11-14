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

/// Number of wheels the firmware mix stage weights: index `i` feeds elements `2 i` and `2 i + 1`.
///
/// VERIFIED: poking the real mix stage `0x12268F` in the oracle (`sharc-organ`,
/// `docs/research/organ-oracle-audio.md` section 3): weight index 0..=91 each add `x[2 i]` to
/// lane x and `x[2 i + 1]` to lane y with weight 1.0; the old "91 pairs, the 92nd is a spare"
/// reading came from a synthetic record whose scalar tail was zero.
pub const BANK_WHEELS: usize = 92;

/// Conversion of an ARM group gain (24-bit integer) to the float the mix stage multiplies by.
///
/// ASSUMED: `value / 2^23` (so `0x600000` is 0.75). The oracle shows that the DSP words are plain
/// floats with 1.0 as unity; the ARM-side conversion `0xC001FF18` was not decoded further.
pub fn gain_value(raw: u32) -> f32 {
    raw as f32 / 8_388_608.0
}

/// Lane matrix `[[x <- copy 0, x <- copy 1], [y <- copy 0, y <- copy 1]]` of the mix stage for the
/// group gains `[g5, g6, g7, g8]`: `out_x = g5 tx + g8 ty`, `out_y = g6 ty + g7 tx`, where
/// `tx`, `ty` are the weighted sums of copy 0 (even elements) and copy 1 (odd elements).
///
/// VERIFIED: steady-state oracle runs with `(g5, g6, g7, g8) = (0.75, 0.75, 0.375, 0.375)` give
/// `(0.750, 0.375)` for a unit in copy 0 and `(0.375, 0.750)` for a unit in copy 1, and
/// `(1, 0, 1, 0)` gives `(1, 1)` and `(0, 0)`; `tools/organ_oracle_audio.py verify` reproduces a
/// 16,384-frame chord to 1.8e-5 relative rms in both modes. Which DSP word each gain index lands
/// on (`0x959AF`, `0x959B0`, `0x95B3F`, `0x95B3E`) is VERIFIED by poking.
pub fn lane_matrix(gains: [u32; 4]) -> [[f32; 2]; 2] {
    let [g5, g6, g7, g8] = gains.map(gain_value);
    [[g5, g8], [g7, g6]]
}

/// Mix weights of the two copies for a mono output: the mean of the two lanes of [`lane_matrix`].
///
/// Exact: `[1, 0]` (lane x = lane y = copy 0). Detuned: `[0.5625, 0.5625]` (lanes
/// `0.75 a + 0.375 b` and `0.375 a + 0.75 b`, so the detuned ensemble is 1.02 dB louder than the
/// exact one for equal copies; the old normalised `[0.5, 0.5]` hid that). VERIFIED arithmetic
/// ([`lane_matrix`]); the mono mean is the `Manual` link; the `Firmware` link routes both lanes (`lane_mix.rs`).
pub fn copy_weights(detuned: bool) -> [f32; 2] {
    let g = if detuned { GAINS_DETUNED } else { GAINS_EXACT };
    let m = lane_matrix(g);
    [(m[0][0] + m[1][0]) / 2.0, (m[0][1] + m[1][1]) / 2.0]
}

/// One-pole smoothing factor of the mix-stage gain and level states: `s = 0.01 t + 0.99 s`.
///
/// VERIFIED: the constants `0x3C23D70A` (0.01) and `0x3F7D70A3` (0.99) sit at DM `0x959AB..0x959AE`
/// and the states move exactly so per frame (oracle, one call with sentinel words); the steady
/// level is 0.999983 of the target (float rounding of the fixed point).
pub const SMOOTH_KEEP: f32 = 0.99;

/// Advances a mix-stage state one frame (`lane_mix::LaneMixer` applies the same law per engine sample) towards `target` (post-update value is the one used).
pub fn smooth(state: f32, target: f32) -> f32 {
    (1.0 - SMOOTH_KEEP) * target + SMOOTH_KEEP * state
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
        assert!((copy_weights(false)[0] - 1.0).abs() < 1e-6 && copy_weights(false)[1] == 0.0);
        assert_eq!(copy_weights(true), [0.5625, 0.5625]);
        assert!((f64::from(GAINS_DETUNED[0]) / f64::from(GAINS_EXACT[0]) - 0.75).abs() < 1e-6);
        assert!((f64::from(GAINS_DETUNED[2]) / f64::from(GAINS_EXACT[0]) - 0.375).abs() < 1e-6);
    }

    #[test]
    fn lane_matrix_reproduces_the_oracle_steady_state() {
        let m = lane_matrix(GAINS_DETUNED);
        assert_eq!(m, [[0.75, 0.375], [0.375, 0.75]]);
        let e = lane_matrix(GAINS_EXACT);
        assert!((e[0][0] - 1.0).abs() < 1e-6 && (e[1][0] - 1.0).abs() < 1e-6);
        assert_eq!((e[0][1], e[1][1]), (0.0, 0.0));
    }

    #[test]
    fn smoother_approaches_the_target_like_the_firmware_state() {
        let mut s = 0.0f32;
        for _ in 0..4000 {
            s = smooth(s, 1.0);
        }
        assert!((s - 1.0).abs() < 1e-4, "{s}");
        assert!((smooth(0.0, 1.0) - 0.01).abs() < 1e-6);
    }
}
