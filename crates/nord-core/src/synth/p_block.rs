//! Empirical P block of the strip chain's oscillator stage (opt-in `SynthLayer::p_block`).
//!
//! The ORG core 2 stages (`fw_shape.rs`, `hw_saw.rs`) are exact for any parameter block `P`, but the
//! block a loaded program uses (ratios `P[-32]`, `P[-30]`, weights `P[-14..-10]`, gain `P[16]`,
//! shape words) is not written by any path we can reach (`docs/research/synth-wave-models.md`
//! s8 to s10). This module holds the free parameters of the stage as one small value and the
//! second-oscillator mix, so they can be set from a fit to hardware clips
//! (`docs/research/synth-p-fit.md`, `tools/synth_p_fit.py`) instead of being guessed.
//!
//! The mix is the simplified firmware form with the own-lane weights only (partner-lane weights
//! `P[-14]`, `P[-10]` stay 0, ASSUMED: the lead notes are mono, both lanes carry the same signal):
//! `out = gain (osc(f) + weight2 osc(f ratio2))`, the second oscillator on its own phase.
//!
//! Spec: `docs/specs/synth-engine.md` section 4.2.

use super::p_table::{PTable, PTableSpec};
use super::SUPERSAW_VOICES;
use crate::phase_wrap::wrap_pos;

/// Free parameters of the oscillator stage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PBlock {
    /// Output gain (the `P[16]` scale times the lane weight of oscillator 1), linear.
    pub gain: f32,
    /// Weight of the second oscillator against the first (`wA2 / wB1`, 0 = single oscillator).
    pub weight2: f32,
    /// Frequency of the second oscillator relative to the first (`P[-30] / P[-32]`).
    pub ratio2: f32,
    /// Shape word of the SHAPE stage (`DM(17, I2) + DM(19, I2)`, `0..1`).
    pub shape: f32,
    /// Factor on the displayed cutoff before the strip law (control scale, not an oscillator
    /// word; fitted together because the two are confounded on a clip with a filter).
    pub cutoff_scale: f32,
}

impl PBlock {
    /// The idle image of the block as far as it is a playable single oscillator: unit gain, no
    /// second oscillator, the idle shape word 0.5, no cutoff change.
    pub const IDLE: PBlock = PBlock {
        gain: 1.0,
        weight2: 0.0,
        ratio2: 1.0,
        shape: 0.5,
        cutoff_scale: 1.0,
    };

    /// EMPIRICAL: fit to the Smooth Lead clip and the Audible Genius Pure Saw / Pure Square notes;
    /// values, residuals and hold-out error in `docs/research/synth-p-fit.md` section 5.
    pub const FITTED: PBlock = PBlock {
        gain: FIT_GAIN,
        weight2: FIT_WEIGHT2,
        ratio2: FIT_RATIO2,
        shape: FIT_SHAPE,
        cutoff_scale: FIT_CUTOFF_SCALE,
    };

    /// The block as the Pure Saw stage sees it: one oscillator at unit gain. EMPIRICAL: the Pure Saw
    /// notes of the Audible Genius clips carry the ideal `1 / k` series (section 2 of
    /// `docs/research/synth-p-fit.md`), so a second oscillator is excluded there, and the clips'
    /// velocity is unknown, so no gain is fitted; the cutoff scale is shared.
    pub fn single(&self) -> PBlock {
        PBlock {
            gain: 1.0,
            weight2: 0.0,
            ..*self
        }
    }

    /// Builds the block from thousandths (the CLI form `--synth-p gain,w2,r2,shape,cutoff`).
    pub fn from_milli(m: [i32; 5]) -> PBlock {
        let f = |i: usize| m[i] as f32 / 1000.0;
        PBlock {
            gain: f(0).max(0.0),
            weight2: f(1).clamp(0.0, 4.0),
            ratio2: f(2).clamp(0.05, 8.0),
            shape: f(3).clamp(-1.0, 1.0),
            cutoff_scale: f(4).clamp(0.1, 8.0),
        }
    }

    /// Two-oscillator mix of one sample. `first` is the output of oscillator 1, `second` computes
    /// oscillator 2 from `(phase, increment, frequency)`; the second phase lives in `phases[0]`
    /// (the Super Saw phase bank is unused by the stages that call this).
    #[inline]
    pub fn mix(
        &self,
        first: f32,
        dt: f32,
        freq: f32,
        phases: &mut [f32; SUPERSAW_VOICES],
        second: impl FnOnce(f32, f32, f32) -> f32,
    ) -> f32 {
        if self.weight2 == 0.0 {
            return self.gain * first;
        }
        let dt2 = (dt * self.ratio2).clamp(1e-5, 0.49);
        let t2 = phases[0];
        let s2 = second(t2, dt2, freq * self.ratio2);
        phases[0] = wrap_pos(t2 + dt2, 1.0);
        self.gain * (first + self.weight2 * s2)
    }
}

