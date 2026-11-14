//! Click tests of the Organ / Master controls that the engine honours since the poly percussion,
//! Drawbar Live, Aux KB, zone and rotary work landed: each click must send the right `OrganEdit`
//! (or layer command) and the edit must change the engine.

use super::layout_px::rect_or_empty;
use super::prelude::*;
use super::sim::Sim;
use crate::audio::OrganEdit;
use egui::Modifiers;
use nord_core::morph::LayerId;

const SHIFT: Modifiers = Modifiers {
    alt: false,
    ctrl: false,
    shift: true,
    mac_cmd: false,
    command: false,
};

/// Replays every queued command on a fresh engine.
fn engine_after(sim: &Sim) -> Stage4Engine {
    let mut engine = Stage4Engine::new(48_000.0);
    let mut master = 0.85;
    for c in sim.cmds() {
        apply_audio_command(c, &mut engine, &mut master);
    }
    engine
}

#[test]
fn on_switches_the_whole_section_off_and_back_on() {
    let mut sim = Sim::new();
    sim.click("organ.layer.b"); // B joins A
    sim.cmds();
    sim.click("organ.on");
    let cmds = sim.cmds();
    for l in [0usize, 1] {
        assert!(
            cmds.iter()
                .any(|c| matches!(c, AudioCommand::SetLayerActive(i, false) if *i == l)),
            "layer {l} not switched off: {cmds:?}"
        );
    }
    assert!(!sim.state.organ_section_on());
    sim.click("organ.on");
    let cmds = sim.cmds();
    assert!(cmds
        .iter()
        .any(|c| matches!(c, AudioCommand::SetLayerActive(0, true))));
    assert!(cmds
        .iter()
        .any(|c| matches!(c, AudioCommand::SetLayerActive(1, true))));
    assert!(sim.state.organ_a_active && sim.state.organ_b_active);
}

#[test]
fn on_off_takes_both_organ_layers_out_of_the_engine_scene() {
    let mut sim = Sim::new();
    sim.click("organ.layer.b");
    sim.click("organ.on");
    let e = engine_after(&sim);
    assert!(!e.scene.is_layer_active(LayerId::OrganA));
    assert!(!e.scene.is_layer_active(LayerId::OrganB));
    assert!(
        e.scene.is_layer_active(LayerId::PianoA),
        "other sections stay"
    );
}

#[test]
fn poly_percussion_reaches_both_organ_layers() {
    let mut sim = Sim::new();
    sim.tap_with("organ.perc_volume", SHIFT);
    assert_eq!(sim.organ_edits(), vec![OrganEdit::PercPoly(true)]);
    sim.time += 2.0; // not a double tap
    sim.tap_with("organ.perc_volume", SHIFT);
    let mut e = Stage4Engine::new(48_000.0);
    OrganEdit::PercPoly(true).apply(&mut e);
    assert!(e.organ_a.percussion_poly && e.organ_b.percussion_poly);
    assert!(
        !sim.state.organ_ui.perc_poly,
        "second Shift+tap switches it off"
    );
}

#[test]
fn preset_tap_enters_drawbar_live_and_the_sliders_stop_editing_the_preset() {
    let mut sim = Sim::new();
    sim.click("organ.preset");
    assert!(sim.state.organ_ui.drawbar_live);
    let edits = sim.organ_edits();
    assert!(matches!(edits[0], OrganEdit::LiveDrawbars(_)));
    assert_eq!(edits[1], OrganEdit::DrawbarLive(true));

    let r = rect_or_empty("organ.drawbar.4", sim.origin);
    sim.drag(r.center_top(), r.center_bottom());
    let cmds = sim.cmds();
    assert!(
        !cmds
            .iter()
            .any(|c| matches!(c, AudioCommand::SetDrawbars(_))),
        "the stored preset must not change in Drawbar Live"
    );
    assert_eq!(sim.state.organ_drawbars[3], 0, "preset untouched");
    assert!(sim.state.organ_ui.live_bars[3] > 0, "physical bar moved");
    let live = cmds
        .iter()
        .filter_map(|c| match c {
            AudioCommand::Organ(OrganEdit::LiveDrawbars(b)) => Some(*b),
            _ => None,
        })
        .next_back()
        .expect("physical positions sent");
    assert!(live[3] > 0);

    let mut e = Stage4Engine::new(48_000.0);
    OrganEdit::DrawbarLive(true).apply(&mut e);
    OrganEdit::LiveDrawbars(live).apply(&mut e);
    assert_eq!(e.organ_a.live_drawbars, live);
    assert_eq!(e.organ_a.drawbars[3], 0);
    assert_eq!(e.organ_a.active_drawbars(), &live, "Live plays the bars");
}

