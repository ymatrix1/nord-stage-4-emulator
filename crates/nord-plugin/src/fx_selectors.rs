//! Host-automatable per-layer FX selectors (modulation types/rates, amp cabinet, compressor speed,
//! delay mode switches and feedback effect, reverb model/colour).
//!
//! Complements `fx_params.rs` (continuous controls). One `LayerFxSelectors` per layer maps onto
//! `Stage4Engine::fx_chains[i]` (`nord_core::fx::LayerFxChain`). The mirror enums below exist because
//! `#[derive(Enum)]` cannot be applied to `nord_core` types; variant order is the host-visible index
//! and must only ever be appended to. Names/order of the core enums: see `nord_core::fx`.
//!
//! ASSUMED: Mod rate ranges (0.1..20 Hz Mod 1, 0.1..10 Hz Mod 2, skewed). Chosen to cover the engine
//! defaults (4.0 / 0.8 Hz) with usable travel; confirm against the hardware rate table when decoded.
//! Defaults equal `LayerFxChain::new` and are checked by `defaults_match_engine_defaults`.

use nih_plug::prelude::*;
use nord_core::engine::Stage4Engine;
use nord_core::fx::{
    AmpCabinet, DelayFeedbackFx, DelayFilterMode, DelaySubdivision, LayerFxChain, Mod1Type,
    Mod2Type, ReverbColor, ReverbModel,
};

use crate::fx_params::FX_LAYERS;

macro_rules! mirror_enum {
    ($(#[$m:meta])* $name:ident => $core:ty { $($var:ident = $label:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name { $(#[name = $label] $var),+ }
        impl From<$name> for $core {
            fn from(v: $name) -> Self { match v { $($name::$var => <$core>::$var),+ } }
        }
    };
}

mirror_enum!(
    /// Host mirror of [`Mod1Type`].
    Mod1Sel => Mod1Type { Off = "Off", Tremolo = "Tremolo", AutoPan = "Auto Pan", RingMod = "Ring Mod", Wah = "Wah" }
);
mirror_enum!(
    /// Host mirror of [`Mod2Type`].
    Mod2Sel => Mod2Type { Off = "Off", Chorus = "Chorus", Flanger = "Flanger", Phaser = "Phaser", Vibe = "Vibe" }
);
mirror_enum!(
    /// Host mirror of [`AmpCabinet`].
    CabinetSel => AmpCabinet { Off = "Off", Small = "Small Speaker", Jc = "JC", Twin = "Twin", ToRotary = "To Rotary", LpFilter = "LP24", HpFilter = "HP24" }
);
mirror_enum!(
    /// Host mirror of [`DelaySubdivision`].
    SubdivisionSel => DelaySubdivision { Quarter = "1/4", DottedEighth = "Dotted 1/8", Eighth = "1/8", Sixteenth = "1/16" }
);
mirror_enum!(
    /// Host mirror of [`DelayFilterMode`].
    DelayFilterSel => DelayFilterMode { Off = "Off", Hp = "HP", Lp = "LP", Bp = "BP" }
);
mirror_enum!(
    /// Host mirror of [`DelayFeedbackFx`].
    DelayFxSel => DelayFeedbackFx { Off = "Bypass", Ensemble = "Ensemble", Vibe = "Vibe", Chorus = "Chorus", Flam = "Flam", Space = "Space" }
);
mirror_enum!(
    /// Host mirror of [`ReverbModel`].
    ReverbModelSel => ReverbModel { Off = "Off", Booth = "Booth", Room = "Room", Stage = "Stage", Spring = "Spring", Hall = "Hall", Cathedral = "Cathedral" }
);
mirror_enum!(
    /// Host mirror of [`ReverbColor`].
    ReverbColorSel => ReverbColor { Normal = "Normal", Bright = "Bright", Dark = "Dark", Chorale = "Chorale" }
);

/// Selector controls for one layer.
#[derive(Params)]
pub struct LayerFxSelectors {
    #[id = "mod1_type"]
    pub mod1_type: EnumParam<Mod1Sel>,
    #[id = "mod1_rate"]
    pub mod1_rate_hz: FloatParam,
    #[id = "mod2_type"]
    pub mod2_type: EnumParam<Mod2Sel>,
    #[id = "mod2_rate"]
    pub mod2_rate_hz: FloatParam,
    #[id = "cabinet"]
    pub cabinet: EnumParam<CabinetSel>,
    #[id = "comp_fast"]
    pub comp_fast: BoolParam,
    #[id = "delay_sync"]
    pub delay_sync: BoolParam,
    #[id = "delay_div"]
    pub delay_subdivision: EnumParam<SubdivisionSel>,
    #[id = "delay_pp"]
    pub delay_ping_pong: BoolParam,
    #[id = "delay_flt"]
    pub delay_filter: EnumParam<DelayFilterSel>,
    #[id = "delay_fx"]
    pub delay_fx: EnumParam<DelayFxSel>,
    #[id = "rev_model"]
    pub reverb_model: EnumParam<ReverbModelSel>,
    #[id = "rev_color"]
    pub reverb_color: EnumParam<ReverbColorSel>,
}

fn rate(name: &str, default: f32, max: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Skewed {
            min: 0.1,
            max,
            factor: FloatRange::skew_factor(-1.0),
        },
    )
    .with_unit(" Hz")
}

