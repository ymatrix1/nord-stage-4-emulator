//! Firmware ANALOG SYNC wave 1 (Core2 stage `0x1226DD`) as an opt-in model.
//!
//! Spec: `docs/specs/synth-engine.md` section 4.3; evidence `docs/research/synth-stage-models-7.md`;
//! bit-exact Python model `tools/synth_stage_sync1.py` (320 of 320 oracle words), golden
//! `tools/synth_stage_golden7.py` -> `tests_fw_stage_golden7.rs`.
//!
//! The only SYNC stage built on the saw-family helpers: the parabola helper `0x1251D4` (twice, the
//! second call half a cycle later and on the already scaled `R6`, as in `fw_shape.rs`) runs on the
//! table-ratio phase `t h`, the difference is windowed by `0.5 - 0.5 sine(t)` (sine helper
//! `0x12522E`), scaled by the saw fade of `f h` and ends in the one-pole output high pass of
//! `fw_sine_hp.rs`. `t` is the phase in cycles, `i_sy` the increment in cycles per SY frame
//! (`f = 2 i_sy`, ASSUMED ratio word 1), `shape` the spread word `s5`.

use super::fw_shape::signed_phase;
use super::fw_sine_warps::sine_poly;
use super::fw_sync::sync_ratio;
use super::hw_saw::{SAW_A, SAW_B, SAW_C};

/// VERIFIED: `0x3FAAAA8F` of the helper `0x1251D4`, 4/3 to float32 (`fw_shape.rs`).
const FOUR_THIRDS: f32 = 1.333_333_4;

/// One helper sample: `xo^2 - w^3 k` with `w = 1 - min(1, |x| r7)`.
#[inline]
fn helper(p: f32, r7: f32, k: f32) -> f32 {
    let (xf, xo) = (signed_phase(p), signed_phase(p + 0.5));
    let w = 1.0 - (xf.abs() * r7).min(1.0);
    xo * xo - w * w * w * k
}

/// One sample of the stage before the output high pass (own oscillator, level 1).
#[inline]
pub fn sync1_core(t: f32, i_sy: f32, shape: f32) -> f32 {
    let f = (2.0 * i_sy).max(1e-9);
    let h = sync_ratio(shape);
    let fh = f * h;
    let g = SAW_C * (SAW_B + (SAW_A - fh).clamp(-SAW_B, SAW_B));
    let r7 = 0.5 / fh;
    let k1 = fh * FOUR_THIRDS;
    let p = t * h;
    let d = helper(p, r7, k1) - helper(p + 0.5, r7, k1 * FOUR_THIRDS);
    let sine = sine_poly(2.0 * signed_phase(t));
    (0.5 - 0.5 * sine) * d * g
}

#[cfg(test)]
mod tests {
    use super::super::tests_fw_stage_golden7 as gold;
    use super::*;

    #[test]
    fn core_matches_the_python_model_golden_grid() {
        let worst = (0..gold::SY1_OUT.len())
            .map(|i| {
                let (p, dt) = (
                    f32::from_bits(gold::SY1_PHASE[i]),
                    f32::from_bits(gold::SY1_DT[i]),
                );
                let got = sync1_core(p, dt, f32::from_bits(gold::SY1_SHAPE[i]));
                (got - f32::from_bits(gold::SY1_OUT[i])).abs()
            })
            .fold(0.0, f32::max);
        assert!(worst < 3e-5, "worst deviation {worst}");
    }

    #[test]
    fn helper_is_a_parabola_away_from_the_edge_and_dips_at_it() {
        // far from the edge (w = 0): xo^2, here xo = signed_phase(0.75) = -0.5
        assert!((helper(0.25, 4.0, 1.0) - 0.25).abs() < 1e-6);
        // at the edge (x = 0, w = 1): xo^2 = 1 minus k
        assert!((helper(0.0, 4.0, 0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn core_is_finite_bounded_and_deterministic_over_a_cycle() {
        for shape in [0.0f32, 0.25, 0.5, 1.0] {
            for dt in [0.0005f32, 0.005, 0.05] {
                for k in 0..256 {
                    let p = k as f32 / 256.0;
                    let y = sync1_core(p, dt, shape);
                    assert!(y.is_finite() && y.abs() < 40.0, "{shape} {dt} {k}: {y}");
                    assert_eq!(y.to_bits(), sync1_core(p, dt, shape).to_bits());
                }
            }
        }
    }
}
