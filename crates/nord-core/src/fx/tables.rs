//! EQ and delay parameter tables transcribed from the hardware display.

/// Bass shelf corner (Hz). ASSUMED: inherited from the old 1-pole code and `effects-and-routing.md`; no firmware or recording check.
pub(super) const EQ_BASS_HZ: f32 = 100.0;
/// Treble shelf corner (Hz). ASSUMED: same as bass corner, unverified.
pub(super) const EQ_TREBLE_HZ: f32 = 4000.0;
/// Shelf slope `Q` (Butterworth-like `1/sqrt(2)`). ASSUMED: textbook default; firmware shelf shape unknown.
pub(super) const EQ_SHELF_Q: f32 = std::f32::consts::FRAC_1_SQRT_2;
/// Mid peak `Q` index. ASSUMED: scale is VERIFIED (`firmware_q`), but index 64 (`Q~1.02`) is chosen to match the spec's `Q = 1.0`, not read from firmware.
pub(super) const EQ_MID_Q_IDX: usize = 64;

/// EQ gain per preset count (dB) used to decode and encode the signed 7-bit `eq_*` fields.
///
/// ASSUMED: `0.25 dB` (+-60 counts = +-15 dB, the factory range and the manual's +-15 dB). The step
/// of the DSP gain ladder is VERIFIED, see [`EQ_GAIN_DB_PER_COUNT_LADDER`]; that the preset count
/// indexes that ladder one to one is not (no ARM writer decoded, `docs/research/preset-dsp-map.md`
/// section 4.2). Switching to the ladder step is a one-line change here (TD-10): +-60 counts move
/// by 0.05 dB (15.00 to 15.05 dB), and tests that expect exact multiples of 0.25 dB change.
pub const EQ_GAIN_DB_PER_COUNT: f32 = 0.25;

/// Step of the DSP gain ladder `2^((j - 353) / 24)` in dB: `20 log10(2) / 24 = 0.250858`.
///
/// VERIFIED: `DM 0xB05EB` of the PNO and ORG images, all 354 low entries within 1.25 cents of the
/// closed form (`docs/research/preset-dsp-map.md` section 4.2); pinned here by
/// `eq_gain_step_constants_differ_by_a_third_of_a_percent`.
pub const EQ_GAIN_DB_PER_COUNT_LADDER: f32 = 6.020_6 / 24.0;

/// EQ Mid / Amp-filter frequency steps (`0..=127`, Hz).
///
/// EMPIRICAL: values as displayed by the hardware (OLED) and transcribed in third-party `ns4decode`
/// (`ns4decode.py::interpretEQfreq`, MIT, Randy 2024); precision is the display rounding (~1 to 5 %).
/// Not found as a numeric table in OS v1.66 `bank0_CODE.bin` (searched f32/i32/i16), so not firmware-verified.
pub const EQ_FREQ_HZ: [f32; 128] = [
    200.0, 205.0, 210.0, 215.0, 221.0, 226.0, 232.0, 238.0, 244.0, 250.0, 257.0, 263.0, 270.0,
    277.0, 284.0, 291.0, 299.0, 306.0, 314.0, 322.0, 330.0, 339.0, 347.0, 356.0, 365.0, 375.0,
    384.0, 394.0, 404.0, 414.0, 425.0, 436.0, 447.0, 458.0, 470.0, 482.0, 494.0, 507.0, 520.0,
    533.0, 546.0, 560.0, 575.0, 589.0, 604.0, 620.0, 635.0, 652.0, 668.0, 685.0, 703.0, 721.0,
    739.0, 758.0, 777.0, 797.0, 817.0, 838.0, 859.0, 881.0, 904.0, 927.0, 950.0, 975.0, 999.0,
    1000.0, 1000.0, 1100.0, 1100.0, 1200.0, 1200.0, 1300.0, 1300.0, 1400.0, 1400.0, 1500.0, 1500.0,
    1600.0, 1600.0, 1700.0, 1700.0, 1800.0, 1800.0, 1900.0, 1900.0, 2000.0, 2100.0, 2100.0, 2200.0,
    2300.0, 2400.0, 2400.0, 2500.0, 2600.0, 2700.0, 2800.0, 2900.0, 3000.0, 3100.0, 3200.0, 3300.0,
    3400.0, 3500.0, 3600.0, 3700.0, 3900.0, 4000.0, 4100.0, 4300.0, 4400.0, 4600.0, 4700.0, 4900.0,
    5000.0, 5200.0, 5400.0, 5600.0, 5800.0, 5900.0, 6100.0, 6300.0, 6600.0, 6800.0, 7000.0, 7200.0,
    7500.0, 7700.0, 8000.0,
];

