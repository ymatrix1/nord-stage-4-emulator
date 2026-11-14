//! Firmware ANALOG MISC stages (Core2 `0x122030`, `0x122151`, `0x122417`) as opt-in models.
//!
//! Spec: `docs/specs/synth-engine.md` section 4.3; evidence `docs/research/synth-stage-models-8.md`;
//! bit-exact Python models `tools/synth_stage_misc.py` (320 of 320 oracle words each), goldens
//! `tools/synth_stage_golden8.py` -> `tests_fw_stage_golden8.rs`.
//!
//! * MISC 0 (`0x122030`): Voss-McCartney pink noise (ten rows, frame counter) plus one white
//!   value, scaled by 0.2, then the output high pass.
//! * MISC 1 (`0x122151`): the LCG noise through a one-pole low pass (`0.0005`), scaled by 64.
//! * MISC 2 (`0x122417`): a sine whose level is a cubed, phase-modulated sine at `v/2` times the
//!   carrier increment (`v` = SYNC table at the spread word), then the output high pass.

use super::fw_shape::signed_phase;
use super::fw_sine_warps::sine_poly;
use super::fw_stages::{fade, noise_sample, noise_step};
use super::fw_sync::sync_ratio;

/// Rows of the pink noise generator.
pub const PINK_ROWS: usize = 10;
/// VERIFIED (listing `0x2441CE`, `0x244546`, `0x2444EC`; oracle 320 of 320): pink scale 0.2,
/// low-pass coefficient 0.0005 and gain 64.
const PINK_SCALE: f32 = f32::from_bits(0x3E4C_CCCD);
const LP_COEF: f32 = f32::from_bits(0x3A03_126F);
const LP_GAIN: f32 = 64.0;
const TWO_POW_M31: f32 = 1.0 / 2_147_483_648.0;

/// The integer state as the register float `s 2^-31` (not truncated to a memory word).
#[inline]
fn exact(state: u32) -> f32 {
    state as i32 as f32 * TWO_POW_M31
}

/// One sample of MISC 0 before the output high pass: counter step, row update (a row takes the
/// new white value when `counter & mask == 0`, the counter then becomes the mask), row sum plus a
/// second white value, times 0.2.
#[inline]
pub fn pink_sample(rows: &mut [f32; PINK_ROWS], counter: &mut u32, noise: &mut u32) -> f32 {
    *counter = counter.wrapping_add(1);
    *noise = noise_step(*noise);
    let v = noise_sample(*noise);
    let (mut c, mut mask) = (*counter, 0x3FFu32);
    for row in rows.iter_mut() {
        if c & mask == 0 {
            *row = v;
            c = mask;
        }
        mask >>= 1;
    }
    let mut acc = 0.0f32;
    for r in rows.iter() {
        acc += r;
    }
    *noise = noise_step(*noise);
    PINK_SCALE * (acc + exact(*noise))
}

/// One sample of MISC 1: `z += 0.0005 (v - z)`, output `64 z`.
#[inline]
pub fn lowpass_sample(z: &mut f32, noise: &mut u32) -> f32 {
    *noise = noise_step(*noise);
    *z += LP_COEF * (exact(*noise) - *z);
    LP_GAIN * *z
}

/// Increment ratio of the MISC 2 modulator to the carrier, `v / 2` of the spread word, limited so
/// that the modulator increment `a v / 2` stays at or below half a cycle per SY frame
/// (`a = 2 i_sy`).
#[inline]
pub fn fm_ratio(i_sy: f32, shape: f32) -> f32 {
    let half_v = 0.5 * sync_ratio(shape);
    half_v.min(0.5 / (2.0 * i_sy).max(1e-9))
}

