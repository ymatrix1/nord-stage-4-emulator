//! Firmware ANALOG SHAPE SINE wave 3 (Core2 stage `0x12377D`) as an opt-in model.
//!
//! Spec: `docs/specs/synth-engine.md` section 4.3; evidence `docs/research/synth-stage-models-5.md`;
//! bit-exact Python model `tools/synth_stage_shape_sine3.py` (320 of 320 oracle words), golden
//! `tools/synth_stage_golden5.py` -> `tests_fw_stage_golden5.rs`.
//!
//! The stage adds three terms: the wave 0 warp fed to the sine polynomial (offset by a quarter
//! turn, plus `(2/pi) vr`), and two band-limited cubes `(1 - min(1, |x| r7))^3 K` at the phase
//! plus half a cycle and the phase plus `vr / 2` cycles, with `vr = (1 - (1 - s5)^2)(1 - min(1,
//! 5.5 a))`. There is no output high pass. `t` is the phase in cycles, `i_sy` the increment in
//! cycles per SY frame (`a = f = 2 i_sy`), `shape` the spread word `s5` (idle 0.5, ASSUMED to
//! follow Osc Ctrl).

use super::fw_shape::signed_phase;
use super::fw_sine_warps::sine_poly;
use super::hw_saw::{SAW_A, SAW_B, SAW_C};

/// VERIFIED (listing `0x246F66`): the width closes as `1 - min(1, 5.5 f)`.
const WIDTH_FADE: f32 = 5.5;
/// VERIFIED (listing `0x246F80`): `2 / pi` of the sine offset `c`.
const TWO_OVER_PI: f32 = f32::from_bits(0x3F22_F983);
/// VERIFIED (listing `0x246FE0`): `pi` as loaded here (`0x40490FDB`).
const PI: f32 = f32::from_bits(0x4049_0FDB);
/// VERIFIED (listing `0x247136`): `2/3` as loaded (`0x3F2AAA3B`).
const TWO_THIRDS: f32 = f32::from_bits(0x3F2A_AA3B);

/// One band-limited cube term for the phase `p` (cycles).
#[inline]
fn cube(p: f32, r7: f32, k: f32) -> f32 {
    let w = 1.0 - (signed_phase(p).abs() * r7).min(1.0);
    w * w * w * k
}

/// One sample of the stage (own oscillator, level 1, gain included).
#[inline]
pub fn shape_sine3_core(t: f32, i_sy: f32, shape: f32) -> f32 {
    let f = (2.0 * i_sy).max(1e-9);
    let g = SAW_C * (SAW_B + (SAW_A - f).clamp(-SAW_B, SAW_B));
    let s = 1.0 - shape;
    let vr = (1.0 - s * s) * (1.0 - (f * WIDTH_FADE).min(1.0));
    let (ra, rb) = (1.0 / (1.0 - vr).max(1e-3), 1.0 / (1.0 + vr));
    let k = PI * (ra - rb) * TWO_THIRDS * f;
    let r7 = 0.5 / f;
    // wave 0 warp, then the word `2 y 2^30 + 2^30` (wraps at +-2)
    let x = signed_phase(t);
    let hi = (x + 1.0) * ra;
    let y = if hi >= 1.0 { (x - 1.0) * rb } else { hi };
    let mut v = 2.0 * y + 1.0;
    if v >= 2.0 {
        v -= 4.0;
    }
    let z = sine_poly(v) + TWO_OVER_PI * vr;
    (cube(t + 0.5, r7, k) + z + cube(t + 0.5 * vr, r7, k)) * g
}

#[cfg(test)]
mod tests {
    use super::super::tests_fw_stage_golden5 as gold;
    use super::*;

    #[test]
    fn core_matches_the_python_model_golden_grid() {
        let worst = (0..gold::SS3_OUT.len())
            .map(|i| {
                let (p, dt) = (
                    f32::from_bits(gold::SS3_PHASE[i]),
                    f32::from_bits(gold::SS3_DT[i]),
                );
                let got = shape_sine3_core(p, dt, f32::from_bits(gold::SS3_SHAPE[i]));
                (got - f32::from_bits(gold::SS3_OUT[i])).abs()
            })
            .fold(0.0, f32::max);
        assert!(worst < 2e-5, "worst deviation {worst}");
    }

    #[test]
    fn core_is_finite_bounded_and_deterministic_over_a_cycle() {
        for shape in [0.0f32, 0.25, 0.5, 1.0] {
            for dt in [0.0005f32, 0.005, 0.05] {
                for k in 0..256 {
                    let p = k as f32 / 256.0;
                    let y = shape_sine3_core(p, dt, shape);
                    assert!(y.is_finite() && y.abs() < 40.0, "{shape} {dt} {k}: {y}");
                    assert_eq!(y.to_bits(), shape_sine3_core(p, dt, shape).to_bits());
                }
            }
        }
    }
}