#[test]
fn shift_preset_is_sync_and_copies_the_physical_bars_into_the_preset() {
    let mut sim = Sim::new();
    sim.click("organ.preset");
    let r = rect_or_empty("organ.drawbar.4", sim.origin);
    sim.drag(r.center_top(), r.center_bottom());
    sim.cmds();
    sim.tap_with("organ.preset", SHIFT);
    assert!(
        sim.state.organ_drawbars[3] > 0,
        "panel preset took the bars"
    );
    assert!(sim.state.organ_ui.drawbar_live, "SYNC does not leave Live");
    let cmds = sim.cmds();
    assert!(cmds
        .iter()
        .any(|c| matches!(c, AudioCommand::Organ(OrganEdit::SyncPreset))));
    let mut e = Stage4Engine::new(48_000.0);
    let mut master = 0.85;
    for c in cmds {
        apply_audio_command(c, &mut e, &mut master);
    }
    assert_eq!(e.organ_a.drawbars, e.organ_a.live_drawbars);
    assert!(e.organ_b.drawbars[3] > 0);
}

#[test]
fn preset_mode_drawbars_still_edit_the_preset_and_the_live_bars_follow() {
    let mut sim = Sim::new();
    let r = rect_or_empty("organ.drawbar.6", sim.origin);
    sim.drag(r.center_top(), r.center_bottom());
    let cmds = sim.cmds();
    assert!(cmds
        .iter()
        .any(|c| matches!(c, AudioCommand::SetDrawbars(d) if d[5] > 0)));
    assert_eq!(sim.state.organ_ui.live_bars, sim.state.organ_drawbars);
}

#[test]
fn shifted_octave_buttons_assign_the_focused_layer_to_a_zone() {
    let mut sim = Sim::new();
    sim.click("organ.layer.b"); // focus B
    sim.cmds();
    sim.tap_with("organ.octave_up", SHIFT);
    sim.time += 2.0;
    sim.tap_with("organ.octave_up", SHIFT);
    let edits = sim.organ_edits();
    assert_eq!(
        edits.last(),
        Some(&OrganEdit::Zone {
            layer: 1,
            leds: 0b0010
        })
    );
    assert_eq!(sim.state.organ_ui.kb_zone, [0, 2]);
    let mut e = Stage4Engine::new(48_000.0);
    for ed in edits {
        ed.apply(&mut e);
    }
    assert_eq!(e.layer_zone_leds(LayerId::OrganB), 0b0010);
    assert_eq!(e.layer_zone_leds(LayerId::OrganA), 0b1111);
    sim.time += 2.0;
    sim.tap_with("organ.octave_down", SHIFT);
    assert_eq!(
        sim.organ_edits(),
        vec![OrganEdit::Zone {
            layer: 1,
            leds: 0b0001
        }]
    );
}

