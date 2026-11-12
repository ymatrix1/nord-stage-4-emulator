//! Host-automatable morph assignments (Wheel / Pedal / Aftertouch depth per target).
//!
//! Mirrors `nord_core::morph::MorphParameter` deltas (`-1.0..=1.0`) for the 7 layer
//! levels and the 3 synth cutoff morph slots of `Stage4Engine` (spec: `docs/specs/morph-and-zones.md`).
//! ASSUMED: the host range `-1.0..=1.0` and default `0.0` follow the engine's delta domain;
//! hardware morph depth scaling is not yet verified against a Nord Stage 4 (see OPEN-QUESTIONS).

use nih_plug::prelude::*;
use nord_core::engine::Stage4Engine;
use nord_core::morph::MorphParameter;

/// Number of layer-level morph targets (Organ A/B, Piano A/B, Synth A/B/C).
pub const LAYER_TARGETS: usize = 7;
/// Number of synth cutoff morph targets (Synth A/B/C).
pub const CUTOFF_TARGETS: usize = 3;

/// Three signed morph depths for one morph target.
#[derive(Params)]
pub struct MorphDeltaParams {
    #[id = "wheel"]
    pub wheel: FloatParam,
    #[id = "pedal"]
    pub pedal: FloatParam,
    #[id = "at"]
    pub aftertouch: FloatParam,
}

fn delta_param(name: &str, default: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Linear {
            min: -1.0,
            max: 1.0,
        },
    )
}

impl Default for MorphDeltaParams {
    fn default() -> Self {
        Self::with_defaults(0.0, 0.0, 0.0)
    }
}

impl MorphDeltaParams {
    /// Builds depth params with explicit default depths (each clamped by the host range).
    pub fn with_defaults(wheel: f32, pedal: f32, aftertouch: f32) -> Self {
        Self {
            wheel: delta_param("Morph Wheel", wheel),
            pedal: delta_param("Morph Pedal", pedal),
            aftertouch: delta_param("Morph Aftertouch", aftertouch),
        }
    }

    /// Writes the three host depths into an engine morph parameter, leaving `base` untouched.
    pub fn apply_to(&self, target: &mut MorphParameter) {
        target.delta_wheel = self.wheel.value();
        target.delta_pedal = self.pedal.value();
        target.delta_aftertouch = self.aftertouch.value();
    }
}

/// Morph depths for the 7 layer levels (IDs `..._1`..`_7`).
#[derive(Params)]
pub struct LayerLevelMorph {
    #[nested(array, group = "Layer")]
    pub targets: [MorphDeltaParams; LAYER_TARGETS],
}

/// Morph depths for the 3 synth cutoff slots (IDs `..._1`..`_3`).
#[derive(Params)]
pub struct SynthCutoffMorph {
    #[nested(array, group = "Synth")]
    pub targets: [MorphDeltaParams; CUTOFF_TARGETS],
}

/// All morph assignments exposed to the host.
#[derive(Params)]
pub struct MorphAssignParams {
    #[nested(id_prefix = "level", group = "Morph Layer Level")]
    pub layer_level: LayerLevelMorph,
    #[nested(id_prefix = "cutoff", group = "Morph Synth Cutoff")]
    pub synth_cutoff: SynthCutoffMorph,
}

impl Default for MorphAssignParams {
    fn default() -> Self {
        Self {
            layer_level: LayerLevelMorph {
                targets: std::array::from_fn(|_| MorphDeltaParams::default()),
            },
            synth_cutoff: SynthCutoffMorph {
                targets: std::array::from_fn(|_| MorphDeltaParams::default()),
            },
        }
    }
}

impl MorphAssignParams {
    /// Copies every morph depth into the engine (no allocation; safe on the audio thread).
    pub fn sync_to_engine(&self, engine: &mut Stage4Engine) {
        for (p, t) in self
            .layer_level
            .targets
            .iter()
            .zip(engine.layer_levels.iter_mut())
        {
            p.apply_to(t);
        }
        for (p, t) in self
            .synth_cutoff
            .targets
            .iter()
            .zip(engine.synth_cutoff_morph.iter_mut())
        {
            p.apply_to(t);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_zero_and_leave_base_untouched() {
        let p = MorphAssignParams::default();
        let mut e = Stage4Engine::new(48_000.0);
        e.layer_levels[2].base = 0.37;
        e.layer_levels[2].delta_wheel = 0.9;
        p.sync_to_engine(&mut e);
        assert_eq!(e.layer_levels[2].base, 0.37);
        assert_eq!(e.layer_levels[2].delta_wheel, 0.0);
        assert_eq!(e.synth_cutoff_morph[1].delta_aftertouch, 0.0);
    }

    #[test]
    fn host_depths_reach_every_target() {
        let mut p = MorphAssignParams::default();
        p.layer_level.targets[6] = MorphDeltaParams::with_defaults(0.5, 0.0, 0.0);
        p.layer_level.targets[0] = MorphDeltaParams::with_defaults(0.0, -0.25, 0.0);
        p.synth_cutoff.targets[2] = MorphDeltaParams::with_defaults(0.0, 0.0, 1.0);
        let mut e = Stage4Engine::new(48_000.0);
        p.sync_to_engine(&mut e);
        assert_eq!(e.layer_levels[6].delta_wheel, 0.5);
        assert_eq!(e.layer_levels[0].delta_pedal, -0.25);
        assert_eq!(e.synth_cutoff_morph[2].delta_aftertouch, 1.0);
        assert_eq!(e.layer_levels[6].delta_pedal, 0.0);
    }

    #[test]
    fn evaluate_uses_synced_depths() {
        use nord_core::morph::MorphControllers;
        let mut t = MorphParameter::new(0.5);
        t.delta_wheel = 0.4;
        let c = MorphControllers {
            mod_wheel: 0.5,
            ..Default::default()
        };
        assert!((t.evaluate(&c) - 0.7).abs() < 1e-6);
    }

    #[test]
    fn array_ids_are_unique_and_complete() {
        let p = MorphAssignParams::default();
        let ids: Vec<String> = p.param_map().into_iter().map(|(id, _, _)| id).collect();
        assert_eq!(ids.len(), (LAYER_TARGETS + CUTOFF_TARGETS) * 3);
        let mut s = ids.clone();
        s.sort();
        s.dedup();
        assert_eq!(s.len(), ids.len());
    }
}
