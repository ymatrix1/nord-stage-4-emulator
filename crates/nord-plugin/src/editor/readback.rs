//! Read-back: copy the current host parameter values into the panel state so knobs, faders,
//! drawbars and LEDs show host automation and presets.
//!
//! GUI thread only; reads go through `ParamPtr::unmodulated_plain_value`, which is an atomic
//! load, so no lock is taken and the audio thread is never involved.
//!
//! Scope: the top-level `Stage4PanelState` fields that the sink also writes (levels, layer
//! enables, organ model/drawbars/percussion/rotary, piano selectors, global synth filter and
//! envelope, mod wheel) and the FX knobs of the focused slot. Per-layer synth/organ detail kept
//! in `ui_state` is panel-local and not mirrored yet (tracked in the editor spec).

use super::index::ParamIndex;
use super::map::DRAWBARS_A;
use nord_core::fx::{Mod1Type, Mod2Type, ReverbModel};
use nord_core::organ::{OrganModel, RotarySpeed};
use nord_core::piano::{PianoCategory, PianoTimbre, VelocityTouch};
use nord_core::synth::SynthOscMode;
use nord_gui::panel::Stage4PanelState;

fn organ_model(i: i32) -> OrganModel {
    match i {
        1 => OrganModel::Vox,
        2 => OrganModel::Farfisa,
        3 => OrganModel::Pipe1,
        4 => OrganModel::Pipe2,
        _ => OrganModel::B3,
    }
}

fn piano_category(i: i32) -> PianoCategory {
    match i {
        1 => PianoCategory::Upright,
        2 => PianoCategory::Electric,
        3 => PianoCategory::Clav,
        4 => PianoCategory::Digital,
        5 => PianoCategory::Misc,
        _ => PianoCategory::Grand,
    }
}

fn piano_timbre(i: i32) -> PianoTimbre {
    match i {
        0 => PianoTimbre::Soft,
        2 => PianoTimbre::Bright,
        3 => PianoTimbre::Dyno1,
        4 => PianoTimbre::Dyno2,
        _ => PianoTimbre::Mid,
    }
}

fn piano_touch(i: i32) -> VelocityTouch {
    match i {
        1 => VelocityTouch::Light,
        2 => VelocityTouch::Heavy,
        _ => VelocityTouch::Medium,
    }
}

fn osc_mode(i: i32) -> SynthOscMode {
    match i {
        1 => SynthOscMode::Wavetable,
        2 => SynthOscMode::Fm,
        3 => SynthOscMode::Sample,
        _ => SynthOscMode::Analog,
    }
}