impl Default for LayerFxSelectors {
    fn default() -> Self {
        Self {
            mod1_type: EnumParam::new("Mod 1 Type", Mod1Sel::Off),
            mod1_rate_hz: rate("Mod 1 Rate", 4.0, 20.0),
            mod2_type: EnumParam::new("Mod 2 Type", Mod2Sel::Off),
            mod2_rate_hz: rate("Mod 2 Rate", 0.8, 10.0),
            cabinet: EnumParam::new("Amp Cabinet", CabinetSel::Off),
            comp_fast: BoolParam::new("Comp Fast", false),
            delay_sync: BoolParam::new("Delay Sync", false),
            delay_subdivision: EnumParam::new("Delay Division", SubdivisionSel::DottedEighth),
            delay_ping_pong: BoolParam::new("Delay Ping-Pong", false),
            delay_filter: EnumParam::new("Delay Filter", DelayFilterSel::Off),
            delay_fx: EnumParam::new("Delay FX", DelayFxSel::Off),
            reverb_model: EnumParam::new("Reverb Model", ReverbModelSel::Stage),
            reverb_color: EnumParam::new("Reverb Color", ReverbColorSel::Normal),
        }
    }
}

impl LayerFxSelectors {
    /// Writes host values into one engine FX chain (no allocation; plain field stores).
    pub fn apply_to(&self, fx: &mut LayerFxChain) {
        fx.mod1_type = self.mod1_type.value().into();
        fx.mod1_rate_hz = self.mod1_rate_hz.value();
        fx.mod2_type = self.mod2_type.value().into();
        fx.mod2_rate_hz = self.mod2_rate_hz.value();
        fx.amp_cabinet = self.cabinet.value().into();
        fx.comp_fast = self.comp_fast.value();
        fx.delay_clock_sync = self.delay_sync.value();
        fx.delay_subdivision = self.delay_subdivision.value().into();
        fx.delay_ping_pong = self.delay_ping_pong.value();
        fx.delay_filter = self.delay_filter.value().into();
        fx.delay_fx = self.delay_fx.value().into();
        fx.reverb_model = self.reverb_model.value().into();
        fx.reverb_color = self.reverb_color.value().into();
    }
}

/// Per-layer FX selectors for all 7 layers (IDs `fxsel_<id>_1..7`).
#[derive(Params)]
pub struct LayerFxSelectorBank {
    #[nested(array, group = "Layer FX Selectors")]
    pub layers: [LayerFxSelectors; FX_LAYERS],
}

impl Default for LayerFxSelectorBank {
    fn default() -> Self {
        Self {
            layers: std::array::from_fn(|_| LayerFxSelectors::default()),
        }
    }
}

