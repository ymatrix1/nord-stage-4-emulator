//! Per-layer Arpeggiator Zig Zag, fractional Range, custom Pattern (length, gate,
//! accent, pan per step; manual p.35-37).
//!
//! IDs are additive: `synthp_<name>_<layer 1..3>` and per step `synthp_step_{gate,accent,pan}_<step 1..16>_<layer>`.
//! Defaults equal `SynthLayer::new`, so an untouched host leaves sounds unchanged. The custom
//! pattern only plays while `pat_custom` is on together with the Pattern switch of `synthg`.

use nih_plug::prelude::*;
use nord_core::engine::Stage4Engine;
use nord_core::synth::{PatternPan, MAX_RANGE_EXTRA_THIRDS, PATTERN_MAX_STEPS};

use crate::synth_params::SYNTH_LAYERS;

/// One pattern step.
#[derive(Params)]
pub struct PatternStepParams {
    /// Gate length in eighths of a step (`0` = rest, `8` = full step).
    #[id = "step_gate"]
    pub gate: IntParam,
    /// Accent (velocity boost).
    #[id = "step_accent"]
    pub accent: BoolParam,
    /// Pan: 0 Left, 1 Center, 2 Right.
    #[id = "step_pan"]
    pub pan: IntParam,
}

impl Default for PatternStepParams {
    fn default() -> Self {
        Self {
            gate: IntParam::new("Step Gate", 4, IntRange::Linear { min: 0, max: 8 }),
            accent: BoolParam::new("Step Accent", false),
            pan: IntParam::new("Step Pan", 1, IntRange::Linear { min: 0, max: 2 }),
        }
    }
}

/// Zig Zag, Range fraction and custom pattern of one synth layer.
#[derive(Params)]
pub struct SynthPatternParams {
    /// Zig Zag direction: two steps forward, one back (manual p.36).
    #[id = "zigzag"]
    pub zigzag: BoolParam,
    /// Range fraction in thirds of an octave on top of the octave range (manual p.35).
    #[id = "range_frac"]
    pub range_frac: IntParam,
    /// Play the custom pattern instead of the preset.
    #[id = "pat_custom"]
    pub pat_custom: BoolParam,
    /// Custom pattern length `2..16` (manual p.36).
    #[id = "pat_len"]
    pub pat_len: IntParam,
    #[nested(array, group = "Steps")]
    pub steps: [PatternStepParams; PATTERN_MAX_STEPS],
}

impl Default for SynthPatternParams {
    fn default() -> Self {
        Self {
            zigzag: BoolParam::new("Zig Zag", false),
            range_frac: IntParam::new(
                "Range Fraction",
                0,
                IntRange::Linear {
                    min: 0,
                    max: i32::from(MAX_RANGE_EXTRA_THIRDS),
                },
            ),
            pat_custom: BoolParam::new("Custom Pattern", false),
            pat_len: IntParam::new("Pattern Length", 8, IntRange::Linear { min: 2, max: 16 }),
            steps: std::array::from_fn(|_| PatternStepParams::default()),
        }
    }
}

/// All three synth layers.
#[derive(Params)]
pub struct SynthPatternBank {
    #[nested(array, group = "Synth Pattern")]
    pub layers: [SynthPatternParams; SYNTH_LAYERS],
}

impl Default for SynthPatternBank {
    fn default() -> Self {
        Self {
            layers: std::array::from_fn(|_| SynthPatternParams::default()),
        }
    }
}

impl SynthPatternBank {
    /// Copy every value into the engine (no allocation; audio-thread safe).
    pub fn sync_to_engine(&self, engine: &mut Stage4Engine) {
        let targets = [
            &mut engine.synth_a,
            &mut engine.synth_b,
            &mut engine.synth_c,
        ];
        for (p, t) in self.layers.iter().zip(targets) {
            t.perf.zigzag = p.zigzag.value();
            t.perf.range_extra_thirds = p.range_frac.value() as u8;
            t.perf.pattern_custom = p.pat_custom.value();
            t.set_pattern_length(p.pat_len.value() as u8);
            for (i, step) in p.steps.iter().enumerate() {
                t.set_step_gate(i, step.gate.value() as u8);
                t.set_step_accent(i, step.accent.value());
                t.set_step_pan(i, PatternPan::from_index(step.pan.value()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_engine_defaults() {
        let reference = Stage4Engine::new(44100.0);
        let mut e = Stage4Engine::new(44100.0);
        SynthPatternBank::default().sync_to_engine(&mut e);
        for (a, b) in [
            (&e.synth_a, &reference.synth_a),
            (&e.synth_b, &reference.synth_b),
            (&e.synth_c, &reference.synth_c),
        ] {
            assert_eq!(a.perf.zigzag, b.perf.zigzag);
            assert_eq!(a.perf.range_extra_thirds, b.perf.range_extra_thirds);
            assert_eq!(a.perf.pattern_custom, b.perf.pattern_custom);
            assert_eq!(a.perf.custom, b.perf.custom);
        }
    }

    #[test]
    fn values_reach_only_their_layer_and_step() {
        let mut bank = SynthPatternBank::default();
        bank.layers[1].zigzag = BoolParam::new("t", true);
        bank.layers[1].range_frac = IntParam::new("r", 2, IntRange::Linear { min: 0, max: 2 });
        bank.layers[1].pat_len = IntParam::new("l", 5, IntRange::Linear { min: 2, max: 16 });
        bank.layers[1].steps[3].gate = IntParam::new("g", 7, IntRange::Linear { min: 0, max: 8 });
        bank.layers[1].steps[3].accent = BoolParam::new("a", true);
        bank.layers[1].steps[3].pan = IntParam::new("p", 2, IntRange::Linear { min: 0, max: 2 });
        let mut e = Stage4Engine::new(44100.0);
        bank.sync_to_engine(&mut e);
        assert!(e.synth_b.perf.zigzag && !e.synth_a.perf.zigzag);
        assert_eq!(e.synth_b.perf.range_extra_thirds, 2);
        assert_eq!(e.synth_b.pattern_length(), 5);
        assert_eq!(e.synth_b.perf.custom.gate[3], 7);
        assert!(e.synth_b.perf.custom.accent[3]);
        assert_eq!(e.synth_b.perf.custom.pan[3], PatternPan::Right);
        assert_eq!(e.synth_a.perf.custom.gate[3], 4);
    }

    #[test]
    fn ids_unique_and_complete() {
        let ids: Vec<String> = SynthPatternBank::default()
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(ids.len(), (4 + 3 * PATTERN_MAX_STEPS) * SYNTH_LAYERS);
        let mut s = ids.clone();
        s.sort();
        s.dedup();
        assert_eq!(s.len(), ids.len());
    }
}
