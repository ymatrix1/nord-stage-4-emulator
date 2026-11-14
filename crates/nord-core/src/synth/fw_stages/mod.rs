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

use super::fw_misc::{fm_core, fm_ratio, lowpass_sample, pink_sample, PINK_ROWS};
use super::fw_shape34::{shape3_core, shape4_core};
use super::fw_sine3::shape_sine3_core;
use super::fw_sine_hp::{shape_sine_hp_core, OutputHighPass};
use super::fw_sine_warps::{shape_sine_fold, shape_sine_gain, shape_sine_res, shape_sine_wave};
use super::fw_sub::sub_core;
use super::fw_super1::super1_sample;
use super::fw_super2::{prepare, super24_sample, Super24State};
use super::fw_sync::sync2_core;
use super::fw_sync1::sync1_core;
use super::fw_sync3::sync3_core;
use super::fw_sync6::{sync6_core, sync7_core, sync8_core};
use super::gain_dezip::SY_FRAME_RATE_HZ;
use crate::phase_wrap::wrap_pos;

mod laws;
pub(in crate::synth) use laws::fade;
use laws::square_wave;
pub use laws::{noise_sample, noise_step, saw_diff, spread_u, NOISE_SEED};
pub use laws::{MULTI_OCTAVES, SUPER_RATIOS, SUPER_SPREAD_MAX};

/// Which firmware stage an ANALOG category / wave selects (`data/synth-wave-types.csv`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FwStage {
    /// PURE wave 6 (stage `0x121FD8`).
    Noise,
    /// SHAPE wave 2 (stage `0x121AB5`).
    ShapeSquare,
    /// SUPER wave 0 (stage `0x121D6D`, eight detuned saws).
    Super,
    /// SUPER wave 1 (stage `0x121E9D`, seven detuned band-limited squares).
    Super1,
    /// SUPER waves 2..4 (stage `0x124985`, four detuned voices on a band-limited wavetable).
    Super24,
    /// MULTI, all six waves (stage `0x121C54`, three octave saws).
    Multi,
    /// SHAPE wave 3 (stage `0x120F55`, a cosine bump with two band-limiting cubes).
    Shape3,
    /// SHAPE wave 4 (stage `0x12145E`, two shifted band-limited parabolas plus a bias).
    Shape4,
    /// SHAPE SINE wave 0 (stage `0x123427`, a phase-warped sine).
    ShapeSine,
    /// SHAPE SINE wave 1 (stage `0x1235C9`, folded half cycles).
    ShapeSineFold,
    /// SHAPE SINE wave 2 (stage `0x1232A5`, amplified and clipped phase).
    ShapeSineGain,
    /// SHAPE SINE wave 5 (stage `0x122F1F`, resonant sine).
    ShapeSineRes,
    /// SYNC wave 1 (stage `0x1226DD`, saw-family helpers on a table-ratio phase, high pass).
    Sync1,
    /// SYNC wave 2 (stage `0x122A32`, hard-sync saw pair with a table-driven ratio).
    Sync2,
    /// SYNC waves 3..5 (stage `0x122C5C`, square edges on a table-driven sync phase, high pass).
    Sync3,
    /// SYNC wave 6 (stage `0x1239BE`, `Sync3` family without pulse word and high pass).
    Sync6,
    /// SYNC wave 7 (stage `0x123C7C`, three stacked edges and the SYNC 6 ramp).
    Sync7,
    /// SYNC wave 8 (stage `0x123FE9`, the same family with a doubled ratio).
    Sync8,
    /// SHAPE SINE wave 3 (stage `0x12377D`, warp plus two band-limited cubes).
    ShapeSine3,
    /// SHAPE SINE wave 4 (stage `0x120DAB`, folded sine with an output high pass).
    ShapeSineHp,
    /// SUB OSC wave 0 (stage `0x12179A`, a saw plus a square one octave down).
    SubOsc,
    /// MISC wave 0 (stage `0x122030`, pink noise with an output high pass).
    MiscPink,
    /// MISC wave 1 (stage `0x122151`, low-passed noise).
    MiscLowpass,
    /// MISC wave 2 (stage `0x122417`, sine with a cubed phase-modulated level).
    MiscFm,
}

