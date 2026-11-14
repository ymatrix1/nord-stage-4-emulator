//! Firmware ANALOG SHAPE SINE oscillators (Core2 stages `0x123427`, `0x1235C9`, `0x1232A5`,
//! `0x122F1F`) as opt-in models.
//!
//! Spec: `docs/specs/synth-engine.md` section 4.3; evidence `docs/research/synth-stage-models-2.md`
//! and `synth-stage-models-3.md`; bit-exact Python models `tools/synth_stage_shape_sine.py`
//! (320 of 320 oracle words each), goldens `tools/synth_stage_golden.py` (parts 2 and 3).
//!
//! All four feed a warped phase to the sine polynomial of the helper `0x12522E` and scale by the
//! sine fade of the increment; they differ in the warp. `t` is the phase in cycles, `i_sy` the
//! increment in cycles per SY frame (fade and width laws, `a = 2 i_sy`), `shape` the spread word
//! `s5` (idle 0.5; its knob scale is open, ASSUMED to follow Osc Ctrl).

use super::fw_shape::signed_phase;

/// VERIFIED (image words of the sine helper `0x12522E`): `t (c12 + q (c14 + q (c16 + q c18)))`.
const SINE_C12: f32 = 1.570_790_8;
const SINE_C14: f32 = -0.645_891_1;
const SINE_C16: f32 = 0.079_430_89;
const SINE_C18: f32 = -0.004_331_1;
/// VERIFIED (listing `0x2468AE..0x2468DC`): the warp width closes as `1 - min(1, 5.5 a)`.
const WIDTH_FADE: f32 = 5.5;
/// VERIFIED: fade of the sine stages `C (B + clip(A - a, +-B))` (`0x3F30F34B`, `0x3E6BEF0F`,
/// `0x400AE2EB`).
const FADE_A: f32 = 0.691_212_36;
const FADE_B: f32 = 0.230_404_12;
const FADE_C: f32 = 2.170_1;
/// VERIFIED (listing `0x2465AA..0x246602`): gain ceiling `1 + 15 s5^2` and numerator `0.1818 / a`.
const GAIN_CEIL: f32 = 15.0;
const GAIN_NUM: f32 = 0.181_818;
/// VERIFIED (listing `0x245EA6..0x245EDE`): resonance `k = max(0.98643 - 4 a, 0) s5`, level
/// `1 - 0.85 k`.
const RES_TOP: f32 = 0.987;
const RES_DAMP: f32 = 0.85;

/// The sine polynomial of the helper `0x12522E` on `v = 2 y` (`y` = warped phase, -1..1).
#[inline]
pub(super) fn sine_poly(v: f32) -> f32 {
    let t = v.abs() - 1.0;
    let q = t * t;
    t * (SINE_C12 + q * (SINE_C14 + q * (SINE_C16 + q * SINE_C18)))
}

/// Sine fade `g` of the increment `a = 2 i_sy`.
#[inline]
fn fade(a: f32) -> f32 {
    FADE_C * (FADE_B + (FADE_A - a).clamp(-FADE_B, FADE_B))
}

/// Width `w = (1 - (1 - s5)^2)(1 - min(1, 5.5 a))` of waves 0 and 1.
#[inline]
fn width(a: f32, shape: f32) -> f32 {
    let s = 1.0 - shape;
    (1.0 - s * s) * (1.0 - (a * WIDTH_FADE).min(1.0))
}

/// SHAPE SINE wave 0: two slopes around the knee `-w`.
#[inline]
pub fn shape_sine_wave(t: f32, i_sy: f32, shape: f32) -> f32 {
    let a = 2.0 * i_sy;
    let w = width(a, shape);
    let x = signed_phase(t);
    let hi = (x + 1.0) / (1.0 - w).max(1e-3);
    let y = if hi >= 1.0 { (x - 1.0) / (1.0 + w) } else { hi };
    sine_poly(2.0 * y) * fade(a)
}

/// SHAPE SINE wave 1: the half-cycle magnitude stretched by `1 / (1 - w)` and clipped at 1.
#[inline]
pub fn shape_sine_fold(t: f32, i_sy: f32, shape: f32) -> f32 {
    let a = 2.0 * i_sy;
    let rho = 1.0 / (1.0 - width(a, shape)).max(1e-3);
    let upper = t >= 0.5;
    let x = 2.0 * (if upper { t - 0.5 } else { t });
    let y = (x * rho).min(1.0);
    // word `y 2^31` (lower half) or `y 2^31 - 2^31` (upper half), `v = word 2^-30`
    let v = if upper { 2.0 * y - 2.0 } else { 2.0 * y };
    sine_poly(v) * fade(a)
}

