//! Tests of `piano_logic.rs`.

use super::*;

fn st() -> Stage4PanelState {
    Stage4PanelState::default()
}

#[test]
fn scene_mask_follows_the_active_flags() {
    let mut s = st();
    s.organ_a_active = false;
    s.piano_a_active = true;
    s.synth_c_active = true;
    assert_eq!(scene_mask(&s), 0b100_0100);
}

#[test]
fn on_tap_switches_off_then_restores_the_same_layers() {
    let mut s = st();
    s.piano_a_active = true;
    s.piano_b_active = true;
    let mut out = vec![];
    on_tap(&mut s, &mut out);
    assert!(!s.piano_a_active && !s.piano_b_active);
    assert_eq!(out.len(), 2);
    out.clear();
    on_tap(&mut s, &mut out);
    assert!(s.piano_a_active && s.piano_b_active);
    assert_eq!(s.fx_focus, 1);
}

#[test]
fn solo_keeps_only_the_piano_and_restores_the_scene() {
    let mut s = st();
    s.organ_a_active = true;
    s.piano_a_active = true;
    let before = scene_mask(&s);
    let mut out = vec![];
    solo_start(&mut s, &mut out);
    assert_eq!(out, [PianoEdit::SceneMask(0b100)]);
    solo_start(&mut s, &mut out);
    assert_eq!(out.len(), 1, "second start is ignored");
    solo_end(&mut s, &mut out);
    assert_eq!(out[1], PianoEdit::SceneMask(before));
    solo_end(&mut s, &mut out);
    assert_eq!(out.len(), 2);
}

#[test]
fn solo_turns_layer_a_on_when_the_piano_was_off() {
    let mut s = st();
    s.piano_a_active = false;
    let mut out = vec![];
    solo_start(&mut s, &mut out);
    assert_eq!(out, [PianoEdit::SceneMask(0b100)]);
}

#[test]
fn layer_tap_switches_replaces_or_adds() {
    let mut s = st();
    let mut out = vec![];
    layer_tap(&mut s, 1, false, &mut out);
    assert!(s.piano_b_active && !s.piano_a_active);
    assert_eq!(s.piano_ui.focus, 1);
    layer_tap(&mut s, 0, true, &mut out);
    assert!(s.piano_a_active && s.piano_b_active);
    assert_eq!(s.piano_ui.focus, 0);
    out.clear();
    layer_tap(&mut s, 1, false, &mut out);
    assert!(out.is_empty(), "active layer only takes the focus");
    assert_eq!(s.piano_ui.focus, 1);
}

#[test]
fn layer_hold_turns_off_only_when_the_other_layer_stays() {
    let mut s = st();
    let mut out = vec![];
    layer_hold(&mut s, 0, &mut out);
    assert!(s.piano_a_active && out.is_empty());
    s.piano_b_active = true;
    layer_hold(&mut s, 0, &mut out);
    assert!(!s.piano_a_active);
    assert_eq!(s.piano_ui.focus, 1);
}

#[test]
fn fader_clamps_and_targets_the_layer() {
    let mut s = st();
    let mut out = vec![];
    set_level(&mut s, 1, 1.5, &mut out);
    assert_eq!(s.piano_b_level, 1.0);
    assert_eq!(
        out,
        [PianoEdit::Level {
            layer: 1,
            level: 1.0
        }]
    );
}

#[test]
fn octave_steps_clamp_and_follow_the_focus() {
    let mut s = st();
    let mut out = vec![];
    for _ in 0..5 {
        octave_step(&mut s, 1, &mut out);
    }
    assert_eq!(s.piano_ui.octave[0], 2);
    assert_eq!(s.piano_octave, 2);
    s.piano_ui.focus = 1;
    octave_step(&mut s, -1, &mut out);
    assert_eq!(s.piano_ui.octave, [2, -1]);
    assert_eq!(
        out.last(),
        Some(&PianoEdit::OctaveShift {
            layer: 1,
            octaves: -1
        })
    );
}