/// Maps a normalized `0.0..=1.0` knob/morph position to the nearest [`EQ_FREQ_HZ`] step.
#[inline]
pub fn eq_freq_hz_from_norm(norm: f32) -> f32 {
    let idx = (norm.clamp(0.0, 1.0) * 127.0).round() as usize;
    EQ_FREQ_HZ[idx.min(127)]
}

/// Delay time steps (`0..=127`, ms) of the `Delay Time` knob when not locked to the master clock.
///
/// EMPIRICAL: hardware display values transcribed in third-party `ns4decode`
/// (`ns4decode.py::interpretDelayTempo`, MIT, Randy 2024; hand-crafted curve, ~1 % display rounding).
/// The index decode (top 7 bits of the 14-bit field at block bit 242) matches `ns4decode` on all 1,920
/// FX blocks of the 384 factory programs. Not found as a numeric table in `bank0_CODE.bin`.
pub const DELAY_TIME_MS: [f32; 128] = [
    1500.0, 1420.0, 1360.0, 1300.0, 1250.0, 1200.0, 1150.0, 1110.0, 1070.0, 1030.0, 1000.0, 952.0,
    909.0, 870.0, 833.0, 790.0, 750.0, 732.0, 714.0, 698.0, 682.0, 667.0, 652.0, 638.0, 625.0,
    612.0, 600.0, 588.0, 577.0, 566.0, 556.0, 546.0, 541.0, 536.0, 531.0, 526.0, 522.0, 517.0,
    513.0, 508.0, 504.0, 500.0, 496.0, 492.0, 488.0, 484.0, 480.0, 476.0, 472.0, 469.0, 465.0,
    462.0, 458.0, 455.0, 451.0, 448.0, 444.0, 441.0, 438.0, 435.0, 432.0, 428.0, 423.0, 417.0,
    411.0, 405.0, 400.0, 395.0, 390.0, 385.0, 380.0, 375.0, 366.0, 357.0, 349.0, 341.0, 333.0,
    326.0, 319.0, 313.0, 306.0, 300.0, 289.0, 278.0, 268.0, 259.0, 250.0, 238.0, 227.0, 217.0,
    198.0, 188.0, 179.0, 170.0, 163.0, 156.0, 150.0, 144.0, 139.0, 134.0, 129.0, 125.0, 119.0,
    114.0, 109.0, 104.0, 99.0, 94.0, 83.0, 75.0, 68.0, 63.0, 58.0, 54.0, 50.0, 47.0, 44.0, 42.0,
    40.0, 37.0, 34.0, 31.0, 30.0, 28.0, 26.0, 24.0, 22.0, 20.0,
];

