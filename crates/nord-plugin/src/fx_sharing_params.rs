//! Per-layer FX sharing and variation controls (Phase 6b B9b, `ns4-e3zs`; manual p.48-52):
//! Mod 1 / Mod 2 VARIATION, Wah PED, Master Clock delay division, GLOBAL mode for Delay /
//! Compressor / Reverb and GROUP mode for the Piano and Synth sections.
//!
//! IDs are additive: `fxy_<name>_1..7`, same layer order as `fx_*`
//! (`engine.fx_chains`: Organ A, Organ B, Piano A, Piano B, Synth A, B, C). Defaults equal
//! `LayerFxChain::new` and `FxSharing::default()` (everything off), so an untouched host changes
//! nothing. `TO ROTARY` is the existing `cabinet` selector value (`fxsel_cabinet_*`).
//!
//! Sharing flags mean "this layer is the focused source": with several layers flagged for the same
//! mode the lowest layer index wins (hardware: the Shift+On of the focused layer).

use nih_plug::prelude::*;
use nord_core::engine::{FxSharing, Stage4Engine};
use nord_core::fx::DELAY_SYNC_BEATS;

use crate::fx_params::FX_LAYERS;

/// Master Clock delay division (manual p.51: 1/2 to 1/32 with straight, swing, triplet, dotted).
/// Index `n >= 1` is `DELAY_SYNC_BEATS[n - 1]` (EMPIRICAL order, see `nord_core::fx::DELAY_SYNC_BEATS`).
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncDivisionSel {
    #[name = "Use Division"]
    Off,
    #[name = "1/2"]
    Half,
    #[name = "1/4 D"]
    QuarterD,
    #[name = "1/2 T"]
    HalfT,
    #[name = "1/4 S"]
    QuarterS,
    #[name = "1/4"]
    Quarter,
    #[name = "1/8 D"]
    EighthD,
    #[name = "1/4 T"]
    QuarterT,
    #[name = "1/8 S"]
    EighthS,
    #[name = "1/8"]
    Eighth,
    #[name = "1/16 D"]
    SixteenthD,
    #[name = "1/8 T"]
    EighthT,
    #[name = "1/16 S"]
    SixteenthS,
    #[name = "1/16"]
    Sixteenth,
    #[name = "1/16 T"]
    SixteenthT,
    #[name = "1/32"]
    ThirtySecond,
    #[name = "1/32 T"]
    ThirtySecondT,
    #[name = "1/64"]
    SixtyFourth,
}

impl SyncDivisionSel {
    /// Beats (quarter note = 1) of the selection; `None` keeps the plain `delay_div` selector.
    pub fn beats(self) -> Option<f32> {
        (self as usize)
            .checked_sub(1)
            .and_then(|i| DELAY_SYNC_BEATS.get(i).copied())
    }
}

/// Variation, PED, division and sharing controls of one layer.
#[derive(Params)]
pub struct LayerFxSharingParams {
    /// Mod 1 VARIATION (manual p.49).
    #[id = "mod1_var"]
    pub mod1_variation: BoolParam,
    /// Mod 2 VARIATION (manual p.50).
    #[id = "mod2_var"]
    pub mod2_variation: BoolParam,
    /// Mod 1 PED mode (Wah, manual p.49).
    #[id = "mod1_ped"]
    pub mod1_ped: BoolParam,
    /// Wah filter position in PED mode (the expression pedal / Rate knob).
    #[id = "mod1_ped_pos"]
    pub mod1_ped_pos: FloatParam,
    /// Master Clock delay division; used when `delay_sync` (`fxsel_delay_sync_*`) is on.
    #[id = "delay_beats"]
    pub delay_beats: EnumParam<SyncDivisionSel>,
    /// Delay GLOBAL source (Shift + On, manual p.48).
    #[id = "glob_delay"]
    pub global_delay: BoolParam,
    /// Compressor GLOBAL source.
    #[id = "glob_comp"]
    pub global_comp: BoolParam,
    /// Reverb GLOBAL source (reverb moves after rotary and fader, manual p.52).
    #[id = "glob_rev"]
    pub global_reverb: BoolParam,
    /// GROUP source (Piano layers 3-4, Synth layers 5-7; ignored on the Organ layers).
    #[id = "group"]
    pub group: BoolParam,
}

