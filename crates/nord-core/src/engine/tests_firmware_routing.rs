//! `FirmwareRouting`: the engine follows the decoded section / layer enables and KB zones.

use super::*;
use crate::cbin::ProgramDecoded;

const ORGAN_A: u8 = 1 << LayerId::OrganA as u8;
const ORGAN_B: u8 = 1 << LayerId::OrganB as u8;

/// `B3 Chords n Solo` as decoded: organ A + B on in scene I, scene II empty, Mid split at 65,
/// layer A in the lower zone (code 0), layer B in the upper zone (code 6).
fn b3_chords_n_solo() -> ProgramDecoded {
    let mut prog = ProgramDecoded::default();
    prog.global.scene_1_mask = ORGAN_A | ORGAN_B;
    prog.global.scene_2_mask = 0;
    prog.global.split_enabled = true;
    prog.global.split_zone_enabled = [false, true, false];
    prog.global.split_points = [48, 65, 84];
    prog.global.kb_zone = [0, 6, 5, 5, 5, 5, 5];
    prog
}

fn press(e: &mut Stage4Engine, note: u8) -> u8 {
    e.handle_midi(MidiEvent::NoteOn {
        channel: 0,
        note,
        velocity: 100,
    });
    e.note_recipients_for_test(0, note)
}

#[test]
fn off_keeps_the_legacy_routing() {
    let mut e = Stage4Engine::new(48_000.0);
    e.apply_program(&b3_chords_n_solo());
    // legacy: both layers sound everywhere (the zone codes are not read)
    assert_eq!(press(&mut e, 44), ORGAN_A | ORGAN_B);
    assert_eq!(e.firmware_routing(), FirmwareRouting::Off);
}

#[test]
fn on_sends_each_key_only_to_the_layer_of_its_zone() {
    let mut e = Stage4Engine::new(48_000.0);
    e.apply_program(&b3_chords_n_solo());
    e.set_firmware_routing(FirmwareRouting::On);
    assert_eq!(press(&mut e, 44), ORGAN_A);
    assert_eq!(press(&mut e, 64), ORGAN_A);
    assert_eq!(press(&mut e, 65), ORGAN_B);
    assert_eq!(press(&mut e, 90), ORGAN_B);
}

#[test]
fn routing_set_before_the_program_gives_the_same_result() {
    let mut e = Stage4Engine::new(48_000.0);
    e.set_firmware_routing(FirmwareRouting::On);
    e.apply_program(&b3_chords_n_solo());
    assert_eq!(press(&mut e, 50), ORGAN_A);
    assert_eq!(press(&mut e, 70), ORGAN_B);
}

#[test]
fn empty_scene_mask_is_silent_with_firmware_routing_only() {
    let mut prog = ProgramDecoded::default();
    prog.global.scene_1_mask = 0;
    let mut e = Stage4Engine::new(48_000.0);
    e.apply_program(&prog);
    assert_ne!(press(&mut e, 60), 0, "legacy falls back to default layers");
    e.set_firmware_routing(FirmwareRouting::On);
    e.handle_midi(MidiEvent::NoteOff {
        channel: 0,
        note: 60,
        velocity: 0,
    });
    assert_eq!(press(&mut e, 61), 0);
}

#[test]
fn scene_two_follows_its_own_exact_mask() {
    let mut e = Stage4Engine::new(48_000.0);
    e.apply_program(&b3_chords_n_solo());
    e.set_firmware_routing(FirmwareRouting::On);
    e.scene.scene_2_active = true;
    assert_eq!(press(&mut e, 44), 0, "section II is off in the program");
}

#[test]
fn switching_back_restores_the_legacy_masks() {
    let mut e = Stage4Engine::new(48_000.0);
    let mut prog = b3_chords_n_solo();
    prog.global.scene_2_mask = 0;
    e.apply_program(&prog);
    let legacy = (e.scene.scene_1_mask, e.scene.scene_2_mask);
    e.set_firmware_routing(FirmwareRouting::On);
    assert_eq!(
        (e.scene.scene_1_mask, e.scene.scene_2_mask),
        (ORGAN_A | ORGAN_B, 0)
    );
    e.set_firmware_routing(FirmwareRouting::Off);
    assert_eq!((e.scene.scene_1_mask, e.scene.scene_2_mask), legacy);
}

#[test]
fn direct_layer_channel_bypasses_the_zone() {
    let mut e = Stage4Engine::new(48_000.0);
    e.apply_program(&b3_chords_n_solo());
    e.set_firmware_routing(FirmwareRouting::On);
    e.set_layer_channel(LayerId::OrganB, Some(3));
    e.handle_midi(MidiEvent::NoteOn {
        channel: 2,
        note: 40,
        velocity: 100,
    });
    assert_eq!(e.note_recipients_for_test(2, 40), ORGAN_B);
}
