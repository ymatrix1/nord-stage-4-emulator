//! FX edits to host parameter writes (`fx_*`, `fxsel_*`, `fxx_*`, `fxy_*` banks count all seven
//! layers; the panel's FX slot is mapped to a chain with `chain_of_slot`).

use super::map::{enum_index, flag};
use super::write::ParamWrite as W;
use crate::fx_selectors::{
    CabinetSel, DelayFilterSel, DelayFxSel, Mod1Sel, Mod2Sel, ReverbColorSel, ReverbModelSel,
};
use nord_gui::audio::{chain_of_slot, FxEdit, FxFlag, FxFloat, FX_SLOTS};

/// FX chain layer numbers a panel FX slot addresses (slot 0 is the shared Organ A+B chain).
fn fx_layers(slot: u8) -> Vec<u8> {
    let slot = usize::from(slot);
    if slot >= FX_SLOTS {
        return Vec::new();
    }
    let mut v = vec![chain_of_slot(slot) as u8 + 1];
    if slot == 0 {
        v.push(2);
    }
    v
}

fn fx_float_family(p: FxFloat) -> &'static str {
    match p {
        FxFloat::Mod1Rate => "fxsel_mod1_rate",
        FxFloat::Mod1Amount => "fxx_mod1_amount",
        FxFloat::Mod1PedPos => "fxy_mod1_ped_pos",
        FxFloat::Mod2Rate => "fxsel_mod2_rate",
        FxFloat::Mod2Amount => "fxx_mod2_amount",
        FxFloat::Drive => "fx_drive",
        FxFloat::EqBass => "fx_eq_bass",
        FxFloat::EqMid => "fx_eq_mid",
        FxFloat::EqMidFreq => "fxx_eq_mid_freq",
        FxFloat::EqTreble => "fx_eq_treble",
        FxFloat::FilterRes => "fxx_filter_res",
        FxFloat::CompAmount => "fx_comp_amt",
        FxFloat::DelayTime => "fx_delay_ms",
        FxFloat::DelayFeedback => "fx_delay_fb",
        FxFloat::DelayMix => "fx_delay_mix",
        FxFloat::ReverbMix => "fx_rev_mix",
    }
}

fn fx_flag_family(p: FxFlag) -> &'static str {
    match p {
        FxFlag::Mod1Variation => "fxy_mod1_var",
        FxFlag::Mod1Ped => "fxy_mod1_ped",
        FxFlag::Mod2Variation => "fxy_mod2_var",
        FxFlag::AmpVariation => "fxx_amp_variation",
        FxFlag::CompEnabled => "fx_comp_on",
        FxFlag::CompFast => "fxsel_comp_fast",
        FxFlag::DelayEnabled => "fx_delay_on",
        FxFlag::DelayAnalog => "fxx_delay_analog",
        FxFlag::DelayPingPong => "fxsel_delay_pp",
        FxFlag::DelayFxVariation => "fxx_delay_fx_variation",
    }
}

pub(super) fn map_fx(e: &FxEdit, out: &mut Vec<W>) {
    let mut push = |slot: u8, family: &'static str, plain: Option<f32>| {
        if let Some(plain) = plain {
            for n in fx_layers(slot) {
                out.push(W::layer(family, n, plain));
            }
        }
    };
    match *e {
        FxEdit::Float { slot, param, value } => {
            push(slot, fx_float_family(param), Some(value));
        }
        FxEdit::Flag { slot, param, on } => push(slot, fx_flag_family(param), Some(flag(on))),
        FxEdit::Mod1Type { slot, kind } => {
            push(slot, "fxsel_mod1_type", enum_index::<Mod1Sel>(&kind))
        }
        FxEdit::Mod2Type { slot, kind } => {
            push(slot, "fxsel_mod2_type", enum_index::<Mod2Sel>(&kind))
        }
        FxEdit::Cabinet { slot, cabinet } => {
            push(slot, "fxsel_cabinet", enum_index::<CabinetSel>(&cabinet))
        }
        FxEdit::DelaySync { slot, beats } => {
            push(slot, "fxsel_delay_sync", Some(flag(beats.is_some())))
        }
        FxEdit::DelayFilter { slot, mode } => {
            push(slot, "fxsel_delay_flt", enum_index::<DelayFilterSel>(&mode))
        }
        FxEdit::DelayFx { slot, fx } => push(slot, "fxsel_delay_fx", enum_index::<DelayFxSel>(&fx)),
        FxEdit::ReverbModel { slot, model } => push(
            slot,
            "fxsel_rev_model",
            enum_index::<ReverbModelSel>(&model),
        ),
        FxEdit::ReverbColor { slot, color } => push(
            slot,
            "fxsel_rev_color",
            enum_index::<ReverbColorSel>(&color),
        ),
        // FX sharing (global delay/comp/reverb, group) is a bank-level toggle with no panel edit
        // that carries a layer; it stays dimmed.
        FxEdit::Sharing(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::fx::Mod1Type;

    #[test]
    fn fx_slot_zero_writes_both_organ_chains_and_slots_follow_chain_of_slot() {
        let mut v = Vec::new();
        map_fx(
            &FxEdit::Float {
                slot: 0,
                param: FxFloat::Drive,
                value: 0.4,
            },
            &mut v,
        );
        assert_eq!(
            v,
            vec![W::layer("fx_drive", 1, 0.4), W::layer("fx_drive", 2, 0.4)]
        );
        let mut s = Vec::new();
        map_fx(
            &FxEdit::Mod1Type {
                slot: 5,
                kind: Mod1Type::Wah,
            },
            &mut s,
        );
        assert_eq!(s, vec![W::layer("fxsel_mod1_type", 7, 4.0)]);
        let mut bad = Vec::new();
        map_fx(
            &FxEdit::Flag {
                slot: 9,
                param: FxFlag::CompFast,
                on: true,
            },
            &mut bad,
        );
        assert!(bad.is_empty());
    }
}
