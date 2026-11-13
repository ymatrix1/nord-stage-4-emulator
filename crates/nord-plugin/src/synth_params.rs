//! Per-layer Synth parameters for the VOICE, FILTER and OSCILLATORS panel
//! areas (Phase 6b B9a, `ns4-4w9l`; manual p.31-35).
//!
//! IDs are additive: `synthx_<name>_1..3` for Synth A, B and C. The original
//! shared `synth_*` ids stay as they are (D16).

use nih_plug::prelude::*;
use nord_core::engine::Stage4Engine;
use nord_core::synth_types::NotePriority;

/// Number of synth layers (A, B, C).
pub const SYNTH_LAYERS: usize = 3;

/// Parameters of one synth layer.
#[derive(Params)]
pub struct SynthLayerParams {
    /// Unison 0 = Off, 1..=3 (manual p.35).
    #[id = "unison"]
    pub unison: IntParam,
    /// Voice mode: 0 = Poly, 1 = Mono, 2 = Legato (manual p.35).
    #[id = "voice"]
    pub voice: IntParam,
    /// Note priority for Mono/Legato: 0 = Last, 1 = Lo, 2 = Hi (manual p.35).
    #[id = "priority"]
    pub priority: IntParam,
    /// Glide time in seconds per octave; 0 = Off (scale `ASSUMED`, see `SynthLayer::glide_sec`).
    #[id = "glide"]
    pub glide: FloatParam,
    /// Filter keyboard tracking: 0 = Off, 1 = 1/3, 2 = 2/3, 3 = 1/1 (manual p.32).
    #[id = "kb_track"]
    pub kb_track: IntParam,
    /// Filter envelope amount, bipolar (manual p.32).
    #[id = "filter_env"]
    pub filter_env_amount: FloatParam,
    /// Oscillator envelope amount, bipolar (manual p.33).
    #[id = "osc_env"]
    pub osc_env_amount: FloatParam,
    /// Oscillator envelope targets pitch instead of Osc Ctrl (manual p.33).
    #[id = "osc_env_pitch"]
    pub osc_env_to_pitch: BoolParam,
}

