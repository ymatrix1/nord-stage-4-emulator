//! Extern layer MIDI output tests.

use super::extern_out::*;
use super::*;

fn engine() -> Stage4Engine {
    let mut e = Stage4Engine::new(48_000.0);
    e.extern_out.layers[0].enabled = true;
    e
}

fn drain(e: &mut Stage4Engine) -> Vec<MidiOut> {
    std::iter::from_fn(|| e.pop_midi_out()).collect()
}

fn note(e: &mut Stage4Engine, on: bool, n: u8) {
    e.handle_midi(if on {
        MidiEvent::NoteOn {
            channel: 0,
            note: n,
            velocity: 90,
        }
    } else {
        MidiEvent::NoteOff {
            channel: 0,
            note: n,
            velocity: 0,
        }
    });
}

#[test]
fn extern_layer_emits_transposed_notes_and_is_silent() {
    let mut e = engine();
    e.extern_out.layers[0].octave_shift = 1;
    note(&mut e, true, 60);
    e.extern_out.layers[0].octave_shift = -2;
    note(&mut e, false, 60);
    let out = drain(&mut e);
    assert_eq!(out[0], MidiOut::note_on(12, 72, 90));
    assert_eq!(
        out[1],
        MidiOut::note_off(12, 72),
        "off matches the sent note"
    );
    assert_eq!(out.len(), 2);
    note(&mut e, true, 62);
    assert!((0..256).all(|_| e.synth_a.render_frame() == [0.0, 0.0]));
}

#[test]
fn extern_notes_respect_zones_and_range() {
    let mut e = engine();
    e.split_router.splits[0].enabled = true;
    e.split_router.splits[0].midi_note = 60;
    e.split_router.layer_zone_masks[LayerId::SynthA as usize] = 0b0010;
    note(&mut e, true, 40);
    assert!(
        drain(&mut e).is_empty(),
        "note below the split is outside the zone"
    );
    note(&mut e, true, 72);
    assert_eq!(drain(&mut e).len(), 1);
    e.extern_out.layers[0].octave_shift = 4;
    note(&mut e, true, 90);
    assert!(drain(&mut e).is_empty(), "transposed past 127 is dropped");
}

#[test]
fn cc_program_and_send_on_load() {
    let mut e = engine();
    e.extern_out.layers[0].cc_numbers = [74, 71];
    e.extern_set_cc_value(0, 1, 99);
    assert_eq!(drain(&mut e), vec![MidiOut::control_change(12, 71, 99)]);
    e.extern_out.layers[0].bank_msb = 1;
    e.extern_set_program(0, 7);
    assert_eq!(
        drain(&mut e),
        vec![
            MidiOut::control_change(12, 0, 1),
            MidiOut::control_change(12, 32, 0),
            MidiOut::program_change(12, 7)
        ]
    );
    e.extern_send_on_load();
    assert!(drain(&mut e).is_empty(), "Send On Load is off by default");
    e.extern_out.layers[0].send_on_load = true;
    e.extern_send_on_load();
    let out = drain(&mut e);
    assert_eq!(out.len(), 3 + 2 + 1);
    assert_eq!(out[5], MidiOut::control_change(12, 7, 102));
}

#[test]
fn fader_sends_volume_cc7_on_change_only() {
    let mut e = engine();
    e.extern_sync_volume();
    assert!(drain(&mut e).is_empty());
    e.layer_levels[LayerId::SynthA as usize].base = 0.5;
    e.extern_sync_volume();
    e.extern_sync_volume();
    assert_eq!(drain(&mut e), vec![MidiOut::control_change(12, 7, 64)]);
}

#[test]
fn sustain_and_bend_forward_only_with_the_layer_flags() {
    let mut e = engine();
    e.handle_midi(MidiEvent::ControlChange {
        channel: 0,
        controller: cc::SUSTAIN_PEDAL,
        value: 127,
    });
    e.handle_midi(MidiEvent::PitchBend {
        channel: 0,
        value: 16383,
    });
    assert!(drain(&mut e).is_empty(), "SUSTPED / PSTICK are off");
    e.pedals.sustped[LayerId::SynthA as usize] = true;
    e.pedals.pstick[LayerId::SynthA as usize] = true;
    e.refresh_pedal_routing();
    assert_eq!(
        drain(&mut e),
        vec![
            MidiOut::control_change(12, 64, 127),
            MidiOut::pitch_bend(12, 16383)
        ]
    );
    e.refresh_pedal_routing();
    assert!(drain(&mut e).is_empty(), "no repeat while unchanged");
}

#[test]
fn disabled_extern_or_off_channel_is_quiet() {
    let mut e = Stage4Engine::new(48_000.0);
    note(&mut e, true, 60);
    assert!(drain(&mut e).is_empty());
    e.extern_out.layers[1].enabled = true;
    e.extern_out.layers[1].channel = None;
    e.layer_midi_channels[LayerId::SynthB as usize] = Some(3);
    e.scene.scene_1_mask |= 1 << LayerId::SynthB as u8;
    e.handle_midi(MidiEvent::NoteOn {
        channel: 3,
        note: 50,
        velocity: 80,
    });
    assert!(drain(&mut e).is_empty());
    e.extern_all_notes_off(1);
}