/// SHAPE SINE wave 2: the phase amplified by `rho` (clipped at 1), minus `1 - 1 / rho`, level `-g`.
#[inline]
pub fn shape_sine_gain(t: f32, i_sy: f32, shape: f32) -> f32 {
    let a = 2.0 * i_sy;
    let ceil = 1.0 + GAIN_CEIL * shape * shape;
    let rho = (GAIN_NUM / a.max(1e-6)).clamp(1.0, ceil.max(1.0));
    let y = (signed_phase(t) * rho).clamp(-1.0, 1.0);
    -(sine_poly(2.0 * y) - (1.0 - 1.0 / rho)) * fade(a)
}

/// SHAPE SINE wave 5: `cos / (1 + k^2 + 2 k sin)` of the phase, level `g (1 - 0.85 k)`.
#[inline]
pub fn shape_sine_res(t: f32, i_sy: f32, shape: f32) -> f32 {
    let a = 2.0 * i_sy;
    let k = (RES_TOP - 4.0 * a).max(0.0) * shape;
    let s = sine_poly(2.0 * signed_phase(t));
    let c = sine_poly(2.0 * signed_phase(t + 0.25));
    c / (1.0 + k * k + 2.0 * k * s) * fade(a) * (1.0 - RES_DAMP * k)
}

#[cfg(test)]
mod tests {
    use super::super::tests_fw_stage_golden2 as gold2;
    use super::super::tests_fw_stage_golden3 as gold3;
    use super::*;

    fn worst(n: usize, f: impl Fn(usize) -> (f32, f32)) -> f32 {
        (0..n)
            .map(f)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max)
    }

    #[test]
    fn wave_0_matches_the_python_model_golden_grid() {
        let w = worst(gold2::SSINE_OUT.len(), |i| {
            let (p, dt) = (
                f32::from_bits(gold2::SSINE_PHASE[i]),
                f32::from_bits(gold2::SSINE_DT[i]),
            );
            let got = shape_sine_wave(p, dt, f32::from_bits(gold2::SSINE_SHAPE[i]));
            (got, f32::from_bits(gold2::SSINE_OUT[i]))
        });
        assert!(w < 2e-6, "worst deviation {w}");
    }

    #[test]
    fn waves_1_2_5_match_the_python_model_golden_grids() {
        let w1 = worst(gold3::SSF_OUT.len(), |i| {
            let (p, dt) = (
                f32::from_bits(gold3::SSF_PHASE[i]),
                f32::from_bits(gold3::SSF_DT[i]),
            );
            let got = shape_sine_fold(p, dt, f32::from_bits(gold3::SSF_SHAPE[i]));
            (got, f32::from_bits(gold3::SSF_OUT[i]))
        });
        let w2 = worst(gold3::SSG_OUT.len(), |i| {
            let (p, dt) = (
                f32::from_bits(gold3::SSG_PHASE[i]),
                f32::from_bits(gold3::SSG_DT[i]),
            );
            let got = shape_sine_gain(p, dt, f32::from_bits(gold3::SSG_SHAPE[i]));
            (got, f32::from_bits(gold3::SSG_OUT[i]))
        });
        let w5 = worst(gold3::SSR_OUT.len(), |i| {
            let (p, dt) = (
                f32::from_bits(gold3::SSR_PHASE[i]),
                f32::from_bits(gold3::SSR_DT[i]),
            );
            let got = shape_sine_res(p, dt, f32::from_bits(gold3::SSR_SHAPE[i]));
            (got, f32::from_bits(gold3::SSR_OUT[i]))
        });
        assert!(w1 < 2e-6 && w2 < 2e-6 && w5 < 2e-5, "{w1} {w2} {w5}");
    }

    #[test]
    fn all_warps_are_finite_and_bounded_over_a_cycle() {
        for shape in [0.0f32, 0.5, 1.0] {
            for k in 0..512 {
                let t = k as f32 / 512.0;
                for f in [
                    shape_sine_wave,
                    shape_sine_fold,
                    shape_sine_gain,
                    shape_sine_res,
                ] {
                    let y = f(t, 0.005, shape);
                    assert!(y.is_finite() && y.abs() < 40.0, "shape {shape} t {t}: {y}");
                }
            }
        }
    }
}