impl FwStage {
    /// Stage of an ANALOG `(category, wave)` pair, `None` for the waves without a model here.
    pub fn from_category_wave(category: u8, wave: u8) -> Option<Self> {
        match (category, wave) {
            (0, 6) => Some(Self::Noise),
            (1, 0) => Some(Self::SubOsc),
            (7, 0) => Some(Self::MiscPink),
            (7, 1) => Some(Self::MiscLowpass),
            (7, 2) => Some(Self::MiscFm),
            (2, 1) => Some(Self::Sync1),
            (2, 2) => Some(Self::Sync2),
            (2, 3..=5) => Some(Self::Sync3),
            (2, 6) => Some(Self::Sync6),
            (2, 7) => Some(Self::Sync7),
            (2, 8) => Some(Self::Sync8),
            (3, 2) => Some(Self::ShapeSquare),
            (3, 3) => Some(Self::Shape3),
            (3, 4) => Some(Self::Shape4),
            (6, 0) => Some(Self::Super),
            (6, 1) => Some(Self::Super1),
            (6, 2..=4) => {
                prepare(); // parse the tables here, not in the audio thread
                Some(Self::Super24)
            }
            (5, _) => Some(Self::Multi),
            (4, 0) => Some(Self::ShapeSine),
            (4, 1) => Some(Self::ShapeSineFold),
            (4, 2) => Some(Self::ShapeSineGain),
            (4, 5) => Some(Self::ShapeSineRes),
            (4, 3) => Some(Self::ShapeSine3),
            (4, 4) => Some(Self::ShapeSineHp),
            _ => None,
        }
    }
}

/// Render state of the voice oscillators of one synth voice (pre-allocated, no allocation per sample).
#[derive(Debug, Clone, Copy)]
pub struct FwStageState {
    /// Noise generator state.
    pub noise: u32,
    /// Voice phases in cycles (SUPER uses 8, MULTI 3).
    pub phases: [f32; 8],
    /// Output high pass of SHAPE SINE wave 4, SYNC 1 / 3 and MISC 0 / 2.
    pub hp: OutputHighPass,
    /// MISC 0 pink noise rows.
    pub pink: [f32; PINK_ROWS],
    /// MISC 0 frame counter.
    pub counter: u32,
    /// MISC 1 low-pass state.
    pub lp: f32,
    /// SUPER 2..4 phases, hold and biquad states.
    pub s24: Super24State,
}

