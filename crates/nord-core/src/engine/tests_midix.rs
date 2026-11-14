//! Engine tests of the MIDI menu settings: Local Control, Sect On, Ctrl / Dev / PC filters,
//! Transpose MIDI At and the keyboard transmit path (`midi_menu.rs`).

use super::*;

fn cc_in(e: &mut Stage4Engine, channel: u8, controller: u8, value: u8) {
    e.handle_midi(MidiEvent::ControlChange {
        channel,
        controller,
        value,
    });
}

fn drain(e: &mut Stage4Engine) -> Vec<MidiOut> {
    std::iter::from_fn(|| e.pop_midi_out()).collect()
}

fn note_on(note: u8) -> MidiEvent {
    MidiEvent::NoteOn {
        channel: 0,
        note,
        velocity: 100,
    }
}

fn note_off(note: u8) -> MidiEvent {
    MidiEvent::NoteOff {
        channel: 0,
        note,
        velocity: 0,
    }
}

fn peak(e: &mut Stage4Engine, frames: usize) -> f32 {
    (0..frames).fold(0.0f32, |m, _| {
        let [l, r] = e.render_frame();
        m.max(l.abs()).max(r.abs())
    })
}

#[test]
fn defaults_are_inert() {
    let e = Stage4Engine::new(48_000.0);
    assert_eq!(e.midi_menu, MidiMenuSettings::default());
    assert_eq!(e.extern_menu, ExternMenu::default());
    assert_eq!(e.output_routing, OutputRouting::default());
    assert_eq!(e.organ_trigger.point, TriggerPoint::High);
    assert!(e.midi_menu.local_control);
}

#[test]
fn local_control_off_transmits_the_keyboard_but_keeps_it_silent() {
    let mut e = Stage4Engine::new(48_000.0);
    e.midi_menu.local_control = false;
    e.handle_keyboard_midi(note_on(60));
    assert_eq!(peak(&mut e, 2048), 0.0, "keyboard is disconnected");
    let out = drain(&mut e);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].bytes(), [0x90, 60, 100]);
    // MIDI In still plays the instrument.
    e.handle_midi(note_on(62));
    assert!(peak(&mut e, 2048) > 1e-4);
    // With Local On the keyboard sounds and is transmitted too.
    let mut on = Stage4Engine::new(48_000.0);
    on.handle_keyboard_midi(note_on(60));
    assert!(peak(&mut on, 2048) > 1e-4);
    assert_eq!(drain(&mut on).len(), 1);
    on.handle_keyboard_midi(note_off(60));
    assert_eq!(drain(&mut on)[0].bytes(), [0x80, 60, 0]);
}

#[test]
fn keyboard_transmit_follows_the_global_channel_and_dev_ctrl_send() {
    let mut e = Stage4Engine::new(48_000.0);
    e.set_global_channel(Some(5));
    e.handle_keyboard_midi(note_on(60));
    assert_eq!(drain(&mut e)[0].status, 0x94);
    e.midi_menu.dev = MidiMode::Receive;
    e.midi_menu.ctrl = MidiMode::Off;
    e.handle_keyboard_midi(MidiEvent::ControlChange {
        channel: 0,
        controller: 1,
        value: 9,
    });
    e.handle_keyboard_midi(MidiEvent::PitchBend {
        channel: 0,
        value: 9000,
    });
    e.handle_keyboard_midi(MidiEvent::ControlChange {
        channel: 0,
        controller: 82,
        value: 9,
    });
    assert!(drain(&mut e).is_empty(), "Dev and Ctrl send are off");
    e.midi_menu.dev = MidiMode::SendReceive;
    e.midi_menu.ctrl = MidiMode::Send;
    e.handle_keyboard_midi(MidiEvent::ControlChange {
        channel: 0,
        controller: 1,
        value: 9,
    });
    e.handle_keyboard_midi(MidiEvent::ControlChange {
        channel: 0,
        controller: 82,
        value: 9,
    });
    e.handle_keyboard_midi(MidiEvent::ChannelAftertouch {
        channel: 0,
        pressure: 20,
    });
    e.handle_keyboard_midi(MidiEvent::PitchBend {
        channel: 0,
        value: 9000,
    });
    let bytes: Vec<_> = drain(&mut e).iter().map(|m| (m.bytes(), m.len)).collect();
    assert_eq!(bytes[0], ([0xB4, 1, 9], 3));
    assert_eq!(bytes[1], ([0xB4, 82, 9], 3));
    assert_eq!(bytes[2], ([0xD4, 20, 0], 2));
    assert_eq!(bytes[3].0[0], 0xE4);
}

