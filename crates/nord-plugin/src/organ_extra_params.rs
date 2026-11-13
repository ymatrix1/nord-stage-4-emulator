//! More per-layer Organ parameters the panel exposes (Phase 6b B9c, `ns4-h7he`; manual
//! p.18-22, p.53): per-layer model override, key click, Rotary on/off and mic angle,
//! Fine Tune, transformer crosstalk and detuned (vibrato) copies.
//!
//! IDs are additive: `organy_<name>_1` (Organ A) and `organy_<name>_2` (Organ B). Defaults
//! equal the `OrganLayer::new` defaults, so a fresh instance renders as before. These are applied
//! after the global Organ block so the per-layer model override can win over `organ_model`.

use nih_plug::prelude::*;
use nord_core::engine::Stage4Engine;
use nord_core::organ::{OrganLayer, OrganModel};

/// Number of organ layers (A and B).
pub const ORGAN_EXTRA_LAYERS: usize = 2;

/// Parameters of one organ layer.
#[derive(Params)]
pub struct OrganExtraLayerParams {
    /// Model override: -1 = follow the global `organ_model`, 0..=4 = B3, Vox, Farfisa, Pipe1, Pipe2.
    #[id = "model"]
    pub model: IntParam,
    /// Key click transient on key down/up (engine default on).
    #[id = "key_click"]
    pub key_click: BoolParam,
    /// Rotary speaker section active (engine default on).
    #[id = "rotary_on"]
    pub rotary_on: BoolParam,
    /// Rotary stereo mic separation in degrees. ASSUMED range/default (engine `120.0`).
    #[id = "mic_angle"]
    pub mic_angle: FloatParam,
    /// Fine tune in cents; the engine clamps to -400..+381.
    #[id = "fine_tune"]
    pub fine_tune: FloatParam,
    /// Transformer crosstalk intensity (`1.0` = nominal B3 bleed). ASSUMED upper bound 2.0.
    #[id = "crosstalk"]
    pub crosstalk: FloatParam,
    /// Detuned vibrato copies (firmware `DETUNE_CENTS` table).
    #[id = "detuned"]
    pub detuned: BoolParam,
}

impl Default for OrganExtraLayerParams {
    fn default() -> Self {
        Self {
            model: IntParam::new(
                "Organ Model Override",
                -1,
                IntRange::Linear { min: -1, max: 4 },
            ),
            key_click: BoolParam::new("Key Click", true),
            rotary_on: BoolParam::new("Rotary Active", true),
            mic_angle: FloatParam::new(
                "Rotary Mic Angle",
                120.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 180.0,
                },
            )
            .with_unit(" deg"),
            fine_tune: FloatParam::new(
                "Organ Fine Tune",
                0.0,
                FloatRange::Linear {
                    min: -400.0,
                    max: 381.0,
                },
            )
            .with_unit(" ct"),
            crosstalk: FloatParam::new("Crosstalk", 1.0, FloatRange::Linear { min: 0.0, max: 2.0 }),
            detuned: BoolParam::new("Detuned Copies", false),
        }
    }
}

/// Both organ layers.
#[derive(Params)]
pub struct OrganExtraBank {
    #[nested(array, group = "Organ Layer Extra")]
    pub layers: [OrganExtraLayerParams; ORGAN_EXTRA_LAYERS],
}

impl Default for OrganExtraBank {
    fn default() -> Self {
        Self {
            layers: std::array::from_fn(|_| OrganExtraLayerParams::default()),
        }
    }
}

/// Map the override index (`0..=4`) to a model; `None` for "follow global".
pub fn model_from_index(i: i32) -> Option<OrganModel> {
    match i {
        0 => Some(OrganModel::B3),
        1 => Some(OrganModel::Vox),
        2 => Some(OrganModel::Farfisa),
        3 => Some(OrganModel::Pipe1),
        4 => Some(OrganModel::Pipe2),
        _ => None,
    }
}

impl OrganExtraLayerParams {
    fn apply(&self, t: &mut OrganLayer) {
        if let Some(m) = model_from_index(self.model.value()) {
            t.model = m;
        }
        t.key_click_enabled = self.key_click.value();
        t.rotary_enabled = self.rotary_on.value();
        t.rotary_mic_angle_deg = self.mic_angle.value();
        t.fine_tune_cents = self.fine_tune.value();
        t.crosstalk_level = self.crosstalk.value();
        t.detuned_copies = self.detuned.value();
    }
}

