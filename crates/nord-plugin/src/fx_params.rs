//! Host-automatable per-layer FX controls (Amp/EQ, Compressor, Delay, Reverb mix).
//!
//! One `LayerFxParams` per layer (Organ A/B, Piano A/B, Synth A/B/C) mapped onto
//! `Stage4Engine::fx_chains[i]` (`nord_core::fx::LayerFxChain`).
//! Ranges: EQ +-16 dB = VERIFIED against the 7-bit signed field x 0.25 dB in
//! `nord_core::cbin::fx_preset` (`eq_bass_db`); delay 20..1500 ms = the hardware table
//! documented on `FxPreset::delay_time_ms`. Defaults equal `LayerFxChain::new` (EMPIRICAL: same
//! constants as the engine). Selector enums (cabinet, mod types, reverb model) are not yet exposed.

use nih_plug::prelude::*;
use nord_core::engine::Stage4Engine;
use nord_core::fx::LayerFxChain;

/// Number of layers with an independent FX chain.
pub const FX_LAYERS: usize = 7;

/// FX controls for one layer.
#[derive(Params)]
pub struct LayerFxParams {
    #[id = "drive"]
    pub drive: FloatParam,
    #[id = "eq_bass"]
    pub eq_bass_db: FloatParam,
    #[id = "eq_mid"]
    pub eq_mid_db: FloatParam,
    #[id = "eq_treble"]
    pub eq_treble_db: FloatParam,
    #[id = "comp_on"]
    pub comp_enabled: BoolParam,
    #[id = "comp_amt"]
    pub comp_amount: FloatParam,
    #[id = "delay_on"]
    pub delay_enabled: BoolParam,
    #[id = "delay_ms"]
    pub delay_time_ms: FloatParam,
    #[id = "delay_fb"]
    pub delay_feedback: FloatParam,
    #[id = "delay_mix"]
    pub delay_mix: FloatParam,
    #[id = "rev_mix"]
    pub reverb_mix: FloatParam,
}

fn unit(name: &str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
}

fn eq_db(name: &str) -> FloatParam {
    FloatParam::new(
        name,
        0.0,
        FloatRange::Linear {
            min: -16.0,
            max: 16.0,
        },
    )
    .with_unit(" dB")
}

impl Default for LayerFxParams {
    fn default() -> Self {
        Self {
            drive: unit("Drive", 0.0),
            eq_bass_db: eq_db("EQ Bass"),
            eq_mid_db: eq_db("EQ Mid"),
            eq_treble_db: eq_db("EQ Treble"),
            comp_enabled: BoolParam::new("Comp On", false),
            comp_amount: unit("Comp Amount", 0.0),
            delay_enabled: BoolParam::new("Delay On", false),
            delay_time_ms: FloatParam::new(
                "Delay Time",
                375.0,
                FloatRange::Skewed {
                    min: 20.0,
                    max: 1500.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_unit(" ms"),
            delay_feedback: unit("Delay Feedback", 0.35),
            delay_mix: unit("Delay Mix", 0.25),
            reverb_mix: unit("Reverb Mix", 0.15),
        }
    }
}

impl LayerFxParams {
    /// Writes host values into one engine FX chain (no allocation).
    pub fn apply_to(&self, fx: &mut LayerFxChain) {
        fx.drive_amount = self.drive.value();
        fx.eq_bass_db = self.eq_bass_db.value();
        fx.eq_mid_db = self.eq_mid_db.value();
        fx.eq_treble_db = self.eq_treble_db.value();
        fx.comp_enabled = self.comp_enabled.value();
        fx.comp_amount = self.comp_amount.value();
        fx.delay_enabled = self.delay_enabled.value();
        fx.delay_time_ms = self.delay_time_ms.value();
        fx.delay_feedback = self.delay_feedback.value();
        fx.delay_mix = self.delay_mix.value();
        fx.reverb_mix = self.reverb_mix.value();
    }
}

/// Per-layer FX parameters for all 7 layers (IDs `fx_<id>_1..7`).
#[derive(Params)]
pub struct LayerFxBank {
    #[nested(array, group = "Layer FX")]
    pub layers: [LayerFxParams; FX_LAYERS],
}

impl Default for LayerFxBank {
    fn default() -> Self {
        Self {
            layers: std::array::from_fn(|_| LayerFxParams::default()),
        }
    }
}

impl LayerFxBank {
    /// Copies every layer's FX controls into the engine.
    pub fn sync_to_engine(&self, engine: &mut Stage4Engine) {
        for (p, fx) in self.layers.iter().zip(engine.fx_chains.iter_mut()) {
            p.apply_to(fx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_engine_defaults() {
        let reference = Stage4Engine::new(48_000.0);
        let mut e = Stage4Engine::new(48_000.0);
        LayerFxBank::default().sync_to_engine(&mut e);
        for (a, b) in e.fx_chains.iter().zip(reference.fx_chains.iter()) {
            assert_eq!(a.drive_amount, b.drive_amount);
            assert_eq!(a.eq_mid_freq_hz, b.eq_mid_freq_hz);
            assert_eq!(a.comp_enabled, b.comp_enabled);
            assert_eq!(a.delay_enabled, b.delay_enabled);
            assert_eq!(a.delay_time_ms, b.delay_time_ms);
            assert_eq!(a.delay_feedback, b.delay_feedback);
            assert_eq!(a.delay_mix, b.delay_mix);
            assert_eq!(a.reverb_mix, b.reverb_mix);
        }
    }

    #[test]
    fn host_values_reach_only_their_layer() {
        let mut bank = LayerFxBank::default();
        bank.layers[4].delay_mix = unit("Delay Mix", 0.9);
        bank.layers[4].eq_bass_db = FloatParam::new(
            "b",
            -6.0,
            FloatRange::Linear {
                min: -16.0,
                max: 16.0,
            },
        );
        let mut e = Stage4Engine::new(48_000.0);
        bank.sync_to_engine(&mut e);
        assert_eq!(e.fx_chains[4].delay_mix, 0.9);
        assert_eq!(e.fx_chains[4].eq_bass_db, -6.0);
        assert_eq!(e.fx_chains[3].delay_mix, 0.25);
    }

    #[test]
    fn ids_unique_and_complete() {
        let ids: Vec<String> = LayerFxBank::default()
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(ids.len(), FX_LAYERS * 11);
        let mut s = ids.clone();
        s.sort();
        s.dedup();
        assert_eq!(s.len(), ids.len());
    }
}
