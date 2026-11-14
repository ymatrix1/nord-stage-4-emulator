//! Shared laws of the firmware ANALOG stages: noise generator, spread law, high-pitch fade and
//! the inline saw / Shape Square helpers.
//!
//! Split out of `fw_stages` (code-structure rule); behaviour unchanged. Spec:
//! `docs/specs/synth-engine.md` section 4.3; evidence `docs/research/synth-stage-models.md`.

use super::super::fw_shape::signed_phase;
use super::super::hw_saw::{SAW_A, SAW_B, SAW_C};

/// VERIFIED: LCG multiplier `DM(0xE08C8)` of the image (`tools/synth_stage_noise.py`).
const NOISE_MULT: i32 = 0x0BB3_8435;
/// VERIFIED: LCG increment `DM(0xE08C9)`.
const NOISE_INC: i32 = 0x3619_636B;
/// VERIFIED (image word `DM(0xB2EF9)`): initial generator state of lane x.
pub const NOISE_SEED: u32 = 3;
/// VERIFIED: the Shape Square helper amplitude `R6 = 0x3F333333`.
const SQUARE_AMP: f32 = 0.7;
/// VERIFIED: the Shape Square width closes as `1 - min(1, 5.5 a)` (`0x40B00000`).
const SQUARE_FADE: f32 = 5.5;
/// VERIFIED: spread law `u = 0.35 (t + 2 t^4)`, `t = s5 + 0.06`.
const SPREAD_OFFSET: f32 = 0.06;
const SPREAD_SCALE: f32 = 0.35;
/// ASSUMED: the SUPER voice ratios (P block, unknown): a symmetric spread `-1..1` around the pitch.
pub const SUPER_RATIOS: [f32; 8] = [-1.0, -0.714, -0.429, -0.143, 0.143, 0.429, 0.714, 1.0];
/// ASSUMED: spread word of SUPER at Osc Ctrl 0..1 is `0.1 ctrl` (the hardware scale is open).
pub const SUPER_SPREAD_MAX: f32 = 0.1;
/// VERIFIED (image words `DM(0xC1596..0xC1598)`): the idle MULTI voice multipliers (octaves).
pub const MULTI_OCTAVES: [f32; 3] = [1.0, 2.0, 0.5];

/// One LCG step of the noise generator (32-bit wrap).
#[inline]
pub fn noise_step(state: u32) -> u32 {
    (state as i32)
        .wrapping_mul(NOISE_MULT)
        .wrapping_add(NOISE_INC) as u32
}

/// `s 2^-31` as the stage stores it: the integer converted to float and truncated to the 24-bit
/// significand of a memory word.
#[inline]
pub fn noise_sample(state: u32) -> f32 {
    let v = state as i32;
    let mag = v.unsigned_abs();
    let extra = (32 - mag.leading_zeros()).saturating_sub(24);
    let t = (mag >> extra) << extra;
    let f = t as f32 * (1.0 / 2_147_483_648.0);
    if v < 0 {
        -f
    } else {
        f
    }
}

/// High-pitch fade `g = C (B + clip(A - a, +-B))` of the saw helpers, `a = 2 i_sy`.
#[inline]
pub(in crate::synth) fn fade(a: f32) -> f32 {
    SAW_C * (SAW_B + (SAW_A - a).clamp(-SAW_B, SAW_B))
}

/// The inline saw of the unison stages, `copysign(w^2, xo) - xo` (a falling band-limited saw
/// with the sign of the firmware stage output), for the phase `p` (cycles) and the increment `dt`
/// (cycles per sample).
#[inline]
pub fn saw_diff(p: f32, dt: f32) -> f32 {
    let xf = signed_phase(p);
    let xo = signed_phase(p + 0.5);
    let w = 1.0 - (xf.abs() * 0.25 / dt.max(1e-6)).min(1.0);
    (w * w).copysign(xo) - xo
}

/// Detune amount `u` of the SUPER / MULTI spread word `s5`.
#[inline]
pub fn spread_u(s5: f32) -> f32 {
    let t = s5 + SPREAD_OFFSET;
    let q = t * t;
    SPREAD_SCALE * (t + 2.0 * q * q)
}

/// One sample of one Shape Square oscillator: two band-limited saws a pulse width apart.
/// `i_sy` is the increment in cycles per SY frame (fade and width law), `dt` the render increment.
#[inline]
pub fn square_wave(t: f32, dt: f32, i_sy: f32, shape: f32) -> f32 {
    let a = 2.0 * i_sy;
    let room = 1.0 - (a * SQUARE_FADE).min(1.0);
    let w = shape.clamp(-room, room).clamp(-1.0, 1.0);
    let g = SQUARE_AMP * fade(a);
    // `saw_diff` is minus the helper saw, so the firmware `-(saw(x) - saw(x - off))` is its difference
    g * (saw_diff(t + 0.5 * (1.0 - w), dt) - saw_diff(t, dt))
}

#[cfg(test)]
mod tests {
    use super::super::super::tests_fw_stage_golden as gold;
    use super::super::super::tests_fw_stage_golden2 as gold2;
    use super::*;

    fn worst(got: impl Iterator<Item = (f32, f32)>) -> f32 {
        got.map(|(a, b)| (a - b).abs()).fold(0.0, f32::max)
    }

    #[test]
    fn saw_matches_the_python_model_golden_grid() {
        let w = worst((0..gold::SAW_OUT.len()).map(|i| {
            let (p, dt) = (
                f32::from_bits(gold::SAW_PHASE[i]),
                f32::from_bits(gold::SAW_DT[i]),
            );
            (saw_diff(p, dt), f32::from_bits(gold::SAW_OUT[i]))
        }));
        assert!(w < 1e-6, "worst deviation {w}");
    }

    #[test]
    fn noise_matches_the_python_model_bit_for_bit() {
        let mut s = NOISE_SEED;
        for i in 0..gold::NOISE_OUT.len() {
            s = noise_step(s);
            assert_eq!(s, gold::NOISE_STATE[i], "state {i}");
            assert_eq!(noise_sample(s).to_bits(), gold::NOISE_OUT[i], "sample {i}");
        }
    }

    #[test]
    fn spread_law_matches_the_python_model() {
        let w = worst((0..gold::SPREAD_IN.len()).map(|i| {
            (
                spread_u(f32::from_bits(gold::SPREAD_IN[i])),
                f32::from_bits(gold::SPREAD_OUT[i]),
            )
        }));
        assert!(w < 1e-6, "worst deviation {w}");
    }

    #[test]
    fn shape_square_matches_the_python_model_golden_grid() {
        let w = worst((0..gold2::SQUARE_OUT.len()).map(|i| {
            let (p, dt) = (
                f32::from_bits(gold2::SQUARE_PHASE[i]),
                f32::from_bits(gold2::SQUARE_DT[i]),
            );
            let got = square_wave(p, dt, dt, f32::from_bits(gold2::SQUARE_SHAPE[i]));
            (got, f32::from_bits(gold2::SQUARE_OUT[i]))
        }));
        assert!(w < 1e-6, "worst deviation {w}");
    }
}