#[test]
fn zone_step_wraps_over_four_leds() {
    let mut s = st();
    for _ in 0..5 {
        zone_step(&mut s, true);
    }
    assert_eq!(s.piano_ui.kb_zone[0], 1);
    zone_step(&mut s, false);
    zone_step(&mut s, false);
    assert_eq!(s.piano_ui.kb_zone[0], 3);
}

#[test]
fn type_cycles_through_all_six_and_wraps() {
    let mut s = st();
    let mut out = vec![];
    for _ in 0..6 {
        type_cycle(&mut s, &mut out);
    }
    assert_eq!(s.piano_cat, PianoCategory::Grand);
    assert_eq!(out[0], PianoEdit::Category(PianoCategory::Upright));
    assert_eq!(out.len(), 6);
}

#[test]
fn acoustics_toggles_emit_their_edits() {
    let mut s = st();
    let mut out = vec![];
    soft_release_toggle(&mut s, &mut out);
    string_res_toggle(&mut s, &mut out);
    ped_noise_toggle(&mut s, &mut out);
    assert!(s.piano_ui.soft_release[0]);
    assert!(!s.string_res && !s.pedal_noise);
    assert_eq!(
        out,
        [
            PianoEdit::SoftRelease { layer: 0, on: true },
            PianoEdit::StringRes(false),
            PianoEdit::PedalNoise(false)
        ]
    );
}

#[test]
fn touch_cycles_heavy_medium_light() {
    let mut s = st();
    let mut out = vec![];
    assert_eq!(s.piano_touch, VelocityTouch::Medium);
    touch_cycle(&mut s, &mut out);
    assert_eq!(s.piano_touch, VelocityTouch::Light);
    touch_cycle(&mut s, &mut out);
    assert_eq!(s.piano_touch, VelocityTouch::Heavy);
}

#[test]
fn dyn_comp_and_unison_cycle_off_1_2_3_per_layer() {
    let mut s = st();
    s.piano_ui.focus = 1;
    let mut out = vec![];
    for _ in 0..3 {
        dyn_comp_cycle(&mut s, &mut out);
        unison_cycle(&mut s, &mut out);
    }
    assert_eq!(s.piano_ui.dyn_comp, [0, 3]);
    assert_eq!(s.piano_ui.unison, [0, 3]);
    dyn_comp_cycle(&mut s, &mut out);
    assert_eq!(s.piano_ui.dyn_comp[1], 0);
    assert_eq!(
        out.last(),
        Some(&PianoEdit::DynComp {
            layer: 1,
            amount: 0
        })
    );
}

#[test]
fn timbre_steps_the_engine_list_and_keeps_clav_ui_only() {
    let mut s = st();
    let mut out = vec![];
    timbre_cycle(&mut s, &mut out);
    assert_eq!(s.piano_timbre, PianoTimbre::Soft);
    s.piano_cat = PianoCategory::Clav;
    timbre_cycle(&mut s, &mut out);
    assert_eq!(s.piano_ui.clav_timbre, 1);
    assert_eq!(s.piano_timbre, PianoTimbre::Soft, "engine timbre untouched");
    assert_eq!(out.len(), 1, "no engine edit for Clav");
    for _ in 0..7 {
        timbre_cycle(&mut s, &mut out);
    }
    assert_eq!(s.piano_ui.clav_timbre, 0);
}

#[test]
fn model_step_cycles_and_requests_the_entry() {
    let mut s = st();
    assert!(!model_step(&mut s, true));
    s.piano_ui.models = vec!["a.npno".into(), "b.npno".into(), "c.npno".into()];
    assert!(model_step(&mut s, true));
    assert_eq!(s.piano_ui.model_request.as_deref(), Some("b.npno"));
    assert!(model_step(&mut s, false));
    assert!(model_step(&mut s, false));
    assert_eq!(s.piano_ui.model_request.as_deref(), Some("c.npno"));
}