/// Overwrite `state` fields from the host parameters (missing parameters are skipped).
pub fn sync_panel_from_params(idx: &ParamIndex, s: &mut Stage4PanelState) {
    let f = |id: &str| idx.plain(id);
    let b = |id: &str| idx.plain(id).map(|v| v >= 0.5);
    let i = |id: &str| idx.plain(id).map(|v| v.round() as i32);

    if let Some(v) = f("master_level") {
        s.master_volume = v;
    }
    if let Some(v) = f("mod_wheel") {
        s.mod_wheel = v;
    }
    if let Some(v) = f("master_compressor") {
        s.master_comp = v;
    }
    if let Some(fast) = b("rotary_speed_fast") {
        s.rotary_speed = if fast {
            RotarySpeed::Fast
        } else if s.rotary_stop {
            RotarySpeed::Stop
        } else {
            RotarySpeed::Slow
        };
    }
    if let Some(v) = f("rotary_drive") {
        s.rotary_drive = v;
    }
    if let Some(v) = b("organx_stop_mode") {
        s.rotary_stop = v;
    }

    // Layer enables and levels.
    for (flag, id) in [
        (&mut s.organ_a_active, "organ_a_enabled"),
        (&mut s.organ_b_active, "organ_b_enabled"),
        (&mut s.piano_a_active, "piano_a_enabled"),
        (&mut s.piano_b_active, "piano_b_enabled"),
        (&mut s.synth_a_active, "synth_a_enabled"),
        (&mut s.synth_b_active, "synth_b_enabled"),
        (&mut s.synth_c_active, "synth_c_enabled"),
    ] {
        if let Some(v) = b(id) {
            *flag = v;
        }
    }
    for (lvl, id) in [
        (&mut s.organ_a_level, "organ_a_level"),
        (&mut s.organ_b_level, "organ_b_level"),
        (&mut s.piano_a_level, "piano_a_level"),
        (&mut s.piano_b_level, "piano_b_level"),
        (&mut s.synth_a_level, "synth_a_level"),
        (&mut s.synth_b_level, "synth_b_level"),
        (&mut s.synth_c_level, "synth_c_level"),
    ] {
        if let Some(v) = f(id) {
            *lvl = v;
        }
    }

    // Organ.
    if let Some(v) = i("organ_model") {
        s.organ_model = organ_model(v);
    }
    for (slot, id) in s.organ_drawbars.iter_mut().zip(DRAWBARS_A) {
        if let Some(v) = i(id) {
            *slot = v.clamp(0, 8) as u8;
        }
    }
    for (flag, id) in [
        (&mut s.perc_on, "organ_perc_enable"),
        (&mut s.perc_soft, "organ_perc_soft"),
        (&mut s.perc_fast, "organ_perc_fast"),
        (&mut s.perc_third, "organ_perc_third"),
    ] {
        if let Some(v) = b(id) {
            *flag = v;
        }
    }

    // Piano.
    if let Some(v) = i("piano_category") {
        s.piano_cat = piano_category(v);
    }
    if let Some(v) = i("piano_timbre") {
        s.piano_timbre = piano_timbre(v);
    }
    if let Some(v) = i("piano_touch") {
        s.piano_touch = piano_touch(v);
    }
    if let Some(v) = b("piano_string_res") {
        s.string_res = v;
    }
    if let Some(v) = b("piano_pedal_noise") {
        s.pedal_noise = v;
    }

    // Synth (global filter and envelope).
    if let Some(v) = i("synth_osc_mode") {
        s.synth_osc_mode = osc_mode(v);
    }
    for (slot, id) in [
        (&mut s.synth_cutoff, "synth_filter_cutoff"),
        (&mut s.synth_resonance, "synth_filter_resonance"),
        (&mut s.synth_drive, "synth_filter_drive"),
        (&mut s.synth_attack, "synth_attack"),
        (&mut s.synth_decay, "synth_decay"),
        (&mut s.synth_release, "synth_release"),
    ] {
        if let Some(v) = f(id) {
            *slot = v;
        }
    }
    sync_fx_focus(idx, s);
    super::readback_menu::sync_menu_from_params(idx, &mut s.program_ui.menu);
}

/// Host layer number (`fx_*_{n}`) the panel's FX focus slot addresses.
pub fn fx_focus_layer(focus: usize) -> u8 {
    (nord_gui::audio::chain_of_slot(focus.min(nord_gui::audio::FX_SLOTS - 1)) + 1) as u8
}

fn sync_fx_focus(idx: &ParamIndex, s: &mut Stage4PanelState) {
    let n = Some(fx_focus_layer(s.fx_focus));
    let f = |family: &str| idx.plain_of(family, n);
    for (slot, family) in [
        (&mut s.eq_drive, "fx_drive"),
        (&mut s.eq_bass, "fx_eq_bass"),
        (&mut s.eq_mid, "fx_eq_mid"),
        (&mut s.eq_mid_freq, "fxx_eq_mid_freq"),
        (&mut s.eq_treble, "fx_eq_treble"),
        (&mut s.delay_feedback, "fx_delay_fb"),
        (&mut s.delay_mix, "fx_delay_mix"),
        (&mut s.reverb_mix, "fx_rev_mix"),
        (&mut s.mod1_rate, "fxsel_mod1_rate"),
        (&mut s.mod1_amount, "fxx_mod1_amount"),
        (&mut s.mod2_rate, "fxsel_mod2_rate"),
        (&mut s.mod2_amount, "fxx_mod2_amount"),
    ] {
        if let Some(v) = f(family) {
            *slot = v;
        }
    }
    if let Some(v) = f("fx_delay_on") {
        s.delay_on = v >= 0.5;
    }
    if let Some(v) = f("fxsel_mod1_type") {
        s.mod1_type = mod1_type(v.round() as i32);
    }
    if let Some(v) = f("fxsel_mod2_type") {
        s.mod2_type = mod2_type(v.round() as i32);
    }
    if let Some(v) = f("fxsel_rev_model") {
        s.reverb_model = reverb_model(v.round() as i32);
    }
}