#[test]
fn aux_led_click_toggles_the_layer_flag_and_the_engine_routing() {
    let mut sim = Sim::new();
    sim.click("organ.aux_led.b");
    assert_eq!(
        sim.organ_edits(),
        vec![OrganEdit::AuxKb { layer: 1, on: true }]
    );
    assert!(sim.state.organ_ui.aux[1] && !sim.state.organ_ui.aux[0]);
    let mut e = Stage4Engine::new(48_000.0);
    OrganEdit::AuxKb { layer: 1, on: true }.apply(&mut e);
    assert!(e.aux_kb.is_aux_layer(LayerId::OrganB));
    sim.click("organ.aux_led.b");
    assert_eq!(
        sim.organ_edits(),
        vec![OrganEdit::AuxKb {
            layer: 1,
            on: false
        }]
    );
}

#[test]
fn rotary_organ_and_angle_buttons_change_the_rotary_engine_fields() {
    let mut sim = Sim::new();
    sim.click("rotary.organ");
    assert_eq!(sim.organ_edits(), vec![OrganEdit::RotaryOrgan(false)]);
    assert!(!sim.state.organ_ui.rotary_organ);
    // Shift + STOP MODE opens the ANGLE popup; its last row is the wide / narrow mic toggle.
    sim.tap_with("rotary.stop_mode", SHIFT);
    assert!(sim.state.organ_ui.stop_angle_open);
    assert!(sim.organ_edits().is_empty());
    sim.frame(vec![], egui::Modifiers::NONE);
    let rect = super::rotor_angle::popup_rect(rect_or_empty("rotary.stop_mode", sim.origin));
    let mic_row = pos2(rect.center().x, rect.bottom() - 6.0);
    sim.frame(
        vec![egui::Event::PointerMoved(mic_row)],
        egui::Modifiers::NONE,
    );
    sim.press(mic_row, true, egui::Modifiers::NONE);
    sim.press(mic_row, false, egui::Modifiers::NONE);
    let edits = sim.organ_edits();
    assert_eq!(edits, vec![OrganEdit::RotaryAngle { wide: false }]);
    let mut e = Stage4Engine::new(48_000.0);
    OrganEdit::RotaryOrgan(false).apply(&mut e);
    for ed in edits {
        ed.apply(&mut e);
    }
    assert!(!e.organ_a.rotary_enabled && !e.organ_b.rotary_enabled);
    assert!(e.organ_a.rotary_mic_angle_deg < 120.0);
    sim.press(mic_row, true, egui::Modifiers::NONE);
    sim.press(mic_row, false, egui::Modifiers::NONE);
    assert_eq!(
        sim.organ_edits(),
        vec![OrganEdit::RotaryAngle { wide: true }]
    );
}

#[test]
fn stop_angle_popup_picks_an_angle_sends_it_to_the_engine_and_closes() {
    let mut sim = Sim::new();
    sim.tap_with("rotary.stop_mode", SHIFT);
    sim.frame(vec![], egui::Modifiers::NONE);
    let rect = super::rotor_angle::popup_rect(rect_or_empty("rotary.stop_mode", sim.origin));
    // rows: title, Free, 1..6, mic; row 3 is "2: Front Left"
    let row = pos2(rect.center().x, rect.top() + 4.0 + 3.5 * 11.0);
    sim.frame(vec![egui::Event::PointerMoved(row)], egui::Modifiers::NONE);
    sim.press(row, true, egui::Modifiers::NONE);
    sim.press(row, false, egui::Modifiers::NONE);
    let edits = sim.organ_edits();
    assert_eq!(edits, vec![OrganEdit::RotaryStopAngle(2)]);
    assert!(!sim.state.organ_ui.stop_angle_open);
    let mut e = Stage4Engine::new(48_000.0);
    edits[0].apply(&mut e);
    assert_eq!(e.organ_a.rotary_stop_angle, 2);
}

#[test]
fn no_organ_or_master_control_is_dimmed_any_more() {
    let sim = Sim::new();
    let dimmed = super::ctl::painted(&sim.ctx).dimmed;
    assert!(dimmed.is_empty(), "still dimmed: {dimmed:?}");
}