impl LayerFxSelectorBank {
    /// Copies every layer's selectors into the engine.
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
        // Perturb first so the test fails if sync writes nothing.
        for fx in e.fx_chains.iter_mut() {
            fx.mod1_type = Mod1Type::Wah;
            fx.reverb_model = ReverbModel::Cathedral;
            fx.delay_ping_pong = true;
        }
        LayerFxSelectorBank::default().sync_to_engine(&mut e);
        for (a, b) in e.fx_chains.iter().zip(reference.fx_chains.iter()) {
            assert_eq!(a.mod1_type, b.mod1_type);
            assert_eq!(a.mod1_rate_hz, b.mod1_rate_hz);
            assert_eq!(a.mod2_type, b.mod2_type);
            assert_eq!(a.mod2_rate_hz, b.mod2_rate_hz);
            assert_eq!(a.amp_cabinet, b.amp_cabinet);
            assert_eq!(a.comp_fast, b.comp_fast);
            assert_eq!(a.delay_clock_sync, b.delay_clock_sync);
            assert_eq!(a.delay_subdivision, b.delay_subdivision);
            assert_eq!(a.delay_ping_pong, b.delay_ping_pong);
            assert_eq!(a.delay_filter, b.delay_filter);
            assert_eq!(a.delay_fx, b.delay_fx);
            assert_eq!(a.reverb_model, b.reverb_model);
            assert_eq!(a.reverb_color, b.reverb_color);
        }
    }

    #[test]
    fn host_values_reach_only_their_layer() {
        let mut bank = LayerFxSelectorBank::default();
        bank.layers[2].mod1_type = EnumParam::new("m", Mod1Sel::RingMod);
        bank.layers[2].reverb_model = EnumParam::new("r", ReverbModelSel::Spring);
        bank.layers[2].delay_ping_pong = BoolParam::new("p", true);
        bank.layers[2].mod2_rate_hz = rate("r2", 5.0, 10.0);
        let mut e = Stage4Engine::new(48_000.0);
        bank.sync_to_engine(&mut e);
        assert_eq!(e.fx_chains[2].mod1_type, Mod1Type::RingMod);
        assert_eq!(e.fx_chains[2].reverb_model, ReverbModel::Spring);
        assert!(e.fx_chains[2].delay_ping_pong);
        assert_eq!(e.fx_chains[2].mod2_rate_hz, 5.0);
        assert_eq!(e.fx_chains[1].mod1_type, Mod1Type::Off);
        assert!(!e.fx_chains[1].delay_ping_pong);
    }

    #[test]
    fn mirror_enums_cover_every_core_variant() {
        assert_eq!(Mod1Sel::variants().len(), 5);
        assert_eq!(Mod2Sel::variants().len(), 5);
        assert_eq!(CabinetSel::variants().len(), 7);
        assert_eq!(SubdivisionSel::variants().len(), 4);
        assert_eq!(DelayFilterSel::variants().len(), 4);
        assert_eq!(DelayFxSel::variants().len(), 6);
        assert_eq!(ReverbModelSel::variants().len(), 7);
        assert_eq!(ReverbColorSel::variants().len(), 4);
        assert_eq!(Mod1Type::from(Mod1Sel::Wah), Mod1Type::Wah);
        assert_eq!(AmpCabinet::from(CabinetSel::HpFilter), AmpCabinet::HpFilter);
        assert_eq!(
            DelayFeedbackFx::from(DelayFxSel::Space),
            DelayFeedbackFx::Space
        );
        assert_eq!(
            ReverbColor::from(ReverbColorSel::Chorale),
            ReverbColor::Chorale
        );
    }

    #[test]
    fn rate_defaults_equal_engine_defaults() {
        let p = LayerFxSelectors::default();
        assert!((p.mod1_rate_hz.value() - 4.0).abs() < 1e-4);
        assert!((p.mod2_rate_hz.value() - 0.8).abs() < 1e-4);
    }

    #[test]
    fn ids_unique_and_complete() {
        let ids: Vec<String> = LayerFxSelectorBank::default()
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(ids.len(), FX_LAYERS * 13);
        let mut s = ids.clone();
        s.sort();
        s.dedup();
        assert_eq!(s.len(), ids.len());
    }
}