/// CLI / GUI selection of the P block (`--synth-p fitted | gain,w2,r2,shape,cutoff`), `Eq` so it
/// can live in `FirmwareOptions`; the custom form is in thousandths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PSpec {
    /// The EMPIRICAL fit ([`PBlock::FITTED`]).
    Fitted,
    /// Explicit values in thousandths (fit sweeps, experiments).
    Custom([i32; 5]),
    /// The per-wave table (`p_table.rs`), optionally with rows overridden (fit sweeps).
    Table(PTableSpec),
}

impl PSpec {
    /// Parses `fitted` or five comma separated numbers `gain,w2,r2,shape,cutoff`.
    pub fn parse(v: &str) -> Result<PSpec, String> {
        if v.eq_ignore_ascii_case("fitted") {
            return Ok(PSpec::Fitted);
        }
        if let Some(rest) = v.strip_prefix("table") {
            return PTableSpec::parse(rest).map(PSpec::Table);
        }
        let bad =
            || format!("invalid --synth-p '{v}' (expected fitted|table|gain,w2,r2,shape,cutoff)");
        let mut m = [0i32; 5];
        let mut it = v.split(',');
        for slot in &mut m {
            let x: f32 = it
                .next()
                .ok_or_else(bad)?
                .trim()
                .parse()
                .map_err(|_| bad())?;
            *slot = (x * 1000.0).round() as i32;
        }
        if it.next().is_some() {
            return Err(bad());
        }
        Ok(PSpec::Custom(m))
    }

    /// The block this selection stands for.
    pub fn block(self) -> PBlock {
        match self {
            PSpec::Fitted => PBlock::FITTED,
            PSpec::Custom(m) => PBlock::from_milli(m),
            // the single-block view of a table is the idle block; use `table()`
            PSpec::Table(_) => PBlock::IDLE,
        }
    }

    /// The per-wave table when this selection is one.
    pub fn table(self) -> Option<PTable> {
        match self {
            PSpec::Table(t) => Some(t.table()),
            _ => None,
        }
    }
}

