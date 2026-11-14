//! Per-layer Piano String Resonance and Pedal Noise switches (Phase 6b B9c, `ns4-h7he`;
//! manual p.23-26). The engine exposes these as booleans per layer (no amount), so they are
//! switches. The effective value is the global `piano_string_res` / `piano_pedal_noise` AND the
//! layer switch, so an old session (layer defaults on) behaves exactly as before.
//!
//! IDs are additive: `pianoy_<name>_1` (Piano A) and `pianoy_<name>_2` (Piano B).

use nih_plug::prelude::*;
use nord_core::engine::Stage4Engine;
use nord_core::piano::PianoLayer;

/// Number of piano layers (A and B).
pub const PIANO_EXTRA_LAYERS: usize = 2;

/// Parameters of one piano layer.
#[derive(Params)]
pub struct PianoExtraLayerParams {
    /// Sympathetic string resonance for this layer (engine default on).
    #[id = "string_res"]
    pub string_res: BoolParam,
    /// Pedal noise for this layer (engine default on).
    #[id = "pedal_noise"]
    pub pedal_noise: BoolParam,
}

impl Default for PianoExtraLayerParams {
    fn default() -> Self {
        Self {
            string_res: BoolParam::new("Layer String Resonance", true),
            pedal_noise: BoolParam::new("Layer Pedal Noise", true),
        }
    }
}

/// Both piano layers.
#[derive(Params)]
pub struct PianoExtraBank {
    #[nested(array, group = "Piano Layer Extra")]
    pub layers: [PianoExtraLayerParams; PIANO_EXTRA_LAYERS],
}

impl Default for PianoExtraBank {
    fn default() -> Self {
        Self {
            layers: std::array::from_fn(|_| PianoExtraLayerParams::default()),
        }
    }
}

impl PianoExtraLayerParams {
    fn apply(&self, t: &mut PianoLayer) {
        t.string_resonance_enabled &= self.string_res.value();
        t.pedal_noise_enabled &= self.pedal_noise.value();
    }
}

impl PianoExtraBank {
    /// AND the layer switches into the engine; call after the global piano block.
    pub fn sync_to_engine(&self, engine: &mut Stage4Engine) {
        self.layers[0].apply(&mut engine.piano_a);
        self.layers[1].apply(&mut engine.piano_b);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bank(string_res: bool, pedal_noise: bool) -> PianoExtraBank {
        let mut bank = PianoExtraBank::default();
        bank.layers[0].string_res = BoolParam::new("s", string_res);
        bank.layers[0].pedal_noise = BoolParam::new("p", pedal_noise);
        bank
    }

    fn render_pedal(bank: &PianoExtraBank, note: bool) -> Vec<[f32; 2]> {
        let mut engine = Stage4Engine::new(44100.0);
        bank.sync_to_engine(&mut engine);
        let p = &mut engine.piano_a;
        if note {
            p.note_on(60, 100);
        }
        p.set_damper_pedal(true, 110);
        (0..4096).map(|_| p.render_frame()).collect()
    }

    #[test]
    fn defaults_leave_the_engine_unchanged() {
        let mut engine = Stage4Engine::new(44100.0);
        PianoExtraBank::default().sync_to_engine(&mut engine);
        assert!(engine.piano_a.string_resonance_enabled && engine.piano_b.pedal_noise_enabled);
    }

    #[test]
    fn layer_switch_wins_only_for_its_layer() {
        let mut engine = Stage4Engine::new(44100.0);
        bank(false, false).sync_to_engine(&mut engine);
        assert!(!engine.piano_a.string_resonance_enabled && !engine.piano_a.pedal_noise_enabled);
        assert!(engine.piano_b.string_resonance_enabled && engine.piano_b.pedal_noise_enabled);
    }

    #[test]
    fn pedal_noise_changes_rendered_audio() {
        let on = render_pedal(&PianoExtraBank::default(), false);
        let muted = render_pedal(&bank(true, false), false);
        assert!(on
            .iter()
            .chain(&muted)
            .all(|f| f[0].is_finite() && f[1].is_finite()));
        assert!(
            on.iter().any(|f| f[0].abs() > 1e-6),
            "pedal noise is silent"
        );
        assert!(muted.iter().all(|f| f[0] == 0.0 && f[1] == 0.0));
    }

    #[test]
    fn string_resonance_changes_rendered_audio() {
        let on = render_pedal(&PianoExtraBank::default(), true);
        let muted = render_pedal(&bank(false, true), true);
        assert!(on.iter().any(|f| f[0].abs() > 1e-6), "piano is silent");
        assert!(on != muted, "string resonance off renders identically");
    }

    #[test]
    fn ids_follow_the_additive_scheme() {
        let ids: Vec<String> = PianoExtraBank::default()
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(ids.len(), 2 * PIANO_EXTRA_LAYERS);
        assert!(ids.contains(&"string_res_1".to_string()));
        assert!(ids.contains(&"pedal_noise_2".to_string()));
    }
}