impl Default for FwStageState {
    fn default() -> Self {
        Self {
            noise: NOISE_SEED,
            phases: [0.0; 8],
            hp: OutputHighPass::default(),
            pink: [0.0; PINK_ROWS],
            counter: 0,
            lp: 0.0,
            s24: Super24State::default(),
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
                    *ph = wrap_pos(*ph + dt_j, 1.0);
                }
                // ASSUMED: equal voice weights summing to one
                sum * g * 0.125
            }
            // ASSUMED: shape word = Osc Ctrl (idle 0.5 when the strip shape is unset).
            FwStage::ShapeSine => shape_sine_wave(t, i_sy, ctrl),
            FwStage::ShapeSineFold => shape_sine_fold(t, i_sy, ctrl),
            FwStage::ShapeSineGain => shape_sine_gain(t, i_sy, ctrl),
            FwStage::ShapeSineRes => shape_sine_res(t, i_sy, ctrl),
            FwStage::Sync2 => sync2_core(t, i_sy, ctrl),
            FwStage::Sync3 => self.hp.process(sync3_core(t, i_sy, ctrl)),
            FwStage::Sync1 => self.hp.process(sync1_core(t, i_sy, ctrl)),
            FwStage::Sync6 => sync6_core(t, i_sy, ctrl),
            FwStage::Sync7 => sync7_core(t, i_sy, ctrl),
            FwStage::Sync8 => sync8_core(t, i_sy, ctrl),
            FwStage::ShapeSine3 => shape_sine3_core(t, i_sy, ctrl),
            FwStage::Shape3 => shape3_core(t, i_sy, ctrl),
            FwStage::Shape4 => shape4_core(t, dt, i_sy, ctrl),
            FwStage::ShapeSineHp => self.hp.process(shape_sine_hp_core(t, i_sy, ctrl)),
            FwStage::SubOsc => {
                // the firmware phase advances at half the pitch increment (saw at `2 u`)
                let y = sub_core(self.phases[0], dt, i_sy, ctrl);
                self.phases[0] = wrap_pos(self.phases[0] + 0.5 * dt, 1.0);
                y
            }
            FwStage::MiscPink => {
                let x = pink_sample(&mut self.pink, &mut self.counter, &mut self.noise);
                self.hp.process(x)
            }
            FwStage::MiscLowpass => lowpass_sample(&mut self.lp, &mut self.noise),
            FwStage::MiscFm => {
                let y = fm_core(t, self.phases[0], i_sy, ctrl);
                self.phases[0] = wrap_pos(self.phases[0] + dt * fm_ratio(i_sy, ctrl), 1.0);
                self.hp.process(y)
            }
            FwStage::Super1 => super1_sample(&mut self.phases, dt, i_sy, ctrl),
            FwStage::Super24 => super24_sample(&mut self.s24, dt, i_sy, ctrl),
            FwStage::Multi => {
                let mut sum = 0.0;
                for (ph, c) in self.phases.iter_mut().zip(MULTI_OCTAVES) {
                    let dt_v = dt * c;
                    // the fade of each voice is folded into its weight (`0x243A3C`)
                    sum += saw_diff(*ph, dt_v) * fade(2.0 * i_sy * c);
                    *ph = wrap_pos(*ph + dt_v, 1.0);
                }
                // ASSUMED: equal voice weights summing to one
                sum * (1.0 / 3.0)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_wave_map_selects_the_modelled_stages_only() {
        assert_eq!(FwStage::from_category_wave(0, 6), Some(FwStage::Noise));
        assert_eq!(
            FwStage::from_category_wave(3, 2),
            Some(FwStage::ShapeSquare)
        );
        assert_eq!(FwStage::from_category_wave(6, 0), Some(FwStage::Super));
        assert_eq!(FwStage::from_category_wave(6, 1), Some(FwStage::Super1));
        assert_eq!(FwStage::from_category_wave(6, 2), Some(FwStage::Super24));
        assert_eq!(FwStage::from_category_wave(6, 4), Some(FwStage::Super24));
        assert_eq!(FwStage::from_category_wave(6, 5), None);
        assert_eq!(FwStage::from_category_wave(5, 4), Some(FwStage::Multi));
        assert_eq!(FwStage::from_category_wave(4, 0), Some(FwStage::ShapeSine));
        assert_eq!(
            FwStage::from_category_wave(4, 1),
            Some(FwStage::ShapeSineFold)
        );
        assert_eq!(
            FwStage::from_category_wave(4, 2),
            Some(FwStage::ShapeSineGain)
        );
        assert_eq!(
            FwStage::from_category_wave(4, 5),
            Some(FwStage::ShapeSineRes)
        );
        assert_eq!(
            FwStage::from_category_wave(4, 4),
            Some(FwStage::ShapeSineHp)
        );
        assert_eq!(FwStage::from_category_wave(4, 3), Some(FwStage::ShapeSine3));
        assert_eq!(FwStage::from_category_wave(4, 6), None);
        assert_eq!(FwStage::from_category_wave(3, 3), Some(FwStage::Shape3));
        assert_eq!(FwStage::from_category_wave(3, 4), Some(FwStage::Shape4));
        assert_eq!(FwStage::from_category_wave(3, 0), None);
        assert_eq!(FwStage::from_category_wave(2, 2), Some(FwStage::Sync2));
        assert_eq!(FwStage::from_category_wave(2, 3), Some(FwStage::Sync3));
        assert_eq!(FwStage::from_category_wave(2, 5), Some(FwStage::Sync3));
        assert_eq!(FwStage::from_category_wave(2, 6), Some(FwStage::Sync6));
        assert_eq!(FwStage::from_category_wave(2, 8), Some(FwStage::Sync8));
        assert_eq!(FwStage::from_category_wave(2, 7), Some(FwStage::Sync7));
        assert_eq!(FwStage::from_category_wave(2, 1), Some(FwStage::Sync1));
        assert_eq!(FwStage::from_category_wave(2, 0), None);
        assert_eq!(FwStage::from_category_wave(1, 0), Some(FwStage::SubOsc));
        assert_eq!(FwStage::from_category_wave(1, 1), None);
        assert_eq!(FwStage::from_category_wave(7, 0), Some(FwStage::MiscPink));
        assert_eq!(
            FwStage::from_category_wave(7, 1),
            Some(FwStage::MiscLowpass)
        );
        assert_eq!(FwStage::from_category_wave(7, 2), Some(FwStage::MiscFm));
        assert_eq!(FwStage::from_category_wave(7, 3), None);
    }

    #[test]
    fn stages_are_finite_bounded_and_deterministic() {
        for stage in [
            FwStage::Noise,
            FwStage::ShapeSquare,
            FwStage::Super,
            FwStage::Super1,
            FwStage::Super24,
            FwStage::Sync2,
            FwStage::Sync3,
            FwStage::Sync1,
            FwStage::Sync6,
            FwStage::Sync7,
            FwStage::Sync8,
            FwStage::Multi,
            FwStage::ShapeSine,
            FwStage::ShapeSineFold,
            FwStage::ShapeSineGain,
            FwStage::ShapeSineRes,
            FwStage::ShapeSine3,
            FwStage::Shape3,
            FwStage::Shape4,
            FwStage::ShapeSineHp,
            FwStage::SubOsc,
            FwStage::MiscPink,
            FwStage::MiscLowpass,
            FwStage::MiscFm,
        ] {
            let (mut a, mut b) = (FwStageState::default(), FwStageState::default());
            let dt = 220.0 / 48_000.0;
            let mut t = 0.0f32;
            for _ in 0..2000 {
                let x = a.sample(stage, t, dt, 220.0, 0.5);
                assert!(x.is_finite() && x.abs() < 8.0, "{stage:?}: {x}");
                assert_eq!(x.to_bits(), b.sample(stage, t, dt, 220.0, 0.5).to_bits());
                t = wrap_pos(t + dt, 1.0);
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