// EMPIRICAL (all five): grid fit of the Smooth Lead clip (Nord Stage 4 demo Part A, A:87, 156
// confirmed notes) through the strip chain, first half (0 to 11 s) fitted, second half (11 to 22 s)
// held out; `docs/research/synth-p-fit.md` section 5. Score (mean |delta| of low, mid, presence,
// brilliance): 0.30 dB fit half, 1.83 dB hold-out half; presence +0.5 / -1.0 dB, RMS -0.6 / +1.0 dB.
// The optimum is flat and noisy in brilliance (+-4 dB between neighbouring cells), see the doc.
/// Output gain: -4.4 dB (the +4.5 dB RMS delta of unit gain, re-fitted once the cell is fixed).
const FIT_GAIN: f32 = 0.6;
/// Second oscillator weight (0.35 .. 0.5 within 1.5 dB of the best score on the fit half).
const FIT_WEIGHT2: f32 = 0.35;
/// Octave up (cells with ratio 1.5 and 1.006 scored 0.4 to 1.2 dB worse on the fit half).
const FIT_RATIO2: f32 = 2.0;
/// The idle shape word 0.5 (0.35 .. 0.7 within 0.5 dB at unit gain).
const FIT_SHAPE: f32 = 0.5;
/// Cutoff factor before the strip law (1.5 .. 1.75; the strip-path doc s4 what-if is 1.35).
const FIT_CUTOFF_SCALE: f32 = 1.75;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_oscillator_is_the_gain_times_the_first() {
        let mut ph = [0.0f32; SUPERSAW_VOICES];
        let p = PBlock {
            gain: 0.5,
            ..PBlock::IDLE
        };
        assert_eq!(p.mix(0.8, 0.01, 440.0, &mut ph, |_, _, _| 9.0), 0.4);
        assert_eq!(ph[0], 0.0, "the second phase stays untouched");
    }

    #[test]
    fn second_oscillator_runs_on_its_own_phase_at_the_ratio() {
        let mut ph = [0.0f32; SUPERSAW_VOICES];
        let p = PBlock {
            weight2: 0.5,
            ratio2: 2.0,
            ..PBlock::IDLE
        };
        let mut seen = (0.0, 0.0, 0.0);
        let y = p.mix(1.0, 0.01, 100.0, &mut ph, |t, d, f| {
            seen = (t, d, f);
            2.0
        });
        assert_eq!(y, 2.0);
        assert_eq!(seen, (0.0, 0.02, 200.0));
        assert!((ph[0] - 0.02).abs() < 1e-7);
    }

    #[test]
    fn single_keeps_the_shape_and_cutoff_but_drops_gain_and_second_oscillator() {
        let p = PBlock::from_milli([600, 500, 2000, 500, 1500]).single();
        assert_eq!((p.gain, p.weight2), (1.0, 0.0));
        assert_eq!((p.shape, p.cutoff_scale, p.ratio2), (0.5, 1.5, 2.0));
    }

    #[test]
    fn milli_form_clamps_and_scales() {
        let p = PBlock::from_milli([1500, 250, 2000, 800, 1400]);
        assert_eq!(p.gain, 1.5);
        assert_eq!(p.weight2, 0.25);
        assert_eq!(p.ratio2, 2.0);
        assert_eq!(p.shape, 0.8);
        assert_eq!(p.cutoff_scale, 1.4);
        let q = PBlock::from_milli([-5, 9000, 0, 5000, 0]);
        assert_eq!((q.gain, q.weight2, q.ratio2), (0.0, 4.0, 0.05));
        assert_eq!((q.shape, q.cutoff_scale), (1.0, 0.1));
    }

    #[test]
    fn spec_parses_fitted_and_custom_and_rejects_the_rest() {
        assert_eq!(PSpec::parse("Fitted"), Ok(PSpec::Fitted));
        let c = PSpec::parse("1.5, 0.25,2,0.8,1.4").unwrap();
        assert_eq!(c, PSpec::Custom([1500, 250, 2000, 800, 1400]));
        assert_eq!(c.block(), PBlock::from_milli([1500, 250, 2000, 800, 1400]));
        assert_eq!(PSpec::Fitted.block(), PBlock::FITTED);
        for bad in ["", "1,2,3", "a,b,c,d,e", "1,2,3,4,5,6"] {
            assert!(PSpec::parse(bad).is_err(), "{bad}");
        }
    }

    fn render_rms(pb: Option<PBlock>, shape_cat: bool) -> f32 {
        use super::super::{SynthChain, SynthLayer, VaWaveform};
        let mut s = SynthLayer::new(48_000.0);
        s.chain = SynthChain::FirmwareStrip;
        s.filter_on = false;
        s.waveform = if shape_cat {
            VaWaveform::Triangle
        } else {
            VaWaveform::Saw
        };
        s.osc_shape_category = shape_cat;
        s.p_block = pb;
        s.p_table = None;
        s.strip_stages = false;
        s.note_on(60, 127);
        let mut acc = 0.0f64;
        for i in 0..24_000 {
            let [l, _] = s.render_frame();
            if i >= 12_000 {
                acc += f64::from(l * l);
            }
        }
        (acc / 12_000.0).sqrt() as f32
    }

    #[test]
    fn gain_scales_the_shape_stage_output_and_none_is_unchanged() {
        let base = render_rms(None, true);
        let idle = render_rms(Some(PBlock::IDLE), true);
        let half = render_rms(
            Some(PBlock {
                gain: 0.5,
                shape: 0.5,
                ..PBlock::IDLE
            }),
            true,
        );
        // with the block the shape category plays the stage (shape 0.5) instead of the saw
        assert!(base > 1e-4 && idle > 1e-4, "the notes sound: {base} {idle}");
        assert!((half / idle - 0.5).abs() < 1e-3, "{half} vs {idle}");
    }

    #[test]
    fn a_second_oscillator_adds_energy_on_the_shape_stage_only() {
        let wide = PBlock {
            weight2: 0.5,
            ratio2: 1.5,
            ..PBlock::IDLE
        };
        let one = render_rms(Some(PBlock::IDLE), true);
        assert!(render_rms(Some(wide), true) > one * 1.05);
        // the Pure Saw stage ignores gain and second oscillator (EMPIRICAL, `single`)
        let saw = render_rms(None, false);
        let saw_p = render_rms(Some(PBlock { gain: 0.3, ..wide }), false);
        assert!((saw_p / saw - 1.0).abs() < 1e-5, "{saw_p} vs {saw}");
    }

    #[test]
    fn fitted_block_is_inside_the_physical_ranges() {
        let p = PBlock::FITTED;
        assert!(p.gain > 0.0 && p.gain < 8.0);
        assert!((0.0..=1.0).contains(&p.weight2));
        assert!((0.05..=8.0).contains(&p.ratio2));
        assert!((-1.0..=1.0).contains(&p.shape));
        assert!((0.25..=4.0).contains(&p.cutoff_scale));
    }
}