impl Default for SynthLayerParams {
    fn default() -> Self {
        let int = |name, max| IntParam::new(name, 0, IntRange::Linear { min: 0, max });
        let bipolar = |name| {
            FloatParam::new(
                name,
                0.0,
                FloatRange::Linear {
                    min: -1.0,
                    max: 1.0,
                },
            )
            .with_step_size(0.01)
        };
        Self {
            unison: int("Unison", 3),
            voice: int("Voice Mode", 2),
            priority: int("Note Priority", 2),
            glide: FloatParam::new(
                "Glide",
                0.0,
                FloatRange::Skewed {
                    min: 0.0,
                    max: 4.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(" s/oct"),
            kb_track: int("KB Track", 3),
            filter_env_amount: bipolar("Filter Env Amt"),
            osc_env_amount: bipolar("Osc Env Amt"),
            osc_env_to_pitch: BoolParam::new("Osc Env To Pitch", false),
        }
    }
}

/// All three synth layers.
#[derive(Params)]
pub struct SynthLayerBank {
    #[nested(array, group = "Synth Layer")]
    pub layers: [SynthLayerParams; SYNTH_LAYERS],
}

impl Default for SynthLayerBank {
    fn default() -> Self {
        Self {
            layers: std::array::from_fn(|_| SynthLayerParams::default()),
        }
    }
}

impl SynthLayerBank {
    /// Copy every value into the engine (no allocation; audio-thread safe).
    pub fn sync_to_engine(&self, engine: &mut Stage4Engine) {
        let targets = [
            &mut engine.synth_a,
            &mut engine.synth_b,
            &mut engine.synth_c,
        ];
        for (p, t) in self.layers.iter().zip(targets) {
            t.unison = p.unison.value() as u8;
            let voice = p.voice.value();
            t.mono = voice >= 1;
            t.legato = voice == 2;
            t.note_priority = match p.priority.value() {
                1 => NotePriority::Lo,
                2 => NotePriority::Hi,
                _ => NotePriority::Off,
            };
            t.glide_sec = p.glide.value();
            t.kb_track = p.kb_track.value() as u8;
            t.filter_env_amount = p.filter_env_amount.value();
            t.osc_env_amount = p.osc_env_amount.value();
            t.osc_env_to_pitch = p.osc_env_to_pitch.value();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_engine_defaults() {
        let mut engine = Stage4Engine::new(44100.0);
        let before = (
            engine.synth_a.unison,
            engine.synth_a.mono,
            engine.synth_a.kb_track,
            engine.synth_a.filter_env_amount,
        );
        SynthLayerBank::default().sync_to_engine(&mut engine);
        let after = (
            engine.synth_a.unison,
            engine.synth_a.mono,
            engine.synth_a.kb_track,
            engine.synth_a.filter_env_amount,
        );
        assert_eq!(before.0, after.0);
        assert_eq!(before.1, after.1);
        assert_eq!(before.3, after.3);
        assert_eq!(before.2, after.2);
    }

    #[test]
    fn values_reach_the_matching_layer_only() {
        let mut bank = SynthLayerBank::default();
        bank.layers[2].voice = IntParam::new("v", 2, IntRange::Linear { min: 0, max: 2 });
        bank.layers[2].priority = IntParam::new("p", 1, IntRange::Linear { min: 0, max: 2 });
        bank.layers[2].unison = IntParam::new("u", 3, IntRange::Linear { min: 0, max: 3 });
        bank.layers[2].osc_env_to_pitch = BoolParam::new("e", true);
        let mut engine = Stage4Engine::new(44100.0);
        bank.sync_to_engine(&mut engine);
        assert!(engine.synth_c.mono && engine.synth_c.legato);
        assert_eq!(engine.synth_c.note_priority, NotePriority::Lo);
        assert_eq!(engine.synth_c.unison, 3);
        assert!(engine.synth_c.osc_env_to_pitch);
        assert!(!engine.synth_a.mono && !engine.synth_a.osc_env_to_pitch);
    }

    #[test]
    fn mono_voice_is_not_legato() {
        let mut bank = SynthLayerBank::default();
        bank.layers[0].voice = IntParam::new("v", 1, IntRange::Linear { min: 0, max: 2 });
        let mut engine = Stage4Engine::new(44100.0);
        bank.sync_to_engine(&mut engine);
        assert!(engine.synth_a.mono);
        assert!(!engine.synth_a.legato);
    }

    fn render(engine: &mut Stage4Engine) -> Vec<[f32; 2]> {
        engine.synth_a.note_on(60, 100);
        (0..4096).map(|_| engine.synth_a.render_frame()).collect()
    }

    #[test]
    fn unison_param_changes_the_rendered_signal() {
        let mut off = Stage4Engine::new(44100.0);
        SynthLayerBank::default().sync_to_engine(&mut off);
        let a = render(&mut off);

        let mut bank = SynthLayerBank::default();
        bank.layers[0].unison = IntParam::new("u", 3, IntRange::Linear { min: 0, max: 3 });
        let mut on = Stage4Engine::new(44100.0);
        bank.sync_to_engine(&mut on);
        let b = render(&mut on);

        assert!(a
            .iter()
            .chain(&b)
            .all(|f| f[0].is_finite() && f[1].is_finite()));
        assert!(a.iter().any(|f| f[0].abs() > 1e-4), "unison off is silent");
        assert!(b.iter().any(|f| f[0].abs() > 1e-4), "unison 3 is silent");
        assert!(a != b, "unison 3 renders identically to unison off");
    }

    #[test]
    fn ids_follow_the_additive_scheme() {
        let ids: Vec<String> = SynthLayerBank::default()
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(ids.len(), 8 * SYNTH_LAYERS);
        assert!(ids.contains(&"unison_1".to_string()));
        assert!(ids.contains(&"osc_env_pitch_3".to_string()));
    }
}