#[test]
fn ctrl_and_dev_receive_filters_split_the_controllers() {
    let mut e = Stage4Engine::new(48_000.0);
    e.midi_menu.ctrl = MidiMode::Send;
    e.organ_a.live_drawbars[0] = 3;
    cc_in(&mut e, 0, cc::DRAWBAR_1_16, 127);
    assert_eq!(e.organ_a.live_drawbars[0], 3, "Ctrl is not received");
    cc_in(&mut e, 0, cc::MOD_WHEEL, 127);
    assert_eq!(e.morph_controllers.mod_wheel, 1.0, "Dev still received");

    e.midi_menu.dev = MidiMode::Send;
    cc_in(&mut e, 0, cc::MOD_WHEEL, 0);
    assert_eq!(e.morph_controllers.mod_wheel, 1.0, "Dev is not received");
    e.handle_midi(MidiEvent::ChannelAftertouch {
        channel: 0,
        pressure: 100,
    });
    assert_eq!(e.morph_controllers.aftertouch, 0.0);
    e.pedals.pstick[LayerId::SynthA as usize] = true;
    e.handle_midi(MidiEvent::PitchBend {
        channel: 0,
        value: 16383,
    });
    assert_eq!(e.synth_a.bend_cents, 0.0);
    cc_in(&mut e, 0, cc::SUSTAIN_PEDAL, 127);
    assert_eq!(e.pedals.sustain[0], 0);

    e.midi_menu.dev = MidiMode::Receive;
    e.midi_menu.ctrl = MidiMode::Receive;
    cc_in(&mut e, 0, cc::MOD_WHEEL, 0);
    assert_eq!(e.morph_controllers.mod_wheel, 0.0);
    cc_in(&mut e, 0, cc::DRAWBAR_1_16, 127);
    assert_eq!(e.organ_a.live_drawbars[0], 8);
}

#[test]
fn aux_channel_is_not_filtered_by_dev() {
    let mut e = Stage4Engine::new(48_000.0);
    e.midi_menu.dev = MidiMode::Off;
    e.aux_kb.enabled = true;
    e.aux_kb.layers[LayerId::SynthA as usize] = true;
    e.pedals.sustped[LayerId::SynthA as usize] = false;
    cc_in(&mut e, 15, cc::SUSTAIN_PEDAL, 127);
    assert_eq!(e.pedals.sustain[PedalSource::Aux as usize], 127);
}

#[test]
fn nrpn_registers_need_cc_and_nrpn_type_and_ctrl_receive() {
    let mut e = Stage4Engine::new(48_000.0);
    cc_in(&mut e, 0, 99, 1);
    cc_in(&mut e, 0, 98, 2);
    cc_in(&mut e, 0, 6, 3);
    assert_eq!(e.take_nrpn(), Some(((1 << 7) | 2, 3 << 7)));
    cc_in(&mut e, 0, 38, 5);
    assert_eq!(e.take_nrpn(), Some(((1 << 7) | 2, (3 << 7) | 5)));
    assert_eq!(e.take_nrpn(), None, "reading clears it");
    e.midi_menu.nrpn = false;
    for (c, v) in [(99, 1), (98, 2), (6, 3), (38, 4)] {
        cc_in(&mut e, 0, c, v);
    }
    assert_eq!(e.take_nrpn(), None, "Type CC ignores NRPN");
    e.midi_menu.nrpn = true;
    e.midi_menu.ctrl = MidiMode::Send;
    cc_in(&mut e, 0, 6, 3);
    assert_eq!(e.take_nrpn(), None, "Ctrl must receive");
}