/// Beats (quarter note = 1) for the 17 `Delay Time` steps when locked to the master clock:
/// `1/2, 1/4D, 1/2T, 1/4S, 1/4, 1/8D, 1/4T, 1/8S, 1/8, 1/16D, 1/8T, 1/16S, 1/16, 1/16T, 1/32, 1/32T, 1/64`.
///
/// EMPIRICAL (labels and order): `ns4decode` `DelayTempoFracs`. VERIFIED (meaning): Nord Stage 4 User Manual
/// p.51 lists straight, swing (`S`), triplet (`T`) and dotted (`D`) options from 1/2 to 1/32. `D` = x1.5 and
/// `T` = x2/3 are exact; the swing ratio is not documented: the x8/7 used for `S` is an ASSUMED placeholder
/// chosen only so the entry sorts between its neighbours.
pub const DELAY_SYNC_BEATS: [f32; 17] = [
    2.0,
    1.5,
    4.0 / 3.0,
    8.0 / 7.0,
    1.0,
    0.75,
    2.0 / 3.0,
    4.0 / 7.0,
    0.5,
    0.375,
    1.0 / 3.0,
    2.0 / 7.0,
    0.25,
    1.0 / 6.0,
    0.125,
    1.0 / 12.0,
    0.0625,
];

/// Delay time in ms for a hardware step index (`0..=127`, clamped).
#[inline]
pub fn delay_time_ms_from_index(idx: usize) -> f32 {
    DELAY_TIME_MS[idx.min(127)]
}

/// Beats for a hardware step index (`0..=127`) when the delay is locked to the master clock.
#[inline]
pub fn delay_sync_beats_from_index(idx: usize) -> f32 {
    DELAY_SYNC_BEATS[(idx.min(127) * 17 / 128).min(16)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eq_gain_step_constants_differ_by_a_third_of_a_percent() {
        let exact = 20.0 * 2f64.log10() / 24.0;
        assert!((f64::from(EQ_GAIN_DB_PER_COUNT_LADDER) - exact).abs() < 1e-6);
        let rel = (EQ_GAIN_DB_PER_COUNT_LADDER / EQ_GAIN_DB_PER_COUNT - 1.0).abs();
        assert!(
            (rel - 0.0034).abs() < 1e-4,
            "relative step difference {rel}"
        );
        // 60 counts: 15.00 dB (assumed step) against 15.05 dB (ladder step)
        let gap = 60.0 * (EQ_GAIN_DB_PER_COUNT_LADDER - EQ_GAIN_DB_PER_COUNT);
        assert!((gap - 0.05).abs() < 0.005, "gap at +-15 dB {gap}");
    }
    use crate::fx::{DelaySubdivision, LayerFxChain};

    #[test]
    fn test_eq_freq_table_hardware_steps() {
        assert_eq!(EQ_FREQ_HZ[0], 200.0);
        assert_eq!(EQ_FREQ_HZ[64], 999.0);
        assert_eq!(EQ_FREQ_HZ[127], 8000.0);
        assert!(EQ_FREQ_HZ.windows(2).all(|w| w[1] >= w[0]));
        assert_eq!(eq_freq_hz_from_norm(0.0), 200.0);
        assert_eq!(eq_freq_hz_from_norm(64.0 / 127.0), 999.0);
        assert_eq!(eq_freq_hz_from_norm(2.0), 8000.0);
    }

    #[test]
    fn test_delay_time_table_and_sync_fractions() {
        assert_eq!(DELAY_TIME_MS[0], 1500.0);
        assert_eq!(DELAY_TIME_MS[127], 20.0);
        assert!(DELAY_TIME_MS.windows(2).all(|w| w[1] <= w[0]));
        assert_eq!(delay_time_ms_from_index(500), 20.0);
        assert_eq!(delay_sync_beats_from_index(0), 2.0);
        assert_eq!(delay_sync_beats_from_index(127), 0.0625);
        assert!(DELAY_SYNC_BEATS.windows(2).all(|w| w[1] < w[0]));
        // 120 BPM, 1/8D = 0.375 beats = 187.5 ms
        let mut fx = LayerFxChain::new(48_000.0);
        fx.delay_clock_sync = true;
        fx.delay_sync_beats = Some(0.375);
        let samples = fx.target_delay_samples(120.0);
        assert!((samples - 0.1875 * 48_000.0).abs() < 1.0, "{samples}");
        fx.delay_sync_beats = None;
        fx.delay_subdivision = DelaySubdivision::Quarter;
        assert!((fx.target_delay_samples(120.0) - 0.5 * 48_000.0).abs() < 1.0);
    }
}
