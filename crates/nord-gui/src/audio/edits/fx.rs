//! Fx edits applied on the DSP thread (one variant per panel control group).
//!
//! The Layer Effects bay edits the focused layer's chain. A `slot` is the panel's FX focus
//! target: `0` Organ (chains 0 and 1, which share one effects chain, manual p.19 and p.48),
//! `1`/`2` Piano A/B (chains 2, 3) and `3`/`4`/`5` Synth A/B/C (chains 4, 5, 6).
//! `AudioCommand::Fx` carries these; every value is clamped here so a bad message can never put
//! `NaN` or an unstable setting into a chain (`.agents/rules/dsp-and-realtime.md`).

mod params;

use nord_core::engine::{FxSharing, Stage4Engine};
use nord_core::fx::{
    AmpCabinet, DelayFeedbackFx, DelayFilterMode, LayerFxChain, Mod1Type, Mod2Type, ReverbColor,
    ReverbModel,
};
pub use params::{FxFlag, FxFloat};

/// Number of FX focus slots (Organ, Piano A/B, Synth A/B/C).
pub const FX_SLOTS: usize = 6;

/// Engine chain index of a focus slot (Organ also drives chain 1, see [`for_slot`]).
pub fn chain_of_slot(slot: usize) -> usize {
    if slot == 0 {
        0
    } else {
        (slot + 1).min(6)
    }
}

/// Panel edits for the Fx area.
#[derive(Debug, Clone, PartialEq)]
pub enum FxEdit {
    /// A continuous control of the chain of `slot`.
    Float {
        slot: u8,
        param: FxFloat,
        value: f32,
    },
    /// A switch of the chain of `slot`.
    Flag { slot: u8, param: FxFlag, on: bool },
    /// Mod 1 type (`Off` while the unit is off).
    Mod1Type { slot: u8, kind: Mod1Type },
    /// Mod 2 type (`Off` while the unit is off).
    Mod2Type { slot: u8, kind: Mod2Type },
    /// Amp model, `TO ROTARY` or LP24/HP24 (`Off` while the unit is off).
    Cabinet { slot: u8, cabinet: AmpCabinet },
    /// Delay MST CLK division: `Some(beats)` locks to the Master Clock, `None` is free time.
    DelaySync { slot: u8, beats: Option<f32> },
    /// Mod 1 MST CLK division (A-Pan, Trem, Wah, Pump): `Some(beats)` replaces the Rate knob.
    Mod1Sync { slot: u8, beats: Option<f32> },
    /// Delay feedback filter LP / HP / BP.
    DelayFilter { slot: u8, mode: DelayFilterMode },
    /// Delay feedback effect (ENS / VIBE / CHOR / FLAM / SPACE).
    DelayFx { slot: u8, fx: DelayFeedbackFx },
    /// Reverb type (`Off` while the unit is off).
    ReverbModel { slot: u8, model: ReverbModel },
    /// Reverb colour (Bright / Dark / Chorale).
    ReverbColor { slot: u8, color: ReverbColor },
    /// GROUP / GLOBAL sharing sources (engine chain indices; `None` = mode off).
    Sharing(FxSharing),
}

/// Runs `f` on every engine chain behind `slot` (Organ drives chains 0 and 1).
fn for_slot(engine: &mut Stage4Engine, slot: u8, mut f: impl FnMut(&mut LayerFxChain)) {
    let slot = slot as usize;
    if slot >= FX_SLOTS {
        return;
    }
    f(&mut engine.fx_chains[chain_of_slot(slot)]);
    if slot == 0 {
        f(&mut engine.fx_chains[1]);
    }
}