#[test]
fn program_change_uses_pc_for_program_banks_and_pc_ext_for_presets() {
    let mut e = Stage4Engine::new(48_000.0);
    e.handle_midi(MidiEvent::ProgramChange {
        channel: 0,
        program: 12,
    });
    assert_eq!(
        e.take_program_change(),
        Some(ProgramRequest {
            bank_msb: 0,
            bank_lsb: 0,
            program: 12
        })
    );
    assert_eq!(e.take_program_change(), None);
    cc_in(&mut e, 0, cc::BANK_SELECT_MSB, 3);
    cc_in(&mut e, 0, cc::BANK_SELECT_LSB, 1);
    e.handle_midi(MidiEvent::ProgramChange {
        channel: 0,
        program: 7,
    });
    assert_eq!(e.take_program_change().unwrap().bank_msb, 3);
    e.midi_menu.pc_ext = MidiMode::Send;
    e.handle_midi(MidiEvent::ProgramChange {
        channel: 0,
        program: 8,
    });
    assert_eq!(e.take_program_change(), None, "preset banks need PC Ext");
    cc_in(&mut e, 0, cc::BANK_SELECT_MSB, 1);
    e.midi_menu.pc = MidiMode::Send;
    e.handle_midi(MidiEvent::ProgramChange {
        channel: 0,
        program: 9,
    });
    assert_eq!(e.take_program_change(), None, "Live bank needs PC");
    e.handle_midi(MidiEvent::ProgramChange {
        channel: 4,
        program: 9,
    });
    e.midi_menu.pc = MidiMode::Receive;
    assert_eq!(e.take_program_change(), None, "only the Global channel");
}

#[test]
fn program_change_send_emits_bank_select_and_pc_on_the_global_channel() {
    let mut e = Stage4Engine::new(48_000.0);
    assert!(e.send_program_change(0, 2, 17));
    let bytes: Vec<_> = drain(&mut e).iter().map(|m| m.bytes()).collect();
    assert_eq!(bytes, vec![[0xB0, 0, 0], [0xB0, 32, 2], [0xC0, 17, 0]]);
    e.midi_menu.pc = MidiMode::Receive;
    assert!(!e.send_program_change(1, 0, 3), "PC does not send");
    assert!(e.send_program_change(4, 0, 3), "PC Ext still sends presets");
    drain(&mut e);
    e.midi_menu.pc_ext = MidiMode::Off;
    assert!(!e.send_program_change(2, 0, 3));
    e.set_global_channel(None);
    e.midi_menu.pc = MidiMode::Send;
    assert!(!e.send_program_change(0, 0, 3), "global MIDI is off");
    assert!(drain(&mut e).is_empty());
}

#[test]
fn transpose_midi_at_in_transposes_incoming_and_out_transposes_the_keyboard_output() {
    let incoming_note = |at| {
        let mut e = Stage4Engine::new(48_000.0);
        e.midi_menu.transpose_at = at;
        e.set_transpose(3);
        e.handle_midi(note_on(60));
        let s = e.piano_a.voice_excitation_for_test(63).is_some();
        let u = e.piano_a.voice_excitation_for_test(60).is_some();
        (s, u)
    };
    assert_eq!(incoming_note(TransposeAt::In), (true, false));
    assert_eq!(incoming_note(TransposeAt::Out), (false, true));

    let mut e = Stage4Engine::new(48_000.0);
    e.set_transpose(3);
    e.handle_keyboard_midi(note_on(60));
    assert_eq!(drain(&mut e)[0].data1, 60, "In: output untransposed");
    assert!(
        e.piano_a.voice_excitation_for_test(63).is_some(),
        "keyboard sounds transposed"
    );
    e.midi_menu.transpose_at = TransposeAt::Out;
    e.handle_keyboard_midi(note_on(61));
    assert_eq!(drain(&mut e)[0].data1, 64, "Out: output transposed");
    assert!(
        e.piano_a.voice_excitation_for_test(64).is_some(),
        "keyboard still transposed"
    );
    e.set_transpose(0);
    e.handle_keyboard_midi(note_off(61));
    assert_eq!(drain(&mut e)[0].data1, 64, "note off matches the sent note");
}