impl Default for LayerFxSharingParams {
    fn default() -> Self {
        Self {
            mod1_variation: BoolParam::new("Mod 1 Variation", false),
            mod2_variation: BoolParam::new("Mod 2 Variation", false),
            mod1_ped: BoolParam::new("Mod 1 Ped", false),
            mod1_ped_pos: FloatParam::new(
                "Mod 1 Ped Pos",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_step_size(0.01),
            delay_beats: EnumParam::new("Delay Clock Division", SyncDivisionSel::Off),
            global_delay: BoolParam::new("Delay Global", false),
            global_comp: BoolParam::new("Comp Global", false),
            global_reverb: BoolParam::new("Reverb Global", false),
            group: BoolParam::new("FX Group", false),
        }
    }
}

/// All seven layers.
#[derive(Params)]
pub struct LayerFxSharingBank {
    #[nested(array, group = "Layer FX Sharing")]
    pub layers: [LayerFxSharingParams; FX_LAYERS],
}

impl Default for LayerFxSharingBank {
    fn default() -> Self {
        Self {
            layers: std::array::from_fn(|_| LayerFxSharingParams::default()),
        }
    }
}

/// Lowest layer index in `range` whose flag is set.
fn first_flagged(
    layers: &[LayerFxSharingParams; FX_LAYERS],
    range: std::ops::Range<usize>,
    flag: impl Fn(&LayerFxSharingParams) -> bool,
) -> Option<usize> {
    range.into_iter().find(|&i| flag(&layers[i]))
}

impl LayerFxSharingBank {
    /// Copy every value into the engine (no allocation; audio-thread safe).
    ///
    /// The engine applies the sharing copies itself at the start of `process_block`.
    pub fn sync_to_engine(&self, engine: &mut Stage4Engine) {
        for (p, fx) in self.layers.iter().zip(engine.fx_chains.iter_mut()) {
            fx.mod1_variation = p.mod1_variation.value();
            fx.mod2_variation = p.mod2_variation.value();
            fx.mod1_ped = p.mod1_ped.value();
            fx.mod1_ped_pos = p.mod1_ped_pos.value();
            fx.delay_sync_beats = p.delay_beats.value().beats();
        }
        let l = &self.layers;
        engine.fx_sharing = FxSharing {
            delay_global: first_flagged(l, 0..FX_LAYERS, |p| p.global_delay.value()),
            comp_global: first_flagged(l, 0..FX_LAYERS, |p| p.global_comp.value()),
            reverb_global: first_flagged(l, 0..FX_LAYERS, |p| p.global_reverb.value()),
            piano_group: first_flagged(l, 2..4, |p| p.group.value()),
            synth_group: first_flagged(l, 4..FX_LAYERS, |p| p.group.value()),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::fx::{AmpCabinet, LayerFxChain, Mod1Type, Mod2Type, ReverbModel};

    fn flag(name: &str) -> BoolParam {
        BoolParam::new(name.to_string(), true)
    }

    fn pos(v: f32) -> FloatParam {
        FloatParam::new("p", v, FloatRange::Linear { min: 0.0, max: 1.0 })
    }

    #[test]
    fn defaults_match_engine_defaults() {
        let reference = Stage4Engine::new(48_000.0);
        let mut e = Stage4Engine::new(48_000.0);
        LayerFxSharingBank::default().sync_to_engine(&mut e);
        for (a, b) in e.fx_chains.iter().zip(reference.fx_chains.iter()) {
            assert_eq!(a.mod1_variation, b.mod1_variation);
            assert_eq!(a.mod2_variation, b.mod2_variation);
            assert_eq!(a.mod1_ped, b.mod1_ped);
            assert_eq!(a.mod1_ped_pos, b.mod1_ped_pos);
            assert_eq!(a.delay_sync_beats, b.delay_sync_beats);
        }
        assert_eq!(e.fx_sharing, FxSharing::default());
    }

    #[test]
    fn division_selector_matches_the_beats_table() {
        assert_eq!(SyncDivisionSel::Off.beats(), None);
        assert_eq!(SyncDivisionSel::Half.beats(), Some(2.0));
        assert_eq!(SyncDivisionSel::Quarter.beats(), Some(1.0));
        assert_eq!(SyncDivisionSel::EighthD.beats(), Some(0.75));
        assert_eq!(SyncDivisionSel::SixtyFourth.beats(), Some(0.0625));
        assert_eq!(DELAY_SYNC_BEATS.len(), 17);
    }

    #[test]
    fn flags_select_the_lowest_flagged_layer_per_section() {
        let mut bank = LayerFxSharingBank::default();
        bank.layers[6].global_reverb = flag("r");
        bank.layers[3].global_reverb = flag("r");
        bank.layers[3].group = flag("g");
        bank.layers[5].group = flag("g");
        bank.layers[0].group = flag("g"); // organ: no group
        let mut e = Stage4Engine::new(48_000.0);
        bank.sync_to_engine(&mut e);
        assert_eq!(e.fx_sharing.reverb_global, Some(3));
        assert_eq!(e.fx_sharing.piano_group, Some(3));
        assert_eq!(e.fx_sharing.synth_group, Some(5));
        assert_eq!(e.fx_sharing.delay_global, None);
    }

    #[test]
    fn ids_unique_and_complete() {
        let ids: Vec<String> = LayerFxSharingBank::default()
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(ids.len(), FX_LAYERS * 9);
        let mut s = ids.clone();
        s.sort();
        s.dedup();
        assert_eq!(s.len(), ids.len());
        assert!(ids.contains(&"glob_rev_7".to_string()));
    }

    fn chain_out(
        configure: impl Fn(&mut LayerFxSharingParams, &mut LayerFxChain),
    ) -> Vec<[f32; 2]> {
        let mut bank = LayerFxSharingBank::default();
        let mut e = Stage4Engine::new(48_000.0);
        e.fx_chains[0].reverb_model = ReverbModel::Off;
        e.fx_chains[0].mod1_amount = 0.8;
        e.fx_chains[0].mod2_amount = 0.8;
        e.fx_chains[0].delay_enabled = true;
        e.fx_chains[0].delay_clock_sync = true;
        e.fx_chains[0].delay_mix = 1.0;
        configure(&mut bank.layers[0], &mut e.fx_chains[0]);
        bank.sync_to_engine(&mut e);
        let fx = &mut e.fx_chains[0];
        (0..24_000)
            .map(|n| {
                let x = if n < 2400 {
                    (n as f32 * 0.05).sin() * 0.5
                } else {
                    0.0
                };
                fx.process_frame([x, x * 0.8])
            })
            .collect()
    }

    #[test]
    fn each_variation_and_division_param_changes_the_chain_output() {
        let base = chain_out(|_, _| {});
        let m1v = chain_out(|p, fx| {
            fx.mod1_type = Mod1Type::Tremolo;
            p.mod1_variation = flag("v");
        });
        let m1 = chain_out(|_, fx| fx.mod1_type = Mod1Type::Tremolo);
        assert!(m1 != m1v, "mod1 variation");
        let m2v = chain_out(|p, fx| {
            fx.mod2_type = Mod2Type::Phaser;
            p.mod2_variation = flag("v");
        });
        let m2 = chain_out(|_, fx| fx.mod2_type = Mod2Type::Phaser);
        assert!(m2 != m2v, "mod2 variation");
        let ped = |v: f32| {
            chain_out(move |p, fx| {
                fx.mod1_type = Mod1Type::Wah;
                p.mod1_ped = flag("p");
                p.mod1_ped_pos = pos(v);
            })
        };
        let (ped_lo, ped_hi) = (ped(0.1), ped(0.9));
        assert!(ped_lo != ped_hi, "wah ped position");
        let div = chain_out(|p, _| p.delay_beats = EnumParam::new("d", SyncDivisionSel::Half));
        assert!(base != div, "delay division");
        let finite = [&m1v, &m2v, &ped_lo, &ped_hi, &div]
            .iter()
            .all(|v| v.iter().all(|f| f[0].is_finite() && f[1].is_finite()));
        assert!(finite);
    }

    type Setup<'a> = &'a dyn Fn(&mut LayerFxSharingBank, &mut Stage4Engine);

    fn render(configure: Setup) -> Vec<(f32, f32)> {
        let mut bank = LayerFxSharingBank::default();
        let mut e = Stage4Engine::new(48_000.0);
        for fx in &mut e.fx_chains {
            fx.reverb_model = ReverbModel::Off;
        }
        e.synth_a.note_on(60, 100);
        configure(&mut bank, &mut e);
        bank.sync_to_engine(&mut e);
        let (mut l, mut r) = (vec![0.0f32; 12_000], vec![0.0f32; 12_000]);
        e.process_block(&mut l, &mut r);
        l.into_iter().zip(r).collect()
    }

    #[test]
    fn to_rotary_and_global_reverb_change_the_rendered_engine_output() {
        let base = render(&|_, _| {});
        let rotary = render(&|_, e| e.fx_chains[4].amp_cabinet = AmpCabinet::ToRotary);
        assert!(base != rotary, "to rotary");
        assert!(rotary.iter().all(|f| f.0.is_finite() && f.1.is_finite()));
        let rev = |e: &mut Stage4Engine| {
            e.fx_chains[4].reverb_model = ReverbModel::Hall;
            e.fx_chains[4].reverb_mix = 0.5;
        };
        let global_rev = render(&|b, e| {
            rev(e);
            b.layers[4].global_reverb = flag("g");
        });
        let local_rev = render(&|_, e| rev(e));
        assert!(global_rev != local_rev, "global reverb");
    }

    #[test]
    fn global_delay_and_synth_group_change_the_rendered_engine_output() {
        let delay = |e: &mut Stage4Engine| {
            e.fx_chains[3].delay_enabled = true;
            e.fx_chains[3].delay_mix = 0.9;
            e.fx_chains[3].delay_time_ms = 80.0;
        };
        let global_delay = render(&|b, e| {
            delay(e);
            b.layers[3].global_delay = flag("g");
        });
        let local_delay = render(&|_, e| delay(e));
        assert!(global_delay != local_delay, "global delay");

        let trem = |e: &mut Stage4Engine| {
            e.fx_chains[5].mod1_type = Mod1Type::Tremolo;
            e.fx_chains[5].mod1_amount = 1.0;
        };
        let group = render(&|b, e| {
            trem(e);
            b.layers[5].group = flag("g");
        });
        let ungrouped = render(&|_, e| trem(e));
        assert!(group != ungrouped, "synth group");
    }
}
