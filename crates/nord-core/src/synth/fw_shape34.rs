//! Firmware ANALOG SHAPE waves 3 and 4 (Core2 stages `0x120F55`, `0x12145E`) as opt-in models.
//!
//! Spec: `docs/specs/synth-engine.md` section 4.3; evidence `docs/research/synth-stage-models-11.md`;
//! bit-exact Python models `tools/synth_stage_shape3.py`, `tools/synth_stage_shape4.py` (320 of 320
//! oracle words each), goldens `tools/synth_stage_golden9.py` -> `tests_fw_stage_golden9.rs`.
//!
//! * SHAPE 4 (`0x12145E`): the sum of two band-limited parabolas, the second shifted by `s / 2`
//!   cycles (`s` the spread word), plus the bias `(2/3)(a^2 - 1)`, scaled by `2 g / D` with
//!   `D = 1 + (1 - s)^2 - s^2 / 2`. (Wave 0 subtracts the second parabola.)
//! * SHAPE 3 (`0x120F55`): a cosine bump of slope `w` around phase 0 (`-cos(pi x / 2)`,
//!   `x = clip(w p, 1)`, polynomial of the helper `0x12522E`) plus `2 / (pi w)`, with a cubic
//!   band-limiting term at the phase shifted by `+-1 / (2 w)` cycles, scaled by `-2 g`.
//!   `w = min(max(0.1815 / a, 2), 14 s^2 + 2)`.
//!
//! `a = f = 2 i_sy` is the pitch word at the SY rate; the band limit of SHAPE 4 follows the render
//! increment `dt` as in `fw_shape.rs`, that of SHAPE 3 the SY rate as in `fw_sine3.rs`.

use super::fw_shape::{parabola, signed_phase, FOUR_THIRDS};
use super::fw_stages::fade;

/// VERIFIED (listing `0x2429B6`, oracle 320 of 320): `0.66666` of the SHAPE 4 bias.
const BIAS_K: f32 = f32::from_bits(0x3F2A_AAAA);
/// VERIFIED (listing `0x241F20`): `K0` of `w = K0 / a`.
const WIDTH_K0: f32 = f32::from_bits(0x3E3A_2E7F);
/// VERIFIED (listing `0x241F86`, `0x241F92`, `0x2420B8`): `2 / pi`, `-pi / 4`, `0.66666`.
const TWO_OVER_PI: f32 = f32::from_bits(0x3F22_F983);
const NEG_PI_4: f32 = f32::from_bits(0xBF49_0FDB);
const BUMP_K: f32 = f32::from_bits(0x3F2A_AA3B);
/// VERIFIED (image words read by the helper `0x12522E`, `DM(0xC1E0E .. 0xC1E14)`): the odd
/// polynomial `c12 u + c14 u^3 + c16 u^5 + c18 u^7` of `sin(pi u / 2)` with `u = |x| - 1`.
const C12: f32 = f32::from_bits(0x3FC9_0FAC);
const C14: f32 = f32::from_bits(0xBF25_591E);
const C16: f32 = f32::from_bits(0x3DA2_ACAA);
const C18: f32 = f32::from_bits(0xBB8D_EBE6);

/// One sample of SHAPE 4 (own oscillator, level 1, gain included): phase `t` (cycles), render
/// increment `dt`, SY increment `i_sy` (`a = 2 i_sy`) and the spread word `shape`.
#[inline]
pub fn shape4_core(t: f32, dt: f32, i_sy: f32, shape: f32) -> f32 {
    let dt = dt.max(1e-6);
    let a = 2.0 * i_sy;
    let u = 1.0 - shape;
    let d = (1.0 + u * u - 0.5 * shape * shape).max(1e-3);
    let k = FOUR_THIRDS * 2.0 * dt;
    let bias = BIAS_K * (a * a - 1.0);
    (parabola(t, dt, k) + parabola(t - 0.5 * shape, dt, k) + bias) * fade(a) * 2.0 / d
}

/// The cosine helper `0x12522E` for `x` in `-1..1`: `u (c12 + q (c14 + q (c16 + c18 q)))`.
#[inline]
fn cos_helper(x: f32) -> f32 {
    let u = x.abs() - 1.0;
    let q = u * u;
    u * (C12 + q * (C14 + q * (C16 + C18 * q)))
}