/// One sample of MISC 2 before the output high pass: carrier phase `t`, modulator phase `t2`
/// (cycles), `i_sy` the increment in cycles per SY frame (`a = 2 i_sy`), `shape` the spread word.
#[inline]
pub fn fm_core(t: f32, t2: f32, i_sy: f32, shape: f32) -> f32 {
    let a = 2.0 * i_sy;
    let g1 = fade(a);
    let u = a * 0.5 * sync_ratio(shape);
    let g2 = fade(u);
    let s = 0.5 * sine_poly(2.0 * signed_phase(t2)) + 0.5;
    let w = s * s * s * g2 + (0.25 - 0.25 * g2);
    sine_poly(2.0 * signed_phase(t)) * w * (1.5 * g1)
}

#[cfg(test)]
mod tests {
    use super::super::tests_fw_stage_golden8 as gold;
    use super::super::{fw_sine_hp::OutputHighPass, fw_stages::NOISE_SEED};
    use super::*;

    #[test]
    fn pink_sequence_matches_the_python_model() {
        let (mut rows, mut counter, mut noise) = ([0.0f32; PINK_ROWS], 0u32, NOISE_SEED);
        let mut hp = OutputHighPass::default();
        let worst = (0..gold::PK_OUT.len())
            .map(|i| {
                let y = hp.process(pink_sample(&mut rows, &mut counter, &mut noise));
                (y - f32::from_bits(gold::PK_OUT[i])).abs()
            })
            .fold(0.0, f32::max);
        assert!(worst < 2e-5, "worst deviation {worst}");
    }

    #[test]
    fn lowpass_sequence_matches_the_python_model() {
        let (mut z, mut noise) = (0.0f32, NOISE_SEED);
        let worst = (0..gold::LP_OUT.len())
            .map(|i| {
                let y = lowpass_sample(&mut z, &mut noise);
                (y - f32::from_bits(gold::LP_OUT[i])).abs()
            })
            .fold(0.0, f32::max);
        assert!(worst < 2e-5, "worst deviation {worst}");
    }

    #[test]
    fn fm_core_matches_the_python_model_golden_grid() {
        let worst = (0..gold::FM_OUT.len())
            .map(|i| {
                let (t, t2, dt) = (
                    f32::from_bits(gold::FM_PHASE[i]),
                    f32::from_bits(gold::FM_PHASE2[i]),
                    f32::from_bits(gold::FM_DT[i]),
                );
                let got = fm_core(t, t2, dt, f32::from_bits(gold::FM_SHAPE[i]));
                (got - f32::from_bits(gold::FM_OUT[i])).abs()
            })
            .fold(0.0, f32::max);
        assert!(worst < 3e-5, "worst deviation {worst}");
    }

    #[test]
    fn pink_rows_refresh_at_the_documented_counter_values() {
        // counter 1 -> 2: the fastest row (mask 1, index 9) takes the new value
        let (mut rows, mut counter, mut noise) = ([9.0f32; PINK_ROWS], 1u32, 7u32);
        pink_sample(&mut rows, &mut counter, &mut noise);
        assert_eq!(counter, 2);
        assert_ne!(rows[9], 9.0, "the fastest row takes the new value");
        assert_eq!(rows[0], 9.0, "the slowest row keeps its value");
    }

    #[test]
    fn fm_ratio_is_limited_and_core_is_finite() {
        assert!(fm_ratio(0.001, 0.5) > 0.0);
        let hi = fm_ratio(0.2, 1.0);
        assert!(
            hi * 2.0 * 0.2 <= 0.5 + 1e-6,
            "modulator increment limited: {hi}"
        );
        for shape in [0.0f32, 0.5, 1.0] {
            for k in 0..64 {
                let p = k as f32 / 64.0;
                let y = fm_core(p, 0.3 * p, 0.003, shape);
                assert!(y.is_finite() && y.abs() < 4.0, "{shape} {k}: {y}");
            }
        }
    }

    #[test]
    fn lowpass_output_stays_bounded() {
        let (mut z, mut noise) = (0.0f32, 5u32);
        let mut peak = 0.0f32;
        for _ in 0..5000 {
            peak = peak.max(lowpass_sample(&mut z, &mut noise).abs());
        }
        assert!(peak < 64.0 * 0.5, "peak {peak}");
    }
}