fn mod1_type(i: i32) -> Mod1Type {
    match i {
        1 => Mod1Type::Tremolo,
        2 => Mod1Type::AutoPan,
        3 => Mod1Type::RingMod,
        4 => Mod1Type::Wah,
        5 => Mod1Type::AutoWah,
        6 => Mod1Type::Pump,
        _ => Mod1Type::Off,
    }
}

fn mod2_type(i: i32) -> Mod2Type {
    match i {
        1 => Mod2Type::Chorus,
        2 => Mod2Type::Flanger,
        3 => Mod2Type::Phaser,
        4 => Mod2Type::Vibe,
        5 => Mod2Type::Spin,
        6 => Mod2Type::Ensemble,
        _ => Mod2Type::Off,
    }
}

fn reverb_model(i: i32) -> ReverbModel {
    match i {
        1 => ReverbModel::Booth,
        2 => ReverbModel::Room,
        3 => ReverbModel::Stage,
        4 => ReverbModel::Spring,
        5 => ReverbModel::Hall,
        6 => ReverbModel::Cathedral,
        _ => ReverbModel::Off,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::map::enum_index;
    use crate::fx_selectors::{Mod1Sel, Mod2Sel, ReverbModelSel};
    use crate::params::Stage4Params;

    #[test]
    fn defaults_are_read_from_the_host_parameters() {
        let p = Stage4Params::default();
        let idx = ParamIndex::new(&p);
        let mut s = Stage4PanelState {
            master_volume: 0.0,
            organ_drawbars: [0; 9],
            ..Default::default()
        };
        sync_panel_from_params(&idx, &mut s);
        assert!((s.master_volume - 0.8).abs() < 1e-6);
        assert_eq!(s.organ_drawbars, [8, 8, 8, 0, 0, 0, 0, 0, 0]);
        assert_eq!(s.organ_model, OrganModel::B3);
        assert!(s.organ_a_active && !s.organ_b_active);
    }

    #[test]
    fn host_enum_indices_round_trip_through_the_name_matching_mapper() {
        for k in [Mod1Type::Off, Mod1Type::Wah, Mod1Type::Pump] {
            let i = enum_index::<Mod1Sel>(&k).unwrap() as i32;
            assert_eq!(mod1_type(i), k);
        }
        for k in [Mod2Type::Off, Mod2Type::Vibe, Mod2Type::Ensemble] {
            let i = enum_index::<Mod2Sel>(&k).unwrap() as i32;
            assert_eq!(mod2_type(i), k);
        }
        for k in [
            ReverbModel::Off,
            ReverbModel::Spring,
            ReverbModel::Cathedral,
        ] {
            let i = enum_index::<ReverbModelSel>(&k).unwrap() as i32;
            assert_eq!(reverb_model(i), k);
        }
    }

    #[test]
    fn selector_indices_agree_with_the_write_side() {
        use crate::editor::map::{
            organ_model_index, osc_mode_index, piano_category_index, piano_timbre_index,
            piano_touch_index,
        };
        for m in [OrganModel::B3, OrganModel::Pipe2, OrganModel::Farfisa] {
            assert_eq!(organ_model(organ_model_index(m) as i32), m);
        }
        for c in [
            PianoCategory::Grand,
            PianoCategory::Misc,
            PianoCategory::Clav,
        ] {
            assert_eq!(piano_category(piano_category_index(c) as i32), c);
        }
        for t in [PianoTimbre::Soft, PianoTimbre::Dyno2, PianoTimbre::Mid] {
            assert_eq!(piano_timbre(piano_timbre_index(t).unwrap() as i32), t);
        }
        for t in [VelocityTouch::Light, VelocityTouch::Heavy] {
            assert_eq!(piano_touch(piano_touch_index(t) as i32), t);
        }
        for m in [SynthOscMode::Fm, SynthOscMode::Sample] {
            assert_eq!(osc_mode(osc_mode_index(m) as i32), m);
        }
    }

    #[test]
    fn fx_focus_uses_the_chain_of_the_slot() {
        assert_eq!(fx_focus_layer(0), 1);
        assert_eq!(fx_focus_layer(1), 3);
        assert_eq!(fx_focus_layer(5), 7);
        assert_eq!(fx_focus_layer(99), 7);
    }
}
