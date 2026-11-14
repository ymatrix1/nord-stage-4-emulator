//! Tests of the engine telemetry snapshot (`telemetry.rs`).

#![allow(clippy::field_reassign_with_default)]

use super::*;
use crate::midi::MidiEvent;
use std::sync::Arc;

fn note_on(e: &mut Stage4Engine, note: u8) {
    e.handle_midi(MidiEvent::NoteOn {
        channel: 0,
        note,
        velocity: 100,
    });
}

#[test]
fn words_round_trip_every_field() {
    let mut t = Telemetry::default();
    t.layer_level = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7];
    t.active_mask = 0b101_0101;
    t.aux_mask = 0b010_0010;
    t.layer_held = [1, 2, 3, 4, 5, 6, 7];
    t.morph = [0.25, 0.5, 0.75];
    t.held_notes = [1, 2, 3, 0x8000_0000];
    t.split_enabled = 0b101;
    t.split_zone_gain = [1.0, 0.5, 0.25, 0.0];
    (t.midi_in_count, t.midi_out_count) = (u32::MAX, 77);
    (t.midi_out_dropped, t.midi_out_pending) = (5, 9);
    t.extern_mask = 0b110;
    t.organ_drawbars = [[8, 7, 6, 5, 4, 3, 2, 1, 0], [0, 1, 2, 3, 4, 5, 6, 7, 8]];
    t.rotary_speed = RotarySpeed::Fast;
    t.rotary_stop = true;
    t.scene_2 = true;
    t.transpose = -5;
    t.bpm = 133.5;
    t.arp_step = 3;
    t.undo = Some(UndoOp::ProgramChange);
    t.redo = Some(UndoOp::LoadPreset);
    assert_eq!(Telemetry::from_words(&t.to_words()), t);
    for speed in [RotarySpeed::Stop, RotarySpeed::Slow] {
        t.rotary_speed = speed;
        assert_eq!(Telemetry::from_words(&t.to_words()).rotary_speed, speed);
    }
    t.undo = None;
    t.redo = None;
    assert_eq!(Telemetry::from_words(&t.to_words()), t);
}

#[test]
fn fresh_engine_reports_defaults() {
    let e = Stage4Engine::new(48_000.0);
    let t = e.telemetry();
    assert_eq!(t.layer_level, [0.8; 7]);
    assert!(t.layer_active(0) && t.layer_active(2) && t.layer_active(4));
    assert!(!t.layer_active(1) && !t.layer_active(7));
    assert_eq!(t.aux_mask, 0);
    assert_eq!(t.held_notes, [0; 4]);
    assert_eq!(t.bpm, 120.0);
    assert_eq!(t.organ_drawbars[0], [8, 8, 8, 0, 0, 0, 0, 0, 0]);
    assert_eq!(t.rotary_speed, RotarySpeed::Slow);
    assert_eq!((t.undo, t.redo), (None, None));
    assert_eq!(t.split_leds(), [false; 3]);
}

#[test]
fn level_is_the_morphed_level_not_the_fader() {
    let mut e = Stage4Engine::new(48_000.0);
    e.layer_levels[2].base = 0.4;
    e.layer_levels[2].delta_wheel = 0.5;
    assert!((e.telemetry().layer_level[2] - 0.4).abs() < 1e-6);
    e.morph_controllers.mod_wheel = 1.0;
    let t = e.telemetry();
    assert!((t.layer_level[2] - 0.9).abs() < 1e-6, "{:?}", t.layer_level);
    assert_eq!(t.morph[0], 1.0);
    e.layer_levels[2].delta_wheel = -1.0;
    assert_eq!(e.telemetry().layer_level[2], 0.0, "clamped like the mix");
}

#[test]
fn held_keys_midi_counters_and_transpose_follow_the_engine() {
    let mut e = Stage4Engine::new(48_000.0);
    e.set_transpose(3);
    note_on(&mut e, 60);
    note_on(&mut e, 100);
    let t = e.telemetry();
    assert!(t.note_held(60) && t.note_held(100) && !t.note_held(61));
    assert_eq!(t.midi_in_count, 2);
    assert_eq!(t.transpose, 3);
    assert!(t.layer_held[0] >= 1 && t.layer_held[1] == 0);
    e.handle_midi(MidiEvent::NoteOff {
        channel: 0,
        note: 60,
        velocity: 0,
    });
    let t = e.telemetry();
    assert!(!t.note_held(60) && t.note_held(100));
    assert_eq!(t.midi_in_count, 3);
}

#[test]
fn aux_split_scene_and_rotary_follow_the_engine() {
    let mut e = Stage4Engine::new(48_000.0);
    e.aux_kb.enabled = true;
    e.aux_kb.set_section_masks(0b10, 0b01, 0);
    let t = e.telemetry();
    assert!(t.layer_aux(1) && t.layer_aux(2) && !t.layer_aux(0));
    e.split_router.splits[1].enabled = true;
    e.organ_a.rotary_speed = RotarySpeed::Fast;
    e.organ_a.rotary_stop_mode = true;
    e.scene.scene_2_active = true;
    let t = e.telemetry();
    assert_eq!(t.split_leds(), [false, true, false]);
    assert_eq!(t.rotary_speed, RotarySpeed::Fast);
    assert!(t.rotary_stop && t.scene_2);
    assert_eq!(t.active_mask, 0b111_1111, "scene II lights every layer");
    assert!(t.split_zone_gain.iter().any(|&g| g > 0.0));
}

#[test]
fn extern_output_and_overflow_counters() {
    let mut e = Stage4Engine::new(48_000.0);
    e.extern_out.layers[0].enabled = true;
    assert_eq!(e.telemetry().extern_mask, 0b001);
    for _ in 0..MIDI_OUT_CAPACITY + 3 {
        e.extern_out.queue.push(MidiOut::default());
    }
    let t = e.telemetry();
    assert_eq!(t.midi_out_count, MIDI_OUT_CAPACITY as u32);
    assert_eq!(t.midi_out_dropped, 3);
    assert_eq!(t.midi_out_pending, MIDI_OUT_CAPACITY as u32);
}

#[test]
fn undo_and_redo_availability_are_published() {
    let mut e = Stage4Engine::new(48_000.0);
    e.all_fx_off();
    assert_eq!(e.telemetry().undo, Some(UndoOp::AllFxOff));
    e.undo();
    let t = e.telemetry();
    assert_eq!((t.undo, t.redo), (None, Some(UndoOp::AllFxOff)));
}

#[test]
fn clock_step_follows_the_beat_phase() {
    let mut e = Stage4Engine::new(48_000.0);
    e.master_clock.beat_phase = 0.6;
    assert_eq!(e.telemetry().arp_step, 2);
    e.master_clock.beat_phase = 5.0;
    assert_eq!(e.telemetry().arp_step, 3);
}

#[test]
fn publish_reaches_the_attached_cell_and_is_a_noop_without_one() {
    let mut e = Stage4Engine::new(48_000.0);
    e.publish_telemetry();
    let cell = Arc::new(TelemetryCell::new());
    e.attach_telemetry(Arc::clone(&cell));
    assert!(cell.read().is_none());
    e.master_clock.bpm = 99.0;
    let mut l = [0.0; 16];
    let mut r = [0.0; 16];
    e.process_block(&mut l, &mut r);
    assert_eq!(cell.read().map(|t| t.bpm), Some(99.0));
}
