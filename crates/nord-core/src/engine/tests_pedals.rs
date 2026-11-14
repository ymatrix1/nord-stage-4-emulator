//! Engine pedal, swell, pitch-stick and SUSTPED routing tests.

use super::pedals::*;
use super::*;
use crate::organ::OrganModel;

fn cc_global(e: &mut Stage4Engine, controller: u8, value: u8) {
    e.handle_midi(MidiEvent::ControlChange {
        channel: 0,
        controller,
        value,
    });
}

fn peak(frames: impl Iterator<Item = [f32; 2]>) -> f32 {
    frames.fold(0.0f32, |m, f| m.max(f[0].abs()).max(f[1].abs()))
}

#[test]
fn swell_cc4_scales_the_organ_output() {
    let render = |swell: u8, model: OrganModel| {
        let mut e = Stage4Engine::new(48_000.0);
        e.scene.scene_1_mask = 1 << LayerId::OrganA as u8;
        e.organ_a.model = model;
        e.organ_a.drawbars = [0, 0, 8, 8, 0, 0, 0, 0, 0];
        cc_global(&mut e, cc::ORGAN_SWELL, swell);
        e.handle_midi(MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100,
        });
        peak((0..8192).map(|_| e.render_frame()).skip(2048))
    };
    let full = render(127, OrganModel::B3);
    let low = render(0, OrganModel::B3);
    assert!(full > 1e-4, "organ is silent");
    assert!(low > 0.0 && low < 0.5 * full, "B3 floor {low} vs {full}");
    let f = render(0, OrganModel::Farfisa);
    assert!(f < 1e-6, "{f}");
}

#[test]
fn swell_scales_only_the_organ_frames_exactly() {
    let mut e = Stage4Engine::new(48_000.0);
    e.pedals.swell = 0.0;
    let mut raw = [[1.0f32, -1.0]; 7];
    e.apply_organ_swell(&mut raw);
    assert_eq!(raw[0], [B3_SWELL_FLOOR, -B3_SWELL_FLOOR]);
    assert_eq!(raw[1], raw[0]);
    assert_eq!(raw[2], [1.0, -1.0]);
    assert_eq!(raw[6], [1.0, -1.0]);
}

#[test]
fn swell_function_volume_and_wheel_morph_and_ctrl_swell() {
    let mut e = Stage4Engine::new(48_000.0);
    e.pedals.swell_func = SwellFunc::Volume;
    cc_global(&mut e, cc::ORGAN_SWELL, 0);
    assert_eq!(e.pedals.volume_gain(), 0.0);
    assert_eq!(e.pedals.organ_swell_gain(OrganModel::B3), 1.0);
    e.pedals.swell_func = SwellFunc::WheelMorph;
    cc_global(&mut e, cc::ORGAN_SWELL, 127);
    assert_eq!(e.morph_controllers.mod_wheel, 1.0);
    e.pedals.control_func = ControlFunc::CtrlSwell;
    cc_global(&mut e, cc::CONTROL_PEDAL, 0);
    assert_eq!(e.pedals.swell, 0.0);
    assert_eq!(e.morph_controllers.control_pedal, 0.0);
    e.pedals.control_gain = 2.0;
    cc_global(&mut e, cc::CONTROL_PEDAL, 40);
    assert!((e.morph_controllers.control_pedal - 80.0 / 127.0).abs() < 1e-6);
}

#[test]
fn sustain_defers_synth_release_only_with_sustped() {
    let mut e = Stage4Engine::new(48_000.0);
    let idx = LayerId::SynthA as usize;
    cc_global(&mut e, cc::SUSTAIN_PEDAL, 127);
    e.handle_midi(MidiEvent::NoteOn {
        channel: 0,
        note: 60,
        velocity: 100,
    });
    e.handle_midi(MidiEvent::NoteOff {
        channel: 0,
        note: 60,
        velocity: 0,
    });
    assert_eq!(
        e.pedals.held[idx], 0,
        "SUSTPED off: key release is immediate"
    );
    e.pedals.sustped[idx] = true;
    e.handle_midi(MidiEvent::NoteOn {
        channel: 0,
        note: 62,
        velocity: 100,
    });
    e.handle_midi(MidiEvent::NoteOff {
        channel: 0,
        note: 62,
        velocity: 0,
    });
    e.refresh_pedal_routing();
    assert_eq!(e.pedals.held[idx], 1u128 << 62);
    cc_global(&mut e, cc::SUSTAIN_PEDAL, 0);
    assert_eq!(e.pedals.held[idx], 0, "pedal up releases the held key");
}

#[test]
fn restriking_a_held_note_cancels_its_deferred_release() {
    let mut e = Stage4Engine::new(48_000.0);
    let idx = LayerId::OrganA as usize;
    e.pedals.sustped[idx] = true;
    e.refresh_pedal_routing();
    cc_global(&mut e, cc::SUSTAIN_PEDAL, 127);
    for on in [true, false, true] {
        let ev = if on {
            MidiEvent::NoteOn {
                channel: 0,
                note: 64,
                velocity: 90,
            }
        } else {
            MidiEvent::NoteOff {
                channel: 0,
                note: 64,
                velocity: 0,
            }
        };
        e.handle_midi(ev);
    }
    assert_eq!(e.pedals.held[idx], 0);
}

