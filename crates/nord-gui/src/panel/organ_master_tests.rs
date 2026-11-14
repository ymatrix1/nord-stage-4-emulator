//! Headless tests of the Master + Organ bays: every inventory id is painted at its layout
//! rectangle, every `Unsupported` control is dimmed, and clicks reach the engine commands.

use super::ctl::painted;
use super::layout_px::rect_or_empty;
use super::prelude::*;
use super::sim::Sim;
use crate::audio::OrganEdit;
use egui::{Modifiers, Shape};
use nord_panel::{all_controls, Access, Binding, ControlEntry, Section};

fn bay_controls() -> Vec<ControlEntry> {
    all_controls()
        .into_iter()
        .filter(|c| matches!(c.section, Section::Master | Section::Organ))
        .collect()
}

fn is_unsupported(c: &ControlEntry) -> bool {
    matches!(c.binding, Binding::Unsupported(_))
}

#[test]
fn bay_has_sixty_inventory_controls() {
    assert_eq!(bay_controls().len(), 61);
}

#[test]
fn every_inventory_id_is_painted() {
    let sim = Sim::new();
    let drawn = painted(&sim.ctx).drawn;
    let missing: Vec<_> = bay_controls()
        .into_iter()
        .filter(|c| !drawn.contains(&c.id))
        .map(|c| c.id)
        .collect();
    assert!(missing.is_empty(), "not painted: {missing:?}");
}

#[test]
fn every_id_has_shapes_inside_its_layout_rect() {
    let sim = Sim::new();
    let boxes: Vec<(Rect, bool)> = sim
        .shapes
        .iter()
        .map(|s| {
            (
                s.shape.visual_bounding_rect(),
                matches!(s.shape, Shape::Text(_)),
            )
        })
        .filter(|(r, _)| r.width() * r.height() < 2500.0)
        .collect();
    let mut bad = Vec::new();
    for c in bay_controls() {
        let r = rect_or_empty(&c.id, sim.origin);
        let ok = if c.access == Access::Direct {
            boxes
                .iter()
                .any(|(b, _)| r.expand(2.5).contains(b.center()))
        } else {
            // Shift / Hold functions print a legend text just below the shared button.
            boxes.iter().any(|(b, text)| {
                *text
                    && b.top() >= r.bottom() - 1.0
                    && b.top() <= r.bottom() + 4.0
                    && (b.center().x - r.center().x).abs() < 14.0
            })
        };
        if !ok {
            bad.push(c.id);
        }
    }
    assert!(
        bad.is_empty(),
        "nothing painted at the layout rect of: {bad:?}"
    );
}

#[test]
fn unsupported_controls_are_dimmed_and_working_ones_are_not() {
    let sim = Sim::new();
    let dimmed = painted(&sim.ctx).dimmed;
    for c in bay_controls() {
        let want = is_unsupported(&c);
        assert_eq!(
            dimmed.contains(&c.id),
            want,
            "{}: dimmed={} but inventory binding is {:?}",
            c.id,
            !want,
            c.binding
        );
    }
}

#[test]
fn model_button_cycles_the_six_models_and_wraps() {
    let mut sim = Sim::new();
    sim.click("organ.model");
    assert!(matches!(
        sim.organ_edits().as_slice(),
        [OrganEdit::Model {
            model: OrganModel::B3,
            b3_bass: true
        }]
    ));
    assert!(sim.state.organ_ui.b3_bass);
    for _ in 0..5 {
        sim.click("organ.model");
    }
    let edits = sim.organ_edits();
    assert_eq!(edits.len(), 5);
    assert_eq!(
        edits.last(),
        Some(&OrganEdit::Model {
            model: OrganModel::B3,
            b3_bass: false
        }),
        "six presses return to B3"
    );
}

#[test]
fn octave_buttons_target_the_focused_layer() {
    let mut sim = Sim::new();
    sim.click("organ.octave_up");
    sim.click("organ.layer.b"); // focus B (switches it on)
    sim.click("organ.octave_down");
    let edits = sim.organ_edits();
    assert!(edits.contains(&OrganEdit::Octave { layer: 0, shift: 1 }));
    assert!(edits.contains(&OrganEdit::Octave {
        layer: 1,
        shift: -1
    }));
    assert!(sim.state.organ_b_active);
}

#[test]
fn shift_tap_on_octave_moves_the_kb_zone_instead() {
    let mut sim = Sim::new();
    sim.tap_with(
        "organ.octave_up",
        Modifiers {
            shift: true,
            ..Modifiers::NONE
        },
    );
    assert_eq!(sim.state.organ_ui.kb_zone[0], 1);
    assert_eq!(
        sim.organ_edits(),
        vec![OrganEdit::Zone {
            layer: 0,
            leds: 0b0001
        }],
        "shifted function sends the zone, no octave"
    );
}

#[test]
fn hold_on_a_layer_button_reaches_sustped() {
    let mut sim = Sim::new();
    sim.hold("organ.layer.a", 0.7);
    assert!(sim.state.organ_ui.sustped[0]);
    assert_eq!(
        sim.organ_edits(),
        vec![OrganEdit::Sustped { layer: 0, on: true }]
    );
    assert!(
        sim.state.organ_a_active,
        "hold does not switch the layer off"
    );
}