impl FxEdit {
    /// Apply the edit to the engine (DSP thread; no allocation).
    pub fn apply(&self, engine: &mut Stage4Engine) {
        match *self {
            Self::Float { slot, param, value } => {
                if value.is_finite() {
                    for_slot(engine, slot, |fx| param.set(fx, value));
                }
            }
            Self::Flag { slot, param, on } => for_slot(engine, slot, |fx| param.set(fx, on)),
            Self::Mod1Type { slot, kind } => for_slot(engine, slot, |fx| fx.mod1_type = kind),
            Self::Mod2Type { slot, kind } => for_slot(engine, slot, |fx| fx.mod2_type = kind),
            Self::Cabinet { slot, cabinet } => {
                // TO ROTARY routes Piano / Synth layers only (the Organ is the rotary's own source).
                let cabinet = if slot == 0 && cabinet == AmpCabinet::ToRotary {
                    AmpCabinet::Off
                } else {
                    cabinet
                };
                for_slot(engine, slot, |fx| fx.amp_cabinet = cabinet);
            }
            Self::DelaySync { slot, beats } => for_slot(engine, slot, |fx| {
                fx.delay_clock_sync = beats.is_some();
                fx.delay_sync_beats = beats.filter(|b| b.is_finite() && *b > 0.0);
            }),
            Self::Mod1Sync { slot, beats } => for_slot(engine, slot, |fx| {
                fx.mod1_sync_beats = beats.filter(|b| b.is_finite() && *b > 0.0);
            }),
            Self::DelayFilter { slot, mode } => for_slot(engine, slot, |fx| fx.delay_filter = mode),
            Self::DelayFx { slot, fx: kind } => for_slot(engine, slot, |fx| fx.delay_fx = kind),
            Self::ReverbModel { slot, model } => {
                for_slot(engine, slot, |fx| fx.reverb_model = model)
            }
            Self::ReverbColor { slot, color } => {
                for_slot(engine, slot, |fx| fx.reverb_color = color)
            }
            Self::Sharing(s) => engine.fx_sharing = s,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> Stage4Engine {
        Stage4Engine::new(48_000.0)
    }

    #[test]
    fn slot_to_chain_mapping() {
        let chains: Vec<usize> = (0..FX_SLOTS).map(chain_of_slot).collect();
        assert_eq!(chains, [0, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn every_float_param_reaches_only_its_chain_and_is_clamped() {
        let reference = engine();
        for p in FxFloat::ALL {
            let mut e = engine();
            let before = e.fx_chains[3].clone();
            FxEdit::Float {
                slot: 2,
                param: p,
                value: 1.0e9,
            }
            .apply(&mut e);
            // Piano B (chain 3) changed to its upper bound; all others untouched.
            let hi = e.fx_chains[3].clone();
            FxEdit::Float {
                slot: 2,
                param: p,
                value: -1.0e9,
            }
            .apply(&mut e);
            let lo = e.fx_chains[3].clone();
            let probe = |c: &LayerFxChain| -> f32 {
                match p {
                    FxFloat::Mod1Rate => c.mod1_rate_hz,
                    FxFloat::Mod1Amount => c.mod1_amount,
                    FxFloat::Mod1PedPos => c.mod1_ped_pos,
                    FxFloat::Mod2Rate => c.mod2_rate_hz,
                    FxFloat::Mod2Amount => c.mod2_amount,
                    FxFloat::Drive => c.drive_amount,
                    FxFloat::EqBass => c.eq_bass_db,
                    FxFloat::EqMid => c.eq_mid_db,
                    FxFloat::EqMidFreq => c.eq_mid_freq_hz,
                    FxFloat::EqTreble => c.eq_treble_db,
                    FxFloat::FilterRes => c.eq_filter_res,
                    FxFloat::CompAmount => c.comp_amount,
                    FxFloat::DelayTime => c.delay_time_ms,
                    FxFloat::DelayFeedback => c.delay_feedback,
                    FxFloat::DelayMix => c.delay_mix,
                    FxFloat::ReverbMix => c.reverb_mix,
                }
            };
            assert!(probe(&hi) > probe(&lo), "{p:?} upper bound not above lower");
            assert!(probe(&hi).is_finite() && probe(&lo).is_finite());
            assert_ne!(probe(&hi), probe(&before), "{p:?} did not change (hi)");
            for i in [0, 1, 2, 4, 5, 6] {
                assert_eq!(probe(&e.fx_chains[i]), probe(&reference.fx_chains[i]));
            }
        }
    }

    #[test]
    fn non_finite_values_are_ignored() {
        let mut e = engine();
        let before = e.fx_chains[2].delay_feedback;
        FxEdit::Float {
            slot: 1,
            param: FxFloat::DelayFeedback,
            value: f32::NAN,
        }
        .apply(&mut e);
        assert_eq!(e.fx_chains[2].delay_feedback, before);
    }

    #[test]
    fn every_flag_param_toggles_its_chain() {
        for p in FxFlag::ALL {
            let mut e = engine();
            FxEdit::Flag {
                slot: 5,
                param: p,
                on: true,
            }
            .apply(&mut e);
            let c = &e.fx_chains[6];
            let v = match p {
                FxFlag::Mod1Variation => c.mod1_variation,
                FxFlag::Mod1Ped => c.mod1_ped,
                FxFlag::Mod2Variation => c.mod2_variation,
                FxFlag::AmpVariation => c.amp_variation,
                FxFlag::CompEnabled => c.comp_enabled,
                FxFlag::CompFast => c.comp_fast,
                FxFlag::DelayEnabled => c.delay_enabled,
                FxFlag::DelayAnalog => c.delay_analog,
                FxFlag::DelayPingPong => c.delay_ping_pong,
                FxFlag::DelayFxVariation => c.delay_fx_variation,
                FxFlag::ReverbVariation => c.reverb_variation,
            };
            assert!(v, "{p:?} not set");
            assert!(!e.fx_chains[2].comp_enabled);
        }
    }

    #[test]
    fn organ_slot_drives_both_organ_chains() {
        let mut e = engine();
        FxEdit::Mod1Type {
            slot: 0,
            kind: Mod1Type::Tremolo,
        }
        .apply(&mut e);
        FxEdit::ReverbModel {
            slot: 0,
            model: ReverbModel::Hall,
        }
        .apply(&mut e);
        for i in [0, 1] {
            assert_eq!(e.fx_chains[i].mod1_type, Mod1Type::Tremolo);
            assert_eq!(e.fx_chains[i].reverb_model, ReverbModel::Hall);
        }
        assert_eq!(e.fx_chains[2].mod1_type, Mod1Type::Off);
    }

    #[test]
    fn selector_edits_set_their_fields() {
        let mut e = engine();
        FxEdit::Mod2Type {
            slot: 3,
            kind: Mod2Type::Phaser,
        }
        .apply(&mut e);
        FxEdit::DelayFilter {
            slot: 3,
            mode: DelayFilterMode::Bp,
        }
        .apply(&mut e);
        FxEdit::DelayFx {
            slot: 3,
            fx: DelayFeedbackFx::Space,
        }
        .apply(&mut e);
        FxEdit::ReverbColor {
            slot: 3,
            color: ReverbColor::Chorale,
        }
        .apply(&mut e);
        FxEdit::Cabinet {
            slot: 3,
            cabinet: AmpCabinet::ToRotary,
        }
        .apply(&mut e);
        let c = &e.fx_chains[4];
        assert_eq!(c.mod2_type, Mod2Type::Phaser);
        assert_eq!(c.delay_filter, DelayFilterMode::Bp);
        assert_eq!(c.delay_fx, DelayFeedbackFx::Space);
        assert_eq!(c.reverb_color, ReverbColor::Chorale);
        assert_eq!(c.amp_cabinet, AmpCabinet::ToRotary);
    }

    #[test]
    fn to_rotary_is_refused_on_the_organ_slot() {
        let mut e = engine();
        FxEdit::Cabinet {
            slot: 0,
            cabinet: AmpCabinet::ToRotary,
        }
        .apply(&mut e);
        assert_eq!(e.fx_chains[0].amp_cabinet, AmpCabinet::Off);
        assert_eq!(e.fx_chains[1].amp_cabinet, AmpCabinet::Off);
    }

    #[test]
    fn delay_sync_sets_and_clears_the_clock_lock() {
        let mut e = engine();
        FxEdit::DelaySync {
            slot: 1,
            beats: Some(0.75),
        }
        .apply(&mut e);
        assert!(e.fx_chains[2].delay_clock_sync);
        assert_eq!(e.fx_chains[2].delay_sync_beats, Some(0.75));
        FxEdit::DelaySync {
            slot: 1,
            beats: None,
        }
        .apply(&mut e);
        assert!(!e.fx_chains[2].delay_clock_sync);
        assert_eq!(e.fx_chains[2].delay_sync_beats, None);
    }

    #[test]
    fn mod1_sync_sets_the_beats_and_ignores_bad_values() {
        let mut e = engine();
        FxEdit::Mod1Sync {
            slot: 4,
            beats: Some(0.5),
        }
        .apply(&mut e);
        assert_eq!(e.fx_chains[5].mod1_sync_beats, Some(0.5));
        assert_eq!(e.fx_chains[4].mod1_sync_beats, None);
        for bad in [Some(f32::NAN), Some(-1.0), Some(0.0), None] {
            FxEdit::Mod1Sync {
                slot: 4,
                beats: bad,
            }
            .apply(&mut e);
            assert_eq!(e.fx_chains[5].mod1_sync_beats, None, "{bad:?}");
        }
    }

    #[test]
    fn new_mod_types_and_reverb_variation_reach_the_engine() {
        use nord_core::fx::{Mod1Type, Mod2Type};
        let mut e = engine();
        for (k1, k2) in [
            (Mod1Type::AutoWah, Mod2Type::Spin),
            (Mod1Type::Pump, Mod2Type::Ensemble),
        ] {
            FxEdit::Mod1Type { slot: 3, kind: k1 }.apply(&mut e);
            FxEdit::Mod2Type { slot: 3, kind: k2 }.apply(&mut e);
            assert_eq!(e.fx_chains[4].mod1_type, k1);
            assert_eq!(e.fx_chains[4].mod2_type, k2);
        }
        FxEdit::Flag {
            slot: 3,
            param: FxFlag::ReverbVariation,
            on: true,
        }
        .apply(&mut e);
        assert!(e.fx_chains[4].reverb_variation && !e.fx_chains[5].reverb_variation);
    }

    #[test]
    fn sharing_edit_replaces_the_engine_modes() {
        let mut e = engine();
        let s = FxSharing {
            piano_group: Some(3),
            delay_global: Some(2),
            ..FxSharing::default()
        };
        FxEdit::Sharing(s).apply(&mut e);
        assert_eq!(e.fx_sharing, s);
        FxEdit::Sharing(FxSharing::default()).apply(&mut e);
        assert!(!e.fx_sharing.any());
    }

    #[test]
    fn out_of_range_slot_is_ignored() {
        let mut e = engine();
        FxEdit::Flag {
            slot: 9,
            param: FxFlag::CompEnabled,
            on: true,
        }
        .apply(&mut e);
        assert!(e.fx_chains.iter().all(|c| !c.comp_enabled));
    }
}
