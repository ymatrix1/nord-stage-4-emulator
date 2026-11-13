//! Per-layer Piano parameters that the panel exposes but the original shared
//! block lacked (Phase 6b B9c, `ns4-h7he`): SOFT RELEASE, UNISON, DYN COMP and
//! OCTAVE SHIFT (manual p.23-26).
//!
//! IDs are additive: `pianox_<name>_1` (Piano A) and `pianox_<name>_2` (Piano B).

use nih_plug::prelude::*;
use nord_core::engine::Stage4Engine;

/// Number of piano layers (A and B).
pub const PIANO_LAYERS: usize = 2;

/// Parameters of one piano layer.
#[derive(Params)]
pub struct PianoLayerParams {
    /// Longer, softer release (manual p.25).
    #[id = "soft_release"]
    pub soft_release: BoolParam,
    /// Unison 0 = Off, 1..=3 (manual p.26).
    #[id = "unison"]
    pub unison: IntParam,
    /// Dynamic compression 0 = Off, 1..=3 (manual p.25).
    #[id = "dyn_comp"]
    pub dyn_comp: IntParam,
    /// Octave shift in octaves, `-2..=2` (engine range; manual p.23).
    #[id = "octave"]
    pub octave: IntParam,
}

impl Default for PianoLayerParams {
    fn default() -> Self {
        Self {
            soft_release: BoolParam::new("Soft Release", false),
            unison: IntParam::new("Unison", 0, IntRange::Linear { min: 0, max: 3 }),
            dyn_comp: IntParam::new("Dyn Comp", 0, IntRange::Linear { min: 0, max: 3 }),
            octave: IntParam::new("Octave Shift", 0, IntRange::Linear { min: -2, max: 2 }),
        }
    }
}

/// Both piano layers.
#[derive(Params)]
pub struct PianoLayerBank {
    #[nested(array, group = "Piano Layer")]
    pub layers: [PianoLayerParams; PIANO_LAYERS],
}

impl Default for PianoLayerBank {
    fn default() -> Self {
        Self {
            layers: std::array::from_fn(|_| PianoLayerParams::default()),
        }
    }
}

impl PianoLayerBank {
    /// Copy every value into the engine (no allocation; audio-thread safe).
    pub fn sync_to_engine(&self, engine: &mut Stage4Engine) {
        let targets = [&mut engine.piano_a, &mut engine.piano_b];
        for (p, t) in self.layers.iter().zip(targets) {
            t.soft_release = p.soft_release.value();
            t.unison = p.unison.value() as u8;
            t.dyn_comp = p.dyn_comp.value() as u8;
            t.octave_shift = p.octave.value() as i8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_leave_the_engine_unchanged() {
        let mut engine = Stage4Engine::new(44100.0);
        PianoLayerBank::default().sync_to_engine(&mut engine);
        assert!(!engine.piano_a.soft_release);
        assert_eq!(engine.piano_a.unison, 0);
        assert_eq!(engine.piano_b.dyn_comp, 0);
        assert_eq!(engine.piano_b.octave_shift, 0);
    }

    #[test]
    fn values_reach_the_matching_layer_only() {
        let mut bank = PianoLayerBank::default();
        let range = |min, max| IntRange::Linear { min, max };
        bank.layers[1].unison = IntParam::new("Unison", 2, range(0, 3));
        bank.layers[1].dyn_comp = IntParam::new("Dyn Comp", 3, range(0, 3));
        bank.layers[1].octave = IntParam::new("Octave Shift", -1, range(-2, 2));
        bank.layers[1].soft_release = BoolParam::new("Soft Release", true);
        let mut engine = Stage4Engine::new(44100.0);
        bank.sync_to_engine(&mut engine);
        assert_eq!(engine.piano_b.unison, 2);
        assert_eq!(engine.piano_b.dyn_comp, 3);
        assert_eq!(engine.piano_b.octave_shift, -1);
        assert!(engine.piano_b.soft_release);
        assert_eq!(engine.piano_a.unison, 0);
        assert!(!engine.piano_a.soft_release);
    }

    #[test]
    fn ids_follow_the_additive_scheme() {
        let bank = PianoLayerBank::default();
        let ids: Vec<String> = bank.param_map().into_iter().map(|(id, _, _)| id).collect();
        assert_eq!(ids.len(), 8);
        assert!(ids.contains(&"soft_release_1".to_string()));
        assert!(ids.contains(&"octave_2".to_string()));
    }
}
