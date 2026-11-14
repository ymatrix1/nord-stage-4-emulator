//! Firmware ANALOG oscillator stages beyond SHAPE as opt-in models: noise (`0x121FD8`), Shape
//! Square (`0x121AB5`), SUPER (`0x121D6D`) and MULTI (`0x121C54`).
//!
//! Spec: `docs/specs/synth-engine.md` section 4.3; evidence `docs/research/synth-stage-models.md`;
//! bit-exact Python models `tools/synth_stage_*.py` (320 of 320 oracle words each), goldens
//! `tools/synth_stage_golden.py` -> `tests_fw_stage_golden.rs`.
//!
//! One lane of each stage with the mix weights folded to one level. The inputs the firmware takes
//! from the program P block (voice ratios, weights, spread word) are not known (the P-block writer
//! is open, OQ22): they are the ASSUMED constants below.

use super::fw_shape::signed_phase;
use super::gain_dezip::SY_FRAME_RATE_HZ;
use super::hw_saw::{SAW_A, SAW_B, SAW_C};

/// Which firmware stage an ANALOG category / wave selects (`data/synth-wave-types.csv`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FwStage {
    /// PURE wave 6 (stage `0x121FD8`).
    Noise,
    /// SHAPE wave 2 (stage `0x121AB5`).
    ShapeSquare,
    /// SUPER wave 0 (stage `0x121D6D`, eight detuned saws).
    Super,
    /// MULTI, all six waves (stage `0x121C54`, three octave saws).
    Multi,
    /// SHAPE SINE wave 0 (stage `0x123427`, a phase-warped sine).
    ShapeSine,
}