impl OrganExtraBank {
    /// Copy every value into the engine (no allocation; audio-thread safe).
    pub fn sync_to_engine(&self, engine: &mut Stage4Engine) {
        self.layers[0].apply(&mut engine.organ_a);
        self.layers[1].apply(&mut engine.organ_b);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::organ::RotarySpeed;

    fn bank_with(f: impl Fn(&mut OrganExtraLayerParams)) -> OrganExtraBank {
        let mut bank = OrganExtraBank::default();
        f(&mut bank.layers[0]);
        bank
    }

    fn render(bank: &OrganExtraBank, setup: impl Fn(&mut OrganLayer)) -> Vec<[f32; 2]> {
        let mut engine = Stage4Engine::new(44100.0);
        bank.sync_to_engine(&mut engine);
        setup(&mut engine.organ_a);
        engine.organ_a.note_on(60);
        (0..8192).map(|_| engine.organ_a.render_frame()).collect()
    }

    fn assert_differs(a: &[[f32; 2]], b: &[[f32; 2]], what: &str) {
        assert!(a
            .iter()
            .chain(b)
            .all(|f| f[0].is_finite() && f[1].is_finite()));
        assert!(a.iter().any(|f| f[0].abs() > 1e-4), "organ is silent");
        assert!(a != b, "{what} renders identically");
    }

    fn float(v: f32, min: f32, max: f32) -> FloatParam {
        FloatParam::new("f", v, FloatRange::Linear { min, max })
    }

    #[test]
    fn defaults_leave_the_engine_unchanged() {
        let reference = Stage4Engine::new(44100.0).organ_a;
        let mut engine = Stage4Engine::new(44100.0);
        OrganExtraBank::default().sync_to_engine(&mut engine);
        let o = &engine.organ_b;
        assert_eq!(o.model, reference.model);
        assert_eq!(o.key_click_enabled, reference.key_click_enabled);
        assert_eq!(o.rotary_enabled, reference.rotary_enabled);
        assert_eq!(o.rotary_mic_angle_deg, reference.rotary_mic_angle_deg);
        assert_eq!(o.fine_tune_cents, reference.fine_tune_cents);
        assert_eq!(o.crosstalk_level, reference.crosstalk_level);
        assert_eq!(o.detuned_copies, reference.detuned_copies);
    }

    #[test]
    fn model_index_maps_and_minus_one_follows_global() {
        assert_eq!(model_from_index(-1), None);
        assert_eq!(model_from_index(0), Some(OrganModel::B3));
        assert_eq!(model_from_index(2), Some(OrganModel::Farfisa));
        assert_eq!(model_from_index(4), Some(OrganModel::Pipe2));
    }

    #[test]
    fn values_reach_the_matching_layer_only() {
        let mut bank = OrganExtraBank::default();
        bank.layers[1].model = IntParam::new("m", 2, IntRange::Linear { min: -1, max: 4 });
        bank.layers[1].detuned = BoolParam::new("d", true);
        bank.layers[1].key_click = BoolParam::new("k", false);
        let mut engine = Stage4Engine::new(44100.0);
        bank.sync_to_engine(&mut engine);
        assert_eq!(engine.organ_b.model, OrganModel::Farfisa);
        assert_eq!(engine.organ_a.model, OrganModel::B3);
        assert!(engine.organ_b.detuned_copies && !engine.organ_a.detuned_copies);
        assert!(!engine.organ_b.key_click_enabled && engine.organ_a.key_click_enabled);
    }

    #[test]
    fn key_click_changes_rendered_audio() {
        let off = bank_with(|p| p.key_click = BoolParam::new("k", false));
        let base = OrganExtraBank::default();
        assert_differs(
            &render(&base, |_| {}),
            &render(&off, |_| {}),
            "key click off",
        );
    }

    #[test]
    fn rotary_off_changes_rendered_audio() {
        let off = bank_with(|p| p.rotary_on = BoolParam::new("r", false));
        let base = OrganExtraBank::default();
        assert_differs(&render(&base, |_| {}), &render(&off, |_| {}), "rotary off");
    }

    #[test]
    fn mic_angle_changes_rendered_audio() {
        let fast = |o: &mut OrganLayer| o.rotary_speed = RotarySpeed::Fast;
        let wide = bank_with(|p| p.mic_angle = float(180.0, 0.0, 180.0));
        let narrow = bank_with(|p| p.mic_angle = float(0.0, 0.0, 180.0));
        assert_differs(&render(&wide, fast), &render(&narrow, fast), "mic angle");
    }

    /// Fine Tune and detuned copies live in the firmware oscillator bank (Pipe models).
    fn pipe(o: &mut OrganLayer) {
        o.model = OrganModel::Pipe1;
    }

    #[test]
    fn fine_tune_changes_rendered_audio() {
        let up = bank_with(|p| p.fine_tune = float(100.0, -400.0, 381.0));
        assert_differs(
            &render(&OrganExtraBank::default(), pipe),
            &render(&up, pipe),
            "fine tune",
        );
    }

    #[test]
    fn crosstalk_changes_rendered_audio() {
        let none = bank_with(|p| p.crosstalk = float(0.0, 0.0, 2.0));
        let two = bank_with(|p| p.crosstalk = float(2.0, 0.0, 2.0));
        assert_differs(&render(&none, |_| {}), &render(&two, |_| {}), "crosstalk");
    }

    #[test]
    fn detuned_copies_change_rendered_audio() {
        let on = bank_with(|p| p.detuned = BoolParam::new("d", true));
        assert_differs(
            &render(&OrganExtraBank::default(), pipe),
            &render(&on, pipe),
            "detuned copies",
        );
    }

    #[test]
    fn ids_follow_the_additive_scheme() {
        let ids: Vec<String> = OrganExtraBank::default()
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(ids.len(), 7 * ORGAN_EXTRA_LAYERS);
        assert!(ids.contains(&"model_1".to_string()));
        assert!(ids.contains(&"mic_angle_2".to_string()));
    }
}
