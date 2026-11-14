//! Per-layer Mod 1 Master Clock division and Reverb VARIATION (manual p.50, p.52-53).
//!
//! IDs are additive: `fxz_<name>_1..7`, same layer order as `fx_*` (`engine.fx_chains`: Organ A,
//! Organ B, Piano A, Piano B, Synth A, B, C). Defaults equal `LayerFxChain::new` (free Mod 1 rate,
//! no reverb variation), so an untouched host leaves sounds unchanged.

use nih_plug::prelude::*;
use nord_core::engine::Stage4Engine;

use crate::fx_params::FX_LAYERS;
use crate::fx_sharing_params::SyncDivisionSel;

/// Mod 1 clock division and Reverb variation of one layer.
#[derive(Params)]
pub struct LayerFxVariationParams {
    /// Master Clock division of the Mod 1 rate (A-Pan, Trem, Wah, Pump); `Use Division` keeps the
    /// free Rate knob. Same 17 fractions as the delay (`fxy_delay_beats_*`).
    #[id = "mod1_beats"]
    pub mod1_beats: EnumParam<SyncDivisionSel>,
    /// Reverb VARIATION: longer Spring tail, longer Booth / Room pre-delay.
    #[id = "rev_var"]
    pub reverb_variation: BoolParam,
}

impl Default for LayerFxVariationParams {
    fn default() -> Self {
        Self {
            mod1_beats: EnumParam::new("Mod 1 Clock Division", SyncDivisionSel::Off),
            reverb_variation: BoolParam::new("Reverb Variation", false),
        }
    }
}

/// All seven layers.
#[derive(Params)]
pub struct LayerFxVariationBank {
    #[nested(array, group = "Layer FX Variation")]
    pub layers: [LayerFxVariationParams; FX_LAYERS],
}

impl Default for LayerFxVariationBank {
    fn default() -> Self {
        Self {
            layers: std::array::from_fn(|_| LayerFxVariationParams::default()),
        }
    }
}

impl LayerFxVariationBank {
    /// Copies every layer's values into the engine (no allocation; audio-thread safe).
    pub fn sync_to_engine(&self, engine: &mut Stage4Engine) {
        for (p, fx) in self.layers.iter().zip(engine.fx_chains.iter_mut()) {
            fx.mod1_sync_beats = p.mod1_beats.value().beats();
            fx.reverb_variation = p.reverb_variation.value();
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
        // Perturb first so the test fails if sync writes nothing.
        for fx in e.fx_chains.iter_mut() {
            fx.mod1_sync_beats = Some(1.0);
            fx.reverb_variation = true;
        }
        LayerFxVariationBank::default().sync_to_engine(&mut e);
        for (a, b) in e.fx_chains.iter().zip(reference.fx_chains.iter()) {
            assert_eq!(a.mod1_sync_beats, b.mod1_sync_beats);
            assert_eq!(a.reverb_variation, b.reverb_variation);
        }
    }

    #[test]
    fn values_reach_only_their_layer() {
        let mut bank = LayerFxVariationBank::default();
        bank.layers[5].mod1_beats = EnumParam::new("m", SyncDivisionSel::Quarter);
        bank.layers[5].reverb_variation = BoolParam::new("r", true);
        let mut e = Stage4Engine::new(48_000.0);
        bank.sync_to_engine(&mut e);
        assert_eq!(e.fx_chains[5].mod1_sync_beats, Some(1.0));
        assert!(e.fx_chains[5].reverb_variation);
        assert_eq!(e.fx_chains[4].mod1_sync_beats, None);
        assert!(!e.fx_chains[4].reverb_variation);
    }
}