impl FwStage {
    /// Stage of an ANALOG `(category, wave)` pair, `None` for the waves without a model here.
    pub fn from_category_wave(category: u8, wave: u8) -> Option<Self> {
        match (category, wave) {
            (0, 6) => Some(Self::Noise),
            (3, 2) => Some(Self::ShapeSquare),
            (6, 0) => Some(Self::Super),
            (5, _) => Some(Self::Multi),
            (4, 0) => Some(Self::ShapeSine),
            _ => None,
        }
    }
}

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
fn fade(a: f32) -> f32 {
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

/// VERIFIED (image words of the sine helper `0x12522E`): `t (c12 + q (c14 + q (c16 + q c18)))`.
const SINE_C12: f32 = 1.570_790_8;
const SINE_C14: f32 = -0.645_891_1;
const SINE_C16: f32 = 0.079_430_89;
const SINE_C18: f32 = -0.004_331_1;
/// VERIFIED (listing `0x2468AE..0x2468DC`): the warp width closes as `1 - min(1, 5.5 a)`.
const SSINE_FADE: f32 = 5.5;
/// VERIFIED: fade of the sine stages `C (B + clip(A - a, +-B))` (`0x3F30F34B`, `0x3E6BEF0F`, `0x400AE2EB`).
const SINE_FADE_A: f32 = 0.691_212_36;
const SINE_FADE_B: f32 = 0.230_404_12;
const SINE_FADE_C: f32 = 2.170_1;

/// The sine polynomial of the helper `0x12522E` on `v = 2 y` (`y` = warped phase, -1..1).
#[inline]
fn sine_poly(v: f32) -> f32 {
    let t = v.abs() - 1.0;
    let q = t * t;
    t * (SINE_C12 + q * (SINE_C14 + q * (SINE_C16 + q * SINE_C18)))
}

/// One sample of SHAPE SINE wave 0: the phase `t` (cycles) is warped by two slopes around the knee
/// `-w` (`w = (1 - (1 - s5)^2) (1 - min(1, 5.5 a))`, `a = 2 i_sy`), then fed to the sine polynomial
/// and the sine fade. `shape` is the spread word `s5` (idle 0.5).
#[inline]
pub fn shape_sine_wave(t: f32, i_sy: f32, shape: f32) -> f32 {
    let a = 2.0 * i_sy;
    let s = 1.0 - shape;
    let w = (1.0 - s * s) * (1.0 - (a * SSINE_FADE).min(1.0));
    let x = signed_phase(t);
    let hi = (x + 1.0) / (1.0 - w).max(1e-3);
    let y = if hi >= 1.0 { (x - 1.0) / (1.0 + w) } else { hi };
    let g = SINE_FADE_C * (SINE_FADE_B + (SINE_FADE_A - a).clamp(-SINE_FADE_B, SINE_FADE_B));
    sine_poly(2.0 * y) * g
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

/// Render state of the voice oscillators of one synth voice (pre-allocated, no allocation per sample).
#[derive(Debug, Clone, Copy)]
pub struct FwStageState {
    /// Noise generator state.
    pub noise: u32,
    /// Voice phases in cycles (SUPER uses 8, MULTI 3).
    pub phases: [f32; 8],
}

impl Default for FwStageState {
    fn default() -> Self {
        Self {
            noise: NOISE_SEED,
            phases: [0.0; 8],
        }
    }
}

impl FwStageState {
    /// One sample of `stage` at the base phase `t`, render increment `dt`, pitch `freq_hz`,
    /// shape / Osc Ctrl value `ctrl` in `0..=1`.
    #[inline]
    pub fn sample(&mut self, stage: FwStage, t: f32, dt: f32, freq_hz: f32, ctrl: f32) -> f32 {
        let i_sy = freq_hz / SY_FRAME_RATE_HZ;
        match stage {
            FwStage::Noise => {
                self.noise = noise_step(self.noise);
                noise_sample(self.noise)
            }
            // ASSUMED: shape word = Osc Ctrl scaled to the idle 0.5 at 0.5 (the knob scale is open).
            FwStage::ShapeSquare => square_wave(t, dt, i_sy, ctrl),
            FwStage::Super => {
                let u = spread_u(SUPER_SPREAD_MAX * ctrl);
                let g = fade(2.0 * i_sy);
                let mut sum = 0.0;
                for (ph, rho) in self.phases.iter_mut().zip(SUPER_RATIOS) {
                    let dt_j = dt * (rho * u + 1.0 - u);
                    sum += saw_diff(*ph, dt_j);
                    *ph = (*ph + dt_j) % 1.0;
                }
                // ASSUMED: equal voice weights summing to one
                sum * g * 0.125
            }
            // ASSUMED: shape word = Osc Ctrl (idle 0.5 when the strip shape is unset).
            FwStage::ShapeSine => shape_sine_wave(t, i_sy, ctrl),
            FwStage::Multi => {
                let mut sum = 0.0;
                for (ph, c) in self.phases.iter_mut().zip(MULTI_OCTAVES) {
                    let dt_v = dt * c;
                    // the fade of each voice is folded into its weight (`0x243A3C`)
                    sum += saw_diff(*ph, dt_v) * fade(2.0 * i_sy * c);
                    *ph = (*ph + dt_v) % 1.0;
                }
                // ASSUMED: equal voice weights summing to one
                sum * (1.0 / 3.0)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_fw_stage_golden as gold;
    use super::super::tests_fw_stage_golden2 as gold2;
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

    #[test]
    fn shape_sine_matches_the_python_model_golden_grid() {
        let w = worst((0..gold2::SSINE_OUT.len()).map(|i| {
            let (p, dt) = (
                f32::from_bits(gold2::SSINE_PHASE[i]),
                f32::from_bits(gold2::SSINE_DT[i]),
            );
            let got = shape_sine_wave(p, dt, f32::from_bits(gold2::SSINE_SHAPE[i]));
            (got, f32::from_bits(gold2::SSINE_OUT[i]))
        }));
        assert!(w < 2e-6, "worst deviation {w}");
    }

    #[test]
    fn category_wave_map_selects_the_modelled_stages_only() {
        assert_eq!(FwStage::from_category_wave(0, 6), Some(FwStage::Noise));
        assert_eq!(
            FwStage::from_category_wave(3, 2),
            Some(FwStage::ShapeSquare)
        );
        assert_eq!(FwStage::from_category_wave(6, 0), Some(FwStage::Super));
        assert_eq!(FwStage::from_category_wave(5, 4), Some(FwStage::Multi));
        assert_eq!(FwStage::from_category_wave(4, 0), Some(FwStage::ShapeSine));
        assert_eq!(FwStage::from_category_wave(4, 1), None);
        assert_eq!(FwStage::from_category_wave(3, 0), None);
        assert_eq!(FwStage::from_category_wave(2, 1), None);
    }

    #[test]
    fn stages_are_finite_bounded_and_deterministic() {
        for stage in [
            FwStage::Noise,
            FwStage::ShapeSquare,
            FwStage::Super,
            FwStage::Multi,
            FwStage::ShapeSine,
        ] {
            let (mut a, mut b) = (FwStageState::default(), FwStageState::default());
            let dt = 220.0 / 48_000.0;
            let mut t = 0.0f32;
            for _ in 0..2000 {
                let x = a.sample(stage, t, dt, 220.0, 0.5);
                assert!(x.is_finite() && x.abs() < 8.0, "{stage:?}: {x}");
                assert_eq!(x.to_bits(), b.sample(stage, t, dt, 220.0, 0.5).to_bits());
                t = (t + dt) % 1.0;
            }
        }
    }

    #[test]
    fn noise_is_zero_mean_and_multi_voices_advance_by_octaves() {
        let mut s = FwStageState::default();
        let mean: f32 = (0..20_000)
            .map(|_| s.sample(FwStage::Noise, 0.0, 0.0, 0.0, 0.0))
            .sum::<f32>()
            / 20_000.0;
        assert!(mean.abs() < 0.03, "mean {mean}");
        let mut m = FwStageState::default();
        m.sample(FwStage::Multi, 0.0, 0.01, 220.0, 0.0);
        assert!((m.phases[1] - 2.0 * m.phases[0]).abs() < 1e-7);
        assert!((m.phases[2] - 0.5 * m.phases[0]).abs() < 1e-7);
    }
}