#[test]
fn hold_on_on_button_toggles_solo() {
    let mut sim = Sim::new();
    sim.hold("organ.on", 0.7);
    assert!(sim.state.organ_ui.solo);
    assert!(sim.state.organ_section_on(), "hold is SOLO, not ON/OFF");
}

#[test]
fn percussion_buttons_send_state() {
    let mut sim = Sim::new();
    sim.click("organ.perc_third");
    let last = sim.cmds().into_iter().last();
    assert!(matches!(
        last,
        Some(AudioCommand::SetPercussion {
            enabled: true,
            third: true,
            ..
        })
    ));
    sim.click("organ.perc_on");
    assert!(!sim.state.perc_on);
    sim.tap_with(
        "organ.perc_volume",
        Modifiers {
            shift: true,
            ..Modifiers::NONE
        },
    );
    assert!(sim.state.organ_ui.perc_poly && !sim.state.perc_soft);
    assert_eq!(sim.organ_edits(), vec![OrganEdit::PercPoly(true)]);
}

#[test]
fn vibrato_type_and_on_buttons_drive_the_scanner() {
    let mut sim = Sim::new();
    sim.click("organ.vibrato_on.a");
    sim.click("organ.vibrato_selector");
    let edits = sim.organ_edits();
    assert_eq!(
        edits[0],
        OrganEdit::Scanner {
            layer: 0,
            mode: ScannerMode::C3
        }
    );
    // Default type C3 -> next is V1 (wraps).
    assert_eq!(
        edits[1],
        OrganEdit::Scanner {
            layer: 0,
            mode: ScannerMode::V1
        }
    );
    sim.click("organ.vibrato_on.a");
    assert_eq!(
        sim.organ_edits()[0],
        OrganEdit::Scanner {
            layer: 0,
            mode: ScannerMode::Off
        }
    );
}

#[test]
fn drawbar_drag_sends_new_values_and_preset_toggles_the_ladder_mode() {
    let mut sim = Sim::new();
    let r = rect_or_empty("organ.drawbar.4", sim.origin);
    sim.drag(r.center_top(), r.center_bottom());
    assert!(sim.state.organ_drawbars[3] > 0);
    assert!(sim
        .cmds()
        .iter()
        .any(|c| matches!(c, AudioCommand::SetDrawbars(d) if d[3] > 0)));
    sim.click("organ.preset");
    assert!(sim.state.organ_ui.drawbar_live);
}

#[test]
fn level_fader_drag_sends_level() {
    let mut sim = Sim::new();
    let r = rect_or_empty("organ.fader.a", sim.origin);
    sim.drag(r.center_bottom(), r.center_top());
    assert!(sim.state.organ_a_level > 0.9);
    assert!(sim
        .cmds()
        .iter()
        .any(|c| matches!(c, AudioCommand::SetOrganLevel(l) if *l > 0.9)));
}

#[test]
fn rotary_buttons_send_speed_stop_and_close_mic() {
    let mut sim = Sim::new();
    sim.click("rotary.speed");
    assert!(matches!(
        sim.organ_edits().as_slice(),
        [OrganEdit::Rotary {
            fast: true,
            stop_mode: false
        }]
    ));
    sim.click("rotary.stop_mode");
    assert!(matches!(
        sim.organ_edits().as_slice(),
        [OrganEdit::Rotary {
            fast: true,
            stop_mode: true
        }]
    ));
    sim.tap_with(
        "rotary.organ",
        Modifiers {
            shift: true,
            ..Modifiers::NONE
        },
    );
    assert_eq!(sim.organ_edits(), vec![OrganEdit::CloseMic(true)]);
}

#[test]
fn master_level_and_drive_knobs_send_commands() {
    let mut sim = Sim::new();
    let at = sim.center("master.level");
    sim.drag(at, at + vec2(0.0, -30.0));
    assert!(sim
        .cmds()
        .iter()
        .any(|c| matches!(c, AudioCommand::SetMasterVolume(v) if *v > 0.85)));
    let at = sim.center("rotary.drive");
    sim.drag(at, at + vec2(0.0, -30.0));
    assert!(sim
        .cmds()
        .iter()
        .any(|c| matches!(c, AudioCommand::SetRotaryDrive(v) if *v > 0.25)));
}

#[test]
fn mod_wheel_drag_sends_cc1() {
    let mut sim = Sim::new();
    let r = rect_or_empty("master.mod_wheel", sim.origin);
    sim.drag(r.center_bottom(), r.center_top());
    assert!(sim.state.mod_wheel > 0.0);
    assert!(sim.cmds().iter().any(|c| matches!(
        c,
        AudioCommand::Keyboard(MidiEvent::ControlChange { controller: 1, .. })
    )));
}

#[test]
fn pitch_stick_drag_sends_keyboard_pitch_bend() {
    let mut sim = Sim::new();
    let r = rect_or_empty("master.pitch_stick", sim.origin);
    sim.drag(r.center(), r.center() + vec2(r.width() * 0.4, 0.0));
    assert!(sim.cmds().iter().any(|c| matches!(
        c,
        AudioCommand::Keyboard(MidiEvent::PitchBend { value, .. }) if *value > 8192
    )));
}
