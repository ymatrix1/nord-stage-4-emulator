//! Firmware ANALOG SHAPE oscillator (Core2 stage `0x1212D3`, helper `0x1251D4`) as an opt-in model.
//!
//! Spec: `docs/specs/synth-engine.md` section 4.2; evidence `docs/research/synth-wave-models.md`;
//! bit-exact Python model `tools/synth_wave_model.py`, golden `tools/synth_wave_golden.py`.
//!
//! One oscillator of the stage (lane x, mix weight 1, ratio 1, no detune) is the difference of two
//! band-limited parabolas, the second one shifted by `(1 - W) / 2` cycles:
//!
//! ```text
//! a    = 2 i_sy                          i_sy: increment in cycles per SY frame (43,402.34 Hz)
//! W    = shape (1 - min(1, 1.5 a))       pulse width, shape = clip(s17 + s19, 1)
//! y(p; k) = xo^2 - k a w^3               xo = 2 frac(p + 1/2) - 1, w = 1 - min(1, |x| / (4 i)),
//!                                        x = 2 (p wrapped to -1/2..1/2), i the increment at the render rate
//! out  = g (y(p; 4/3) - y(p + (1 - W)/2; 16/9)) / (1 - W^2),   g = C (B + clip(A - a, +-B))
//! ```
//!
//! The second call of the helper sees the already scaled `R6`, hence `16/9` (VERIFIED against the
//! oracle, 320 of 320 words bit exact in the Python model).

use super::gain_dezip::SY_FRAME_RATE_HZ;
use super::hw_saw::{SAW_A, SAW_B, SAW_C};

/// VERIFIED: `0x3FAAAA8F` of the helper `0x1251D4` (`F6 = F6 * F11`), 4/3 to float32.
const FOUR_THIRDS: f32 = 1.333_333_4;

/// Signed phase `x` of the helper (`FLOAT R0 BY -31`, -1..1) for a phase in cycles.
#[inline]
fn signed_phase(p: f32) -> f32 {
    let q = p - p.floor();
    2.0 * if q >= 0.5 { q - 1.0 } else { q }
}

/// One helper sample: band-limited parabola for the phase `p` (cycles), increment `inc` at the
/// render rate (cycles per sample) and the coefficient `k a`.
#[inline]
fn parabola(p: f32, inc: f32, ka: f32) -> f32 {
    let xf = signed_phase(p);
    let xo = signed_phase(p + 0.5);
    let w = 1.0 - (xf.abs() * 0.25 / inc).min(1.0);
    xo * xo - w * w * w * ka
}

/// Pulse width `W` of the stage for the SY increment `i_sy` and the shape word.
#[inline]
pub fn fw_shape_width(i_sy: f32, shape: f32) -> f32 {
    shape.clamp(-1.0, 1.0) * (1.0 - (3.0 * i_sy).min(1.0))
}

/// One sample of the SHAPE oscillator at phase `t` (cycles, `0..1`) with the render increment
/// `dt` and the frequency `freq_hz`; `shape` is the strip shape word (the idle frame holds 0.5).
#[inline]
pub fn fw_shape_wave(t: f32, dt: f32, freq_hz: f32, shape: f32) -> f32 {
    let dt = dt.max(1e-6);
    let i_sy = freq_hz / SY_FRAME_RATE_HZ;
    let a = 2.0 * i_sy;
    let width = fw_shape_width(i_sy, shape);
    let d = 1.0 - width * width;
    let g = SAW_C * (SAW_B + (SAW_A - a).clamp(-SAW_B, SAW_B));
    let shift = 0.5 * (1.0 - width);
    // the correction scales with the increment at the render rate (equal to `a` at the SY rate)
    let k1 = FOUR_THIRDS * 2.0 * dt;
    let k2 = FOUR_THIRDS * k1;
    (parabola(t, dt, k1) - parabola(t + shift, dt, k2)) * g / d.max(1e-3)
}

#[cfg(test)]
mod tests {
    use super::super::tests_fw_shape_golden::{SHAPE_INC, SHAPE_OUT, SHAPE_PHASE, SHAPE_SHAPE};
    use super::*;

    /// Golden grid of `tools/synth_wave_golden.py`: phase, SY increment, shape and the sample of one
    /// oscillator of the bit-exact Python model.
    #[test]
    fn matches_the_python_model_golden_grid() {
        let mut worst = 0.0f32;
        for i in 0..SHAPE_PHASE.len() {
            let (p, inc) = (f32::from_bits(SHAPE_PHASE[i]), f32::from_bits(SHAPE_INC[i]));
            let want = f32::from_bits(SHAPE_OUT[i]);
            let got = fw_shape_wave(
                p,
                inc,
                inc * SY_FRAME_RATE_HZ,
                f32::from_bits(SHAPE_SHAPE[i]),
            );
            worst = worst.max((got - want).abs());
        }
        assert!(worst < 1e-5, "worst deviation {worst}");
    }

    #[test]
    fn width_follows_shape_and_shrinks_at_high_pitch() {
        assert_eq!(fw_shape_width(0.0, 0.5), 0.5);
        assert!((fw_shape_width(0.1, 0.5) - 0.35).abs() < 1e-6);
        assert_eq!(fw_shape_width(0.5, 0.5), 0.0);
    }

    #[test]
    fn output_is_finite_and_bounded_over_a_cycle() {
        for shape in [0.0f32, 0.25, 0.5, 0.9] {
            for k in 0..256 {
                let y = fw_shape_wave(k as f32 / 256.0, 0.01, 440.0, shape);
                assert!(y.is_finite() && y.abs() < 8.0, "shape {shape} k {k}: {y}");
            }
        }
    }
}
