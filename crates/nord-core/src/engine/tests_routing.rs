//! Engine MIDI routing tests: scenes, splits, channels and held-note bookkeeping.

use super::*;

#[test]
fn note_off_uses_note_on_recipients_even_after_scene_switch() {
    let mut engine = Stage4Engine::new(48_000.0);
    engine.handle_midi(MidiEvent::NoteOn {
        channel: 0,
        note: 60,
        velocity: 91,
    });
    let recipients = engine.note_recipients_for_test(0, 60);
    assert_eq!(
        recipients,
        (1 << LayerId::OrganA as u8) | (1 << LayerId::PianoA as u8) | (1 << LayerId::SynthA as u8)
    );
    engine.scene.scene_2_active = true;
    engine.handle_midi(MidiEvent::NoteOn {
        channel: 0,
        note: 60,
        velocity: 91,
    });
    assert_eq!(engine.note_recipients_for_test(0, 60), 0b0111_1111);
    engine.handle_midi(MidiEvent::NoteOff {
        channel: 0,
        note: 60,
        velocity: 0,
    });
    assert_eq!(engine.note_recipients_for_test(0, 60), recipients);
    engine.handle_midi(MidiEvent::NoteOff {
        channel: 0,
        note: 60,
        velocity: 0,
    });
    assert_eq!(engine.note_recipients_for_test(0, 60), 0);
}

#[test]
fn split_gain_is_applied_without_reducing_midi_velocity() {
    let mut engine = Stage4Engine::new(48_000.0);
    engine.split_router.splits[0].enabled = true;
    engine.split_router.splits[0].midi_note = 60;
    engine.split_router.splits[0].crossfade = crate::morph::CrossfadeWidth::Small;
    engine.split_router.layer_zone_masks[LayerId::PianoA as usize] = 0b0001;
    engine.handle_midi(MidiEvent::NoteOn {
        channel: 0,
        note: 60,
        velocity: 91,
    });
    let (velocity_response, split_gain, _) = engine.piano_a.voice_excitation_for_test(60).unwrap();
    assert_eq!(velocity_response, 91.0 / 127.0);
    assert_eq!(split_gain, 0.5);
}

#[test]
fn unmapped_midi_channel_does_not_trigger_layers_or_global_controls() {
    let mut engine = Stage4Engine::new(48_000.0);
    engine.handle_midi(MidiEvent::NoteOn {
        channel: 1,
        note: 60,
        velocity: 100,
    });
    engine.handle_midi(MidiEvent::ControlChange {
        channel: 1,
        controller: cc::MOD_WHEEL,
        value: 127,
    });
    assert_eq!(engine.note_recipients_for_test(1, 60), 0);
    assert_eq!(engine.morph_controllers.mod_wheel, 0.0);
    engine.handle_midi(MidiEvent::NoteOn {
        channel: 16,
        note: 60,
        velocity: 100,
    });
    assert_eq!(engine.note_recipients_for_test(1, 60), 0);
}

#[test]
fn layer_channel_routes_directly_and_note_off_uses_its_channel() {
    let mut engine = Stage4Engine::new(48_000.0);
    engine.layer_midi_channels[LayerId::PianoA as usize] = Some(4);
    engine.split_router.layer_zone_masks[LayerId::PianoA as usize] = 0;

    engine.handle_midi(MidiEvent::NoteOn {
        channel: 4,
        note: 60,
        velocity: 100,
    });
    let (velocity_response, direct_gain, released) =
        engine.piano_a.voice_excitation_for_test(60).unwrap();
    assert_eq!(velocity_response, 100.0 / 127.0);
    assert_eq!(direct_gain, 1.0);
    assert!(!released);
    assert_eq!(
        engine.note_recipients_for_test(4, 60),
        1 << LayerId::PianoA as u8
    );
    engine.handle_midi(MidiEvent::ControlChange {
        channel: 4,
        controller: cc::MOD_WHEEL,
        value: 127,
    });
    assert_eq!(engine.morph_controllers.mod_wheel, 0.0);

    engine.handle_midi(MidiEvent::NoteOff {
        channel: 0,
        note: 60,
        velocity: 0,
    });
    assert_eq!(
        engine.note_recipients_for_test(4, 60),
        1 << LayerId::PianoA as u8
    );
    assert!(!engine.piano_a.voice_excitation_for_test(60).unwrap().2);

    engine.handle_midi(MidiEvent::NoteOff {
        channel: 4,
        note: 60,
        velocity: 0,
    });
    assert_eq!(engine.note_recipients_for_test(4, 60), 0);
    assert!(engine.piano_a.voice_excitation_for_test(60).unwrap().2);
}

#[test]
fn global_channel_is_configurable_and_can_be_disabled() {
    let mut engine = Stage4Engine::new(48_000.0);
    engine.global_midi_channel = Some(7);
    engine.handle_midi(MidiEvent::NoteOn {
        channel: 0,
        note: 60,
        velocity: 90,
    });
    assert_eq!(engine.note_recipients_for_test(0, 60), 0);

    engine.handle_midi(MidiEvent::NoteOn {
        channel: 7,
        note: 60,
        velocity: 90,
    });
    assert_ne!(engine.note_recipients_for_test(7, 60), 0);

    engine.global_midi_channel = None;
    engine.handle_midi(MidiEvent::NoteOn {
        channel: 7,
        note: 62,
        velocity: 90,
    });
    assert_eq!(engine.note_recipients_for_test(7, 62), 0);
}

#[test]
fn overlapping_channels_do_not_release_a_pitch_while_still_held() {
    let mut engine = Stage4Engine::new(48_000.0);
    engine.layer_midi_channels[LayerId::PianoA as usize] = Some(4);
    for channel in [0, 4] {
        engine.handle_midi(MidiEvent::NoteOn {
            channel,
            note: 60,
            velocity: 100,
        });
    }

    engine.handle_midi(MidiEvent::NoteOff {
        channel: 0,
        note: 60,
        velocity: 0,
    });
    assert!(engine
        .piano_a
        .voice_excitation_for_test(60)
        .is_some_and(|(_, _, released)| !released));

    engine.handle_midi(MidiEvent::NoteOff {
        channel: 4,
        note: 60,
        velocity: 0,
    });
    assert!(engine
        .piano_a
        .voice_excitation_for_test(60)
        .is_some_and(|(_, _, released)| released));
}
