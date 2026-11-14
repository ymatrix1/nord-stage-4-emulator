//! Preset Library loads on a synthetic backup with real encoded preset files.

use super::super::support::build_synthetic_zip_stored;
use super::*;
use crate::audio::apply_audio_command;
use crate::panel::bay_program::catalog::LoadRequest;
use crate::panel::bay_program::preset_user::UserPreset;
use nord_core::cbin::{nord_name_hash, OrganPresetDecoded, PianoPresetDecoded};
use nord_core::engine::Stage4Engine;
use nord_core::organ::OrganModel;
use std::path::PathBuf;

const ORGAN: &str = "Organ Preset/Bank 1/Vox Chord.ns4o";
const PIANO: &str = "Piano Preset/Bank 1/Big Grand.ns4n";
const NPNO: &str = "Piano/Grand/Astoria Grand Lrg 6.1.npno";

fn organ_file() -> Vec<u8> {
    let mut o = OrganPresetDecoded::default();
    o.model = OrganModel::Vox;
    o.drawbars_a = [1, 2, 3, 4, 5, 6, 7, 8, 0];
    o.layer_a_level = 0.5;
    SectionPreset::Organ(Box::new(o))
        .to_file(0, 0, 18)
        .unwrap()
        .to_bytes()
}

fn piano_file(known_sound: bool) -> Vec<u8> {
    let mut p = PianoPresetDecoded::default();
    p.layer_a.name_hash = if known_sound {
        nord_name_hash("Astoria Grand Lrg 6.1")
    } else {
        nord_name_hash("Missing Grand")
    };
    SectionPreset::Piano(Box::new(p))
        .to_file(0, 0, 21)
        .unwrap()
        .to_bytes()
}

/// An opened backup in a temp directory with one Organ and one Piano preset.
fn open(known_sound: bool) -> (BackupLibraryState, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "ns4-preset-load-{}-{known_sound}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("in.ns4b");
    let zip = build_synthetic_zip_stored(&[
        ("meta.xml", b"<backup product_id=\"46\"/>"),
        (ORGAN, &organ_file()),
        (PIANO, &piano_file(known_sound)),
        (NPNO, b"not-decoded-here"),
    ]);
    std::fs::write(&src, zip).unwrap();
    let mut state = BackupLibraryState::new();
    state.open_backup_path(&src).unwrap();
    (state, dir)
}

fn sent(rx: &crossbeam_channel::Receiver<AudioCommand>) -> Vec<AudioCommand> {
    rx.try_iter().collect()
}

#[test]
fn an_organ_preset_loads_into_the_engine_and_the_panel() {
    let (mut lib, _) = open(true);
    let (audio, rx) = AudioHandle::new();
    let mut panel = Stage4PanelState::default();
    lib.load_preset_entry(PresetSection::Organ, ORGAN, None, &mut panel, &audio);
    assert_eq!(panel.organ_model, OrganModel::Vox);
    assert_eq!(panel.organ_drawbars, [1, 2, 3, 4, 5, 6, 7, 8, 0]);
    assert_eq!(lib.status_message, "Preset: Vox Chord");
    let cmds = sent(&rx);
    assert_eq!(cmds.len(), 1);
    assert!(cmds[0].is_program_edit(), "a preset load edits the program");
    let mut e = Stage4Engine::new(48_000.0);
    let mut vol = 1.0;
    for c in cmds {
        apply_audio_command(c, &mut e, &mut vol);
    }
    assert_eq!(e.organ_a.model, OrganModel::Vox);
    assert_eq!(e.organ_a.drawbars, [1, 2, 3, 4, 5, 6, 7, 8, 0]);
}

#[test]
fn a_piano_preset_schedules_the_sound_it_names_and_reports_a_missing_one() {
    let (mut lib, _) = open(true);
    let (audio, _rx) = AudioHandle::new();
    let mut panel = Stage4PanelState::default();
    lib.load_preset_entry(PresetSection::Piano, PIANO, None, &mut panel, &audio);
    assert_eq!(lib.pending_jobs, 1, "the .npno decode was queued");
    assert!(lib.notices.is_empty());

    let (mut lib, _) = open(false);
    lib.load_preset_entry(PresetSection::Piano, PIANO, None, &mut panel, &audio);
    assert_eq!(lib.pending_jobs, 0);
    assert_eq!(lib.notices, [FwMessage::PianoNotFound]);
}

#[test]
fn a_single_layer_load_goes_to_the_chosen_layer() {
    let (mut lib, _) = open(true);
    let (audio, rx) = AudioHandle::new();
    let mut panel = Stage4PanelState::default();
    lib.load_preset_entry(
        PresetSection::Piano,
        PIANO,
        Some(LayerId::PianoB),
        &mut panel,
        &audio,
    );
    assert!(panel.piano_b_active);
    assert!(matches!(
        &sent(&rx)[0],
        AudioCommand::Program(ProgramEdit::LoadPreset {
            single: Some(LayerId::PianoB),
            ..
        })
    ));
}

#[test]
fn errors_are_reported_not_dropped() {
    let (mut lib, _) = open(true);
    let (audio, rx) = AudioHandle::new();
    let mut panel = Stage4PanelState::default();
    lib.load_preset_entry(PresetSection::Organ, "nope.ns4o", None, &mut panel, &audio);
    assert!(lib.error_message.as_deref().unwrap().contains("nope.ns4o"));
    // a Piano preset asked for as an Organ one
    lib.load_preset_entry(PresetSection::Organ, PIANO, None, &mut panel, &audio);
    assert!(lib
        .error_message
        .as_deref()
        .unwrap()
        .contains("not a Organ"));
    assert!(sent(&rx).is_empty());
    let mut closed = BackupLibraryState::new();
    closed.load_preset_entry(PresetSection::Organ, ORGAN, None, &mut panel, &audio);
    assert!(closed.error_message.is_some());
}

#[test]
fn a_stored_preset_loads_from_its_snapshot_on_top_of_its_base() {
    let (mut lib, _) = open(true);
    let (audio, rx) = AudioHandle::new();
    let mut panel = Stage4PanelState::default();
    let mut engine = Stage4Engine::new(48_000.0);
    engine.organ_a.drawbars = [0, 0, 8, 0, 0, 8, 0, 0, 8];
    engine.layer_levels[0].base = 0.9;
    let user = UserPreset {
        name: "Mine".into(),
        section: PresetSection::Organ,
        state: engine.capture_state(),
        base: Some(ORGAN.into()),
    };
    let presets = &mut panel.program_ui.library.presets;
    let path = presets.store_user(5, user).unwrap();
    lib.load_preset_entry(PresetSection::Organ, &path, None, &mut panel, &audio);
    assert_eq!(panel.organ_drawbars, [0, 0, 8, 0, 0, 8, 0, 0, 8]);
    assert_eq!(
        panel.organ_model,
        OrganModel::B3,
        "the model is panel state"
    );
    assert_eq!(sent(&rx).len(), 1);
}

#[test]
fn the_panel_request_queue_reaches_the_loader() {
    let (mut lib, _) = open(true);
    let (audio, rx) = AudioHandle::new();
    let mut panel = Stage4PanelState::default();
    panel.program_ui.requests.push(LoadRequest::Preset {
        section: PresetSection::Organ,
        entry: ORGAN.into(),
        single: None,
    });
    lib.sync_program_panel(&mut panel, &audio);
    assert!(panel.program_ui.requests.is_empty());
    assert_eq!(panel.organ_model, OrganModel::Vox);
    assert!(!sent(&rx).is_empty());
}
