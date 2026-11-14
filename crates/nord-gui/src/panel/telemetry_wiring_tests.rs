//! Headless tests that the panel shows what the engine publishes (bean `ns4-gjy2`): ladders, drawbar
//! ladders, Aux LEDs, rotary LEDs, MIDI / EXTERN LEDs, split LEDs and the UNDO / REDO soft buttons.

#![allow(clippy::field_reassign_with_default)]

use super::bay_program::model::Screen;
use super::layout_px::rect_or_empty;
use super::prelude::*;
use super::sim::Sim;
use super::ui_state::ProgramUi;
use crate::audio::ProgramEdit;
use crate::telemetry::PanelTelemetry;
use egui::Shape;
use nord_core::engine::Telemetry;
use nord_core::organ::RotarySpeed;

/// Shapes centred in `id`'s rect that are filled with `color` (rectangles and circles).
fn fills(sim: &Sim, id: &str, color: Color32) -> usize {
    let rect = rect_or_empty(id, sim.origin).expand(1.0);
    sim.shapes
        .iter()
        .filter(|s| match &s.shape {
            Shape::Rect(r) => r.fill == color && rect.contains(r.rect.center()),
            Shape::Circle(c) => c.fill == color && rect.contains(c.center),
            _ => false,
        })
        .count()
}

fn with_telemetry(sim: &mut Sim, edit: impl FnOnce(&mut Telemetry)) {
    let mut t = Telemetry::default();
    t.active_mask = 0b111_1111;
    edit(&mut t);
    sim.state.telemetry = PanelTelemetry(Some(t));
    sim.frame(vec![], egui::Modifiers::NONE);
}

#[test]
fn organ_ladder_shows_the_morphed_level_not_the_fader() {
    let mut sim = Sim::new();
    assert_eq!(sim.state.organ_a_level, 0.85);
    assert_eq!(
        fills(&sim, "organ.ladder.a", LED_RED),
        0,
        "UI 0.85 lights 9 of 11"
    );
    with_telemetry(&mut sim, |t| t.layer_level[0] = 1.0);
    assert_eq!(
        fills(&sim, "organ.ladder.a", LED_RED),
        1,
        "engine level 1.0 lights all 11"
    );
    with_telemetry(&mut sim, |t| t.layer_level[0] = 0.0);
    assert_eq!(
        fills(&sim, "organ.ladder.a", LED_GREEN),
        0,
        "engine level 0 lights none"
    );
}

#[test]
fn drawbar_ladder_shows_the_engine_drawbar() {
    let mut sim = Sim::new();
    assert_eq!(sim.state.organ_drawbars[0], 8);
    let lit = |sim: &Sim| fills(sim, "organ.drawbar_ladder.1", LED_RED);
    let before = lit(&sim);
    with_telemetry(&mut sim, |t| t.organ_drawbars[0][0] = 2);
    assert!(
        lit(&sim) < before,
        "morphed drawbar 2 lights fewer LEDs than the UI's 8"
    );
}

#[test]
fn aux_led_follows_the_engine_flag() {
    let mut sim = Sim::new();
    assert_eq!(fills(&sim, "organ.aux_led.b", LED_GREEN), 0);
    with_telemetry(&mut sim, |t| t.aux_mask = 0b10);
    assert_eq!(fills(&sim, "organ.aux_led.b", LED_GREEN), 1);
    assert_eq!(fills(&sim, "organ.aux_led.a", LED_GREEN), 0);
}

#[test]
fn rotary_leds_follow_the_real_rotor_state() {
    let mut sim = Sim::new();
    assert_eq!(sim.state.rotary_speed, RotarySpeed::Slow);
    assert_eq!(fills(&sim, "rotary.speed_leds", LED_RED), 0);
    with_telemetry(&mut sim, |t| t.rotary_speed = RotarySpeed::Fast);
    assert_eq!(
        fills(&sim, "rotary.speed_leds", LED_RED),
        1,
        "a pedal made the rotor fast"
    );
    assert_eq!(fills(&sim, "rotary.speed_leds", LED_GREEN), 0);
}

#[test]
fn panel_reads_telemetry_and_blinks_the_activity_leds() {
    use nord_core::engine::Stage4Engine;
    use nord_core::midi::MidiEvent;
    let ctx = egui::Context::default();
    let (audio, _rx) = AudioHandle::new();
    let mut state = Stage4PanelState::default();
    let mut engine = Stage4Engine::new(48_000.0);
    engine.attach_telemetry(audio.telemetry().cell());
    let run = |time: f64, state: &mut Stage4PanelState| {
        let input = egui::RawInput {
            time: Some(time),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                super::render_stage4_panel(ui, state, &audio);
            });
        });
    };
    run(1.0, &mut state);
    assert!(state.telemetry.get().is_none(), "nothing published yet");

    engine.publish_telemetry();
    run(2.0, &mut state);
    assert!(state.telemetry.get().is_some());
    assert!(!state.leds.midi_on(), "baseline frame is not activity");

    engine.handle_midi(MidiEvent::NoteOn {
        channel: 0,
        note: 60,
        velocity: 90,
    });
    engine
        .extern_out
        .queue
        .push(nord_core::engine::MidiOut::default());
    engine.publish_telemetry();
    run(3.0, &mut state);
    assert!(state.leds.midi_on() && state.leds.extern_on());
    run(3.5, &mut state);
    assert!(
        !state.leds.midi_on() && !state.leds.extern_on(),
        "blink expired"
    );
}

#[test]
fn split_leds_follow_the_engine_split_state() {
    use nord_core::engine::Stage4Engine;
    let mut e = Stage4Engine::new(48_000.0);
    e.split_router.splits[1].enabled = true;
    let t = PanelTelemetry(Some(e.telemetry()));
    assert_eq!(t.split_leds([true, false, false]), [false, true, false]);
}

#[test]
fn undo_soft_button_is_gated_by_availability_and_redo_is_offered() {
    let mut u = ProgramUi::default();
    u.screen = Screen::Undo;
    u.undo_blocked = true;
    assert!(u.soft_down(4).is_empty(), "nothing to undo: button inert");
    assert_eq!(u.screen, Screen::Home);

    u.screen = Screen::Undo;
    u.undo_blocked = false;
    u.redo_available = false;
    assert!(u.soft_down(3).is_empty(), "no redo available");
    u.screen = Screen::Undo;
    u.redo_available = true;
    let out = u.soft_down(3);
    assert!(matches!(
        &out[0],
        crate::audio::AudioCommand::Program(ProgramEdit::Redo)
    ));
}

#[test]
fn program_bay_publishes_undo_availability_into_its_ui_state() {
    use super::bay_program::render_bay_program;
    let mut e = nord_core::engine::Stage4Engine::new(48_000.0);
    e.all_fx_off();
    e.undo();
    let n = super::support::shapes_painted(move |ui, p, rect, state, audio| {
        state.telemetry = PanelTelemetry(Some(e.telemetry()));
        render_bay_program(ui, p, rect, state, audio);
        assert!(state.program_ui.redo_available);
        assert!(
            state.program_ui.undo_blocked,
            "undone: nothing left to undo"
        );
    });
    assert!(n > 0);
}