#[test]
fn half_pedal_band_slows_piano_release() {
    let mut e = Stage4Engine::new(48_000.0);
    cc_global(&mut e, cc::SUSTAIN_PEDAL, 40);
    assert!(e.piano_a.damper_half && !e.piano_a.damper_pedal_down);
    cc_global(&mut e, cc::SUSTAIN_PEDAL, 10);
    assert!(!e.piano_a.damper_half, "below the band the dampers rest");
    cc_global(&mut e, cc::SUSTAIN_PEDAL, 127);
    assert!(e.piano_a.damper_pedal_down && !e.piano_a.damper_half);
}

#[test]
fn pitch_stick_bends_synth_by_range_and_aux_always() {
    let mut e = Stage4Engine::new(48_000.0);
    let bend = |e: &mut Stage4Engine, ch: u8, v: u16| {
        e.handle_midi(MidiEvent::PitchBend {
            channel: ch,
            value: v,
        })
    };
    bend(&mut e, 0, 16383);
    assert_eq!(e.synth_a.bend_cents, 0.0, "PSTICK off");
    e.pedals.pstick[LayerId::SynthA as usize] = true;
    e.refresh_pedal_routing();
    assert!((e.synth_a.bend_cents - 200.0).abs() < 0.1);
    e.pedals.pstick_range[LayerId::SynthA as usize] = 12;
    bend(&mut e, 0, 0);
    assert!((e.synth_a.bend_cents + 1200.0).abs() < 1e-3);
    bend(&mut e, 0, 8192);
    assert_eq!(e.synth_a.bend_cents, 0.0);
    e.aux_kb.enabled = true;
    e.aux_kb.layers[LayerId::SynthB as usize] = true;
    bend(&mut e, 15, 16383);
    assert!(e.synth_b.bend_cents > 190.0, "aux layer follows aux bend");
    assert_eq!(e.synth_c.bend_cents, 0.0);
}

#[test]
fn pitch_bend_changes_the_rendered_synth_signal() {
    let run = |bend: bool| {
        let mut e = Stage4Engine::new(48_000.0);
        e.pedals.pstick[LayerId::SynthA as usize] = true;
        e.handle_midi(MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100,
        });
        if bend {
            e.handle_midi(MidiEvent::PitchBend {
                channel: 0,
                value: 16383,
            });
        }
        (0..4096).map(|_| e.render_frame()).collect::<Vec<_>>()
    };
    let (flat, bent) = (run(false), run(true));
    assert!(flat.iter().any(|f| f[0].abs() > 1e-4));
    assert!(flat != bent);
    assert!(bent.iter().all(|f| f[0].is_finite()));
}

#[test]
fn aux_sustain_holds_aux_layers_without_sustped() {
    let mut e = Stage4Engine::new(48_000.0);
    e.aux_kb.enabled = true;
    e.aux_kb.layers[LayerId::OrganB as usize] = true;
    e.scene.scene_1_mask |= 1 << LayerId::OrganB as u8;
    e.handle_midi(MidiEvent::ControlChange {
        channel: 15,
        controller: cc::SUSTAIN_PEDAL,
        value: 127,
    });
    e.handle_midi(MidiEvent::NoteOn {
        channel: 15,
        note: 55,
        velocity: 90,
    });
    e.handle_midi(MidiEvent::NoteOff {
        channel: 15,
        note: 55,
        velocity: 0,
    });
    assert_eq!(e.pedals.held[LayerId::OrganB as usize], 1u128 << 55);
}

#[test]
fn rotor_pedal_and_foot_switch_functions() {
    let mut e = Stage4Engine::new(48_000.0);
    e.organ_a.rotary_speed = RotarySpeed::Slow;
    e.rotor_pedal(true);
    e.rotor_pedal(false);
    assert_eq!(e.organ_a.rotary_speed, RotarySpeed::Fast, "toggle latches");
    e.rotor_pedal(true);
    assert_eq!(e.organ_b.rotary_speed, RotarySpeed::Slow);
    e.rotor_pedal(false);
    e.pedals.rotor_func = RotorFunc::Hold;
    e.rotor_pedal(true);
    assert_eq!(e.organ_a.rotary_speed, RotarySpeed::Fast);
    e.rotor_pedal(false);
    assert_eq!(e.organ_a.rotary_speed, RotarySpeed::Slow);
    e.pedals.foot_switch = FootSwitchFunc::LayerSceneToggle;
    e.foot_switch(true);
    e.foot_switch(false);
    assert!(e.scene.scene_2_active);
    e.pedals.foot_switch = FootSwitchFunc::LayerSceneHold;
    e.foot_switch(true);
    e.foot_switch(false);
    assert!(!e.scene.scene_2_active);
    e.pedals.right_pedal = RightPedal::SusRotorToggle;
    cc_global(&mut e, cc::SUSTAIN_PEDAL, 127);
    assert_eq!(e.organ_a.rotary_speed, RotarySpeed::Fast);
}