#[test]
fn aux_notes_are_transposed_even_with_transpose_at_out() {
    let mut e = Stage4Engine::new(48_000.0);
    e.midi_menu.transpose_at = TransposeAt::Out;
    e.aux_kb.enabled = true;
    e.aux_kb.layers[LayerId::PianoA as usize] = true;
    e.scene.scene_1_mask = 1 << LayerId::PianoA as u8;
    e.set_transpose(2);
    e.handle_midi(MidiEvent::NoteOn {
        channel: 15,
        note: 60,
        velocity: 100,
    });
    assert!(e.piano_a.voice_excitation_for_test(62).is_some());
}

#[test]
fn sect_on_routes_the_whole_section_through_layer_a_channel() {
    let mut e = Stage4Engine::new(48_000.0);
    e.scene.scene_1_mask = 0b0111_1111;
    e.set_global_channel(None);
    e.set_layer_channel(LayerId::SynthA, Some(3));
    e.set_layer_channel(LayerId::SynthB, Some(4));
    e.midi_menu.sect_on[2] = true;
    e.handle_midi(MidiEvent::NoteOn {
        channel: 2,
        note: 60,
        velocity: 100,
    });
    let mask = e.note_recipients_for_test(2, 60);
    let synths =
        (1 << LayerId::SynthA as u8) | (1 << LayerId::SynthB as u8) | (1 << LayerId::SynthC as u8);
    assert_eq!(
        mask & synths,
        synths,
        "A, B and C all answer Layer A's channel"
    );
    e.handle_midi(MidiEvent::NoteOn {
        channel: 3,
        note: 61,
        velocity: 100,
    });
    assert_eq!(
        e.note_recipients_for_test(3, 61),
        0,
        "B's own channel is ignored"
    );
}

#[test]
fn organ_and_piano_layers_bend_with_the_pitch_stick() {
    let mut e = Stage4Engine::new(48_000.0);
    for l in [
        LayerId::OrganA,
        LayerId::OrganB,
        LayerId::PianoA,
        LayerId::PianoB,
    ] {
        e.pedals.pstick[l as usize] = true;
    }
    e.handle_midi(MidiEvent::PitchBend {
        channel: 0,
        value: 16383,
    });
    for cents in [
        e.organ_a.bend_cents(),
        e.organ_b.bend_cents(),
        e.piano_a.bend_cents(),
        e.piano_b.bend_cents(),
    ] {
        assert!((cents - 200.0).abs() < 0.1, "{cents}");
    }
    e.pedals.pstick_range[LayerId::PianoA as usize] = 12;
    e.handle_midi(MidiEvent::PitchBend {
        channel: 0,
        value: 0,
    });
    assert!((e.piano_a.bend_cents() + 1200.0).abs() < 1e-3);
    assert!((e.organ_a.bend_cents() + 200.0).abs() < 0.1);
    // PSTICK off (the default) leaves them alone.
    let mut off = Stage4Engine::new(48_000.0);
    off.handle_midi(MidiEvent::PitchBend {
        channel: 0,
        value: 16383,
    });
    assert_eq!(
        (off.organ_a.bend_cents(), off.piano_a.bend_cents()),
        (0.0, 0.0)
    );
}

#[test]
fn a_bent_organ_renders_a_different_signal_than_a_flat_one() {
    let run = |bend: bool| {
        let mut e = Stage4Engine::new(48_000.0);
        e.scene.scene_1_mask = 1 << LayerId::OrganA as u8;
        e.organ_a.drawbars = [0, 0, 8, 8, 0, 0, 0, 0, 0];
        e.pedals.pstick[LayerId::OrganA as usize] = true;
        e.handle_midi(note_on(60));
        if bend {
            e.handle_midi(MidiEvent::PitchBend {
                channel: 0,
                value: 16383,
            });
        }
        (0..4096).map(|_| e.render_frame()).collect::<Vec<_>>()
    };
    assert_ne!(run(true), run(false));
}
