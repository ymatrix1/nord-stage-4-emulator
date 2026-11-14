//! Per-layer FX controls the panel has but the host lacked (Phase 6b B9b,
//! `ns4-e3zs`; manual p.49-52): MOD 1 / MOD 2 AMOUNT, EQ MID FREQ, filter RES,
//! Amp VARIATION, Delay ANALOG and Delay effect VARIATION.
//!
//! IDs are additive: `fxx_<name>_1..7`, same layer order as `fx_*`
//! (`engine.fx_chains`: Organ A, Organ B, Piano A, Piano B, Synth A, B, C).
//! Defaults equal `LayerFxChain::new`, so an untouched host changes nothing.

use nih_plug::prelude::*;
use nord_core::engine::Stage4Engine;

use crate::fx_params::FX_LAYERS;

/// Manual p.51: the MID FREQ knob sweeps 200 Hz to 8 kHz.
const MID_FREQ_MIN: f32 = 200.0;
const MID_FREQ_MAX: f32 = 8000.0;

/// Extra FX parameters of one layer.
#[derive(Params)]
pub struct LayerFxExtraParams {
    #[id = "mod1_amount"]
    pub mod1_amount: FloatParam,
    #[id = "mod2_amount"]
    pub mod2_amount: FloatParam,
    /// EQ mid sweep frequency (manual p.51: 200 Hz to 8 kHz).
    #[id = "eq_mid_freq"]
    pub eq_mid_freq: FloatParam,
    /// Resonance of the LP24/HP24 filters (manual p.52).
    #[id = "filter_res"]
    pub filter_res: FloatParam,
    /// Amp model VARIATION (manual p.51).
    #[id = "amp_variation"]
    pub amp_variation: BoolParam,
    /// Delay ANALOG mode (manual p.51).
    #[id = "delay_analog"]
    pub delay_analog: BoolParam,
    /// Delay feedback-effect VARIATION (manual p.51).
    #[id = "delay_fx_variation"]
    pub delay_fx_variation: BoolParam,
}

fn unit(name: &str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 }).with_step_size(0.01)
}

impl Default for LayerFxExtraParams {
    fn default() -> Self {
        Self {
            mod1_amount: unit("Mod 1 Amount", 0.4),
            mod2_amount: unit("Mod 2 Amount", 0.35),
            eq_mid_freq: FloatParam::new(
                "EQ Mid Freq",
                1000.0,
                FloatRange::Skewed {
                    min: MID_FREQ_MIN,
                    max: MID_FREQ_MAX,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_unit(" Hz"),
            filter_res: unit("Filter Res", 0.0),
            amp_variation: BoolParam::new("Amp Variation", false),
            delay_analog: BoolParam::new("Delay Analog", false),
            delay_fx_variation: BoolParam::new("Delay FX Variation", false),
        }
    }
}

/// All seven layers.
#[derive(Params)]
pub struct LayerFxExtraBank {
    #[nested(array, group = "Layer FX Extra")]
    pub layers: [LayerFxExtraParams; FX_LAYERS],
}

impl Default for LayerFxExtraBank {
    fn default() -> Self {
        Self {
            layers: std::array::from_fn(|_| LayerFxExtraParams::default()),
        }
    }
}

impl LayerFxExtraBank {
    /// Copy every value into the engine (no allocation; audio-thread safe).
    pub fn sync_to_engine(&self, engine: &mut Stage4Engine) {
        for (p, fx) in self.layers.iter().zip(engine.fx_chains.iter_mut()) {
            fx.mod1_amount = p.mod1_amount.value();
            fx.mod2_amount = p.mod2_amount.value();
            fx.eq_mid_freq_hz = p.eq_mid_freq.value();
            fx.eq_filter_res = p.filter_res.value();
            fx.amp_variation = p.amp_variation.value();
            fx.delay_analog = p.delay_analog.value();
            fx.delay_fx_variation = p.delay_fx_variation.value();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::fx::{LayerFxChain, Mod1Type};

    #[test]
    fn defaults_match_engine_defaults() {
        let reference = Stage4Engine::new(48_000.0);
        let mut e = Stage4Engine::new(48_000.0);
        LayerFxExtraBank::default().sync_to_engine(&mut e);
        for (a, b) in e.fx_chains.iter().zip(reference.fx_chains.iter()) {
            assert_eq!(a.mod1_amount, b.mod1_amount);
            assert_eq!(a.mod2_amount, b.mod2_amount);
            assert_eq!(a.eq_mid_freq_hz, b.eq_mid_freq_hz);
            assert_eq!(a.eq_filter_res, b.eq_filter_res);
            assert_eq!(a.amp_variation, b.amp_variation);
            assert_eq!(a.delay_analog, b.delay_analog);
            assert_eq!(a.delay_fx_variation, b.delay_fx_variation);
        }
    }

    #[test]
    fn values_reach_only_their_layer() {
        let mut bank = LayerFxExtraBank::default();
        bank.layers[5].mod1_amount = unit("m", 0.9);
        bank.layers[5].amp_variation = BoolParam::new("v", true);
        bank.layers[5].delay_analog = BoolParam::new("a", true);
        bank.layers[5].eq_mid_freq = FloatParam::new(
            "f",
            4000.0,
            FloatRange::Linear {
                min: MID_FREQ_MIN,
                max: MID_FREQ_MAX,
            },
        );
        let mut e = Stage4Engine::new(48_000.0);
        bank.sync_to_engine(&mut e);
        assert_eq!(e.fx_chains[5].mod1_amount, 0.9);
        assert!(e.fx_chains[5].amp_variation && e.fx_chains[5].delay_analog);
        assert_eq!(e.fx_chains[5].eq_mid_freq_hz, 4000.0);
        assert_eq!(e.fx_chains[4].mod1_amount, 0.4);
        assert!(!e.fx_chains[4].amp_variation);
    }

    #[test]
    fn mod1_amount_changes_the_processed_signal() {
        let run = |amount: f32| {
            let mut bank = LayerFxExtraBank::default();
            bank.layers[0].mod1_amount = unit("m", amount);
            let mut e = Stage4Engine::new(48_000.0);
            bank.sync_to_engine(&mut e);
            let fx: &mut LayerFxChain = &mut e.fx_chains[0];
            fx.mod1_type = Mod1Type::Tremolo;
            fx.mod1_rate_hz = 5.0;
            (0..9600)
                .map(|n| {
                    let x = (n as f32 * 0.05).sin() * 0.5;
                    fx.process_frame([x, x])
                })
                .collect::<Vec<_>>()
        };
        let (low, high) = (run(0.0), run(1.0));
        assert!(low
            .iter()
            .chain(&high)
            .all(|f| f[0].is_finite() && f[1].is_finite()));
        assert!(low.iter().any(|f| f[0].abs() > 1e-3), "chain is silent");
        assert!(low != high, "mod1 amount has no effect");
    }

    #[test]
    fn ids_unique_and_complete() {
        let ids: Vec<String> = LayerFxExtraBank::default()
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(ids.len(), FX_LAYERS * 7);
        let mut s = ids.clone();
        s.sort();
        s.dedup();
        assert_eq!(s.len(), ids.len());
        assert!(ids.contains(&"eq_mid_freq_7".to_string()));
    }
}
