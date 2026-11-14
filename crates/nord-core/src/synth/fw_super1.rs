//! Firmware ANALOG SUPER wave 1 (Core2 stage `0x121E9D`) as an opt-in model.
//!
//! Spec: `docs/specs/synth-engine.md` section 4.3; evidence `docs/research/synth-stage-models-5.md`;
//! bit-exact Python model `tools/synth_stage_super1.py` (320 of 320 oracle words), golden
//! `tools/synth_stage_golden5.py` -> `tests_fw_stage_golden5.rs`.
//!
//! Seven detuned voices (`LCNTR = 7`) like `0x121D6D`, but each voice is a band-limited square
//! instead of a saw: with `d = |x| - 0.5` (`x` the signed phase, -1..1), `w = 1 - min(1, |d| r7)`
//! and `sg = sign(d)`, the sample is `sg - copysign(w^2, sg) = sg (1 - w^2)`.

use super::fw_shape::signed_phase;
use super::fw_stages::{fade, spread_u, SUPER_RATIOS, SUPER_SPREAD_MAX};

/// VERIFIED (listing `0x243E52`, `LCNTR = 7`): voices of the loop.
pub const SUPER1_VOICES: usize = 7;

/// The inline square of the loop for the phase `p` (cycles) and increment `dt` (cycles per
/// sample); `r7 = 0.5 / f` with the pitch word `f = 2 dt`.
#[inline]
pub fn fold_diff(p: f32, dt: f32) -> f32 {
    let d = signed_phase(p).abs() - 0.5;
    let w = 1.0 - (d.abs() * 0.25 / dt.max(1e-6)).min(1.0);
    let sg = if d >= 0.0 { 1.0 } else { -1.0 };
    sg - (w * w).copysign(sg)
}

/// One sample of the stage for one lane: `phases` are the voice phases (cycles, advanced here).
#[inline]
pub fn super1_sample(phases: &mut [f32; 8], dt: f32, i_sy: f32, ctrl: f32) -> f32 {
    let u = spread_u(SUPER_SPREAD_MAX * ctrl);
    let g = fade(2.0 * i_sy);
    let mut sum = 0.0;
    for (ph, rho) in phases.iter_mut().zip(SUPER_RATIOS).take(SUPER1_VOICES) {
        let dt_j = dt * (rho * u + 1.0 - u);
        sum += fold_diff(*ph, dt_j);
        *ph = (*ph + dt_j) % 1.0;
    }
    // ASSUMED: equal voice weights summing to one (the P-block weights are open, OQ22)
    sum * g / SUPER1_VOICES as f32
}

#[cfg(test)]
mod tests {
    use super::super::tests_fw_stage_golden5 as gold;
    use super::*;

    #[test]
    fn fold_matches_the_python_model_golden_grid() {
        let mut worst = 0.0f32;
        for i in 0..gold::FOLD_OUT.len() {
            let (p, dt) = (
                f32::from_bits(gold::FOLD_PHASE[i]),
                f32::from_bits(gold::FOLD_DT[i]),
            );
            let want = f32::from_bits(gold::FOLD_OUT[i]);
            worst = worst.max((fold_diff(p, dt) - want).abs());
        }
        assert!(worst < 1e-5, "worst deviation {worst}");
    }

    #[test]
    fn fold_is_a_signed_unit_square_away_from_the_edges() {
        assert_eq!(fold_diff(0.05, 0.001), -1.0);
        assert_eq!(fold_diff(0.4, 0.001), 1.0);
        assert_eq!(fold_diff(0.6, 0.001), 1.0);
        assert_eq!(fold_diff(0.95, 0.001), -1.0);
    }

    #[test]
    fn seven_voices_advance_and_the_eighth_phase_stays() {
        let mut ph = [0.1f32; 8];
        let y = super1_sample(&mut ph, 0.005, 0.005, 0.5);
        assert!(y.is_finite() && y.abs() < 2.0);
        assert!(ph[..7].iter().all(|&p| p != 0.1));
        assert_eq!(ph[7], 0.1);
    }
}