/// One sample of SHAPE 3 (own oscillator, level 1, gain included): phase `t` (cycles), SY
/// increment `i_sy` (`f = 2 i_sy`) and the spread word `shape`.
#[inline]
pub fn shape3_core(t: f32, i_sy: f32, shape: f32) -> f32 {
    let f = (2.0 * i_sy).max(1e-9);
    let limit = 14.0 * shape * shape + 2.0;
    let w = (WIDTH_K0 / f).max(2.0).min(limit);
    let x = (w * signed_phase(t)).clamp(-1.0, 1.0);
    let body = cos_helper(x) + TWO_OVER_PI / w;
    let k6 = NEG_PI_4 * w * BUMP_K * f;
    let (r7, shift) = (0.5 / f, 0.5 / w);
    let bump = |p: f32| {
        let wf = 1.0 - (signed_phase(p).abs() * r7).min(1.0);
        wf * wf * wf * k6
    };
    -2.0 * fade(f) * (body + bump(t + shift) + bump(t - shift))
}

#[cfg(test)]
mod tests {
    use super::super::tests_fw_stage_golden9 as gold;
    use super::*;

    #[test]
    fn shape4_matches_the_python_model_golden_grid() {
        let worst = (0..gold::SH4_OUT.len())
            .map(|i| {
                let (p, dt) = (
                    f32::from_bits(gold::SH4_PHASE[i]),
                    f32::from_bits(gold::SH4_DT[i]),
                );
                let got = shape4_core(p, dt, dt, f32::from_bits(gold::SH4_SHAPE[i]));
                (got - f32::from_bits(gold::SH4_OUT[i])).abs()
            })
            .fold(0.0, f32::max);
        assert!(worst < 2e-5, "worst deviation {worst}");
    }

    #[test]
    fn shape3_matches_the_python_model_golden_grid() {
        let worst = (0..gold::SH3_OUT.len())
            .map(|i| {
                let (p, dt) = (
                    f32::from_bits(gold::SH3_PHASE[i]),
                    f32::from_bits(gold::SH3_DT[i]),
                );
                let got = shape3_core(p, dt, f32::from_bits(gold::SH3_SHAPE[i]));
                (got - f32::from_bits(gold::SH3_OUT[i])).abs()
            })
            .fold(0.0, f32::max);
        assert!(worst < 2e-5, "worst deviation {worst}");
    }

    #[test]
    fn cos_helper_is_minus_cosine_of_the_scaled_argument() {
        assert!((cos_helper(0.0) + 1.0).abs() < 1e-4);
        assert!(cos_helper(1.0).abs() < 1e-6);
        let x = 0.4f32;
        assert!((cos_helper(x) + (std::f32::consts::FRAC_PI_2 * x).cos()).abs() < 1e-4);
        assert_eq!(cos_helper(0.3).to_bits(), cos_helper(-0.3).to_bits());
    }

    #[test]
    fn shape3_bump_is_centred_on_phase_zero() {
        // away from phase 0 the clipped argument is 1: the body is the constant 2 / (pi w)
        let (i_sy, s) = (0.002f32, 0.5f32);
        let far = shape3_core(0.5, i_sy, s);
        let near = shape3_core(0.0, i_sy, s);
        assert!(far.is_finite() && near.is_finite());
        assert!((near - far).abs() > 0.5, "near {near} far {far}");
        let (l, r) = (shape3_core(0.02, i_sy, s), shape3_core(-0.02, i_sy, s));
        assert!((l - r).abs() < 1e-4, "{l} {r}");
    }

    #[test]
    fn shape4_second_parabola_follows_the_spread_word() {
        let a = shape4_core(0.3, 0.005, 0.005, 0.2);
        let b = shape4_core(0.3, 0.005, 0.005, 0.8);
        assert!(a.is_finite() && b.is_finite() && (a - b).abs() > 1e-4);
    }

    #[test]
    fn both_cores_are_finite_bounded_and_deterministic() {
        for shape in [0.0f32, 0.25, 0.5, 1.0] {
            for dt in [0.0005f32, 0.005, 0.05] {
                for k in 0..256 {
                    let p = k as f32 / 256.0;
                    let (y3, y4) = (shape3_core(p, dt, shape), shape4_core(p, dt, dt, shape));
                    assert!(
                        y3.is_finite() && y3.abs() < 60.0,
                        "3: {shape} {dt} {k}: {y3}"
                    );
                    assert!(
                        y4.is_finite() && y4.abs() < 60.0,
                        "4: {shape} {dt} {k}: {y4}"
                    );
                    assert_eq!(y3.to_bits(), shape3_core(p, dt, shape).to_bits());
                }
            }
        }
    }
}
