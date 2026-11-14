//! Export backup / Live import on a synthetic archive laid out like the Factory Restore backup.

use super::super::support::build_synthetic_zip_stored;
use super::*;
use crate::panel::bay_program::store::{ProgLoc, UserProgram};
use nord_core::cbin::CbinKind;
use nord_core::engine::Stage4Engine;
use nord_core::library_ops::{OrganizeOp, OrganizeUndo};

fn prog(bank: u16, loc: u16, tempo: f32) -> Vec<u8> {
    let mut p = ProgramDecoded::default();
    p.global.tempo_bpm = tempo;
    p.to_cbin_file(bank, loc).unwrap().to_bytes()
}

fn live(slot: u16, tempo: f32) -> Vec<u8> {
    let mut p = ProgramDecoded::default();
    p.global.tempo_bpm = tempo;
    p.to_live_cbin_file(slot).unwrap().to_bytes()
}

/// Writes the synthetic backup to a fresh temp dir and opens it in a library state.
fn open() -> (BackupLibraryState, PathBuf, UserLibrary) {
    let dir = std::env::temp_dir().join(format!(
        "ns4-export-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("in.ns4b");
    let zip = build_synthetic_zip_stored(&[
        ("meta.xml", b"<backup product_id=\"46\"/>"),
        ("Program/Bank A/Alpha.ns4p", &prog(0, 0, 100.0_f32)),
        ("Program/Bank A/Beta.ns4p", &prog(0, 1, 110.0_f32)),
        ("Program/Bank B/Gamma.ns4p", &prog(1, 0, 120.0_f32)),
        ("Live/Bank 1/Live 0.ns4l", &live(0, 90.0_f32)),
        ("Live/Bank 1/Live 3.ns4l", &live(3, 93.0_f32)),
        ("Piano/Grand/Big.npno", b"piano-bytes"),
    ]);
    std::fs::write(&src, zip).unwrap();
    let mut state = BackupLibraryState::new();
    state.open_backup_path(&src).unwrap();
    let mut lib = UserLibrary::default();
    lib.set_factory(&state.program_slots);
    (state, dir, lib)
}

/// Entry name -> bytes of the archive at `path`.
fn entries(path: &Path) -> Vec<(String, Vec<u8>)> {
    let a = Ns4BackupArchive::open(path).unwrap();
    a.entries()
        .iter()
        .map(|e| {
            (
                e.name.clone(),
                a.read_entry_from_file(path, &e.name).unwrap(),
            )
        })
        .collect()
}

fn get<'a>(e: &'a [(String, Vec<u8>)], name: &str) -> &'a [u8] {
    &e.iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("{name}"))
        .1
}

fn tempo_of(bytes: &[u8]) -> f32 {
    ProgramDecoded::from_cbin_file(&CbinFile::parse(bytes).unwrap())
        .unwrap()
        .global
        .tempo_bpm
}

fn loc(s: &str) -> ProgLoc {
    ProgLoc::parse(s).unwrap()
}

#[test]
fn the_archive_layout_comes_from_the_entry_headers() {
    let (state, _, lib) = open();
    let mut slots = state.program_slots.clone();
    slots.sort();
    assert_eq!(slots.len(), 3);
    assert_eq!(slots[0], (0, "Program/Bank A/Alpha.ns4p".to_string()));
    assert_eq!(
        lib.factory_at(loc("B:11")),
        Some("Program/Bank B/Gamma.ns4p")
    );
}

#[test]
fn an_untouched_library_exports_a_byte_identical_copy() {
    let (mut state, dir, lib) = open();
    let msg = state.export_backup(&lib).unwrap();
    assert!(msg.contains("0 replaced"), "{msg}");
    let dst = default_export_path(&dir.join("in.ns4b"));
    assert!(dst.ends_with("in-edited.ns4b"));
    assert_eq!(
        std::fs::read(dir.join("in.ns4b")).unwrap(),
        std::fs::read(dst).unwrap()
    );
}

#[test]
fn swap_move_and_clear_rewrite_only_the_moved_entries() {
    let (mut state, dir, mut lib) = open();
    let mut undo = OrganizeUndo::default();
    // Swap Alpha <-> Beta, move Gamma (B:11) into the empty A:14.
    assert!(lib.organize(&mut undo, OrganizeOp::Swap, loc("A:11"), loc("A:12")));
    assert!(lib.organize(&mut undo, OrganizeOp::Swap, loc("B:11"), loc("A:14")));
    state.export_path = dir.join("out.ns4b").display().to_string();
    let msg = state.export_backup(&lib).unwrap();
    assert!(
        msg.contains("1 added") && msg.contains("1 removed"),
        "{msg}"
    );

    let (before, after) = (
        entries(&dir.join("in.ns4b")),
        entries(&dir.join("out.ns4b")),
    );
    for n in [
        "meta.xml",
        "Piano/Grand/Big.npno",
        "Live/Bank 1/Live 0.ns4l",
        "Live/Bank 1/Live 3.ns4l",
    ] {
        assert_eq!(
            get(&before, n),
            get(&after, n),
            "{n} must stay byte-identical"
        );
    }
    let header = |e: &[(String, Vec<u8>)], n: &str| CbinFile::parse(get(e, n)).unwrap().header;
    assert_eq!(
        header(&after, "Program/Bank A/Alpha.ns4p").location_index,
        1
    );
    assert_eq!(header(&after, "Program/Bank A/Beta.ns4p").location_index, 0);
    let g = header(&after, "Program/Bank A/Gamma.ns4p");
    assert_eq!((g.bank_index, g.location_index), (0, 3));
    assert!(after.iter().all(|(n, _)| n != "Program/Bank B/Gamma.ns4p"));
    // The payloads did not change, only the header locations.
    assert_eq!(
        tempo_of(get(&after, "Program/Bank A/Gamma.ns4p")),
        120.0_f32
    );
    assert_eq!(
        tempo_of(get(&after, "Program/Bank A/Alpha.ns4p")),
        100.0_f32
    );
}

#[test]
fn stored_programs_and_live_slots_are_patched_onto_the_archive_base() {
    let (mut state, dir, mut lib) = open();
    let mut engine = Stage4Engine::new(48_000.0_f32);
    engine.master_clock.bpm = 141.0_f32;
    lib.put(
        loc("A:13"),
        UserProgram {
            name: "Mine".into(),
            category: 0,
            state: engine.capture_state(),
        },
    );
    engine.master_clock.bpm = 77.0_f32;
    lib.put_live(3, engine.capture_state());
    lib.put_live(5, engine.capture_state());
    state.export_path = dir.join("out.ns4b").display().to_string();
    state.export_backup(&lib).unwrap();
    let (before, after) = (
        entries(&dir.join("in.ns4b")),
        entries(&dir.join("out.ns4b")),
    );
    assert_eq!(tempo_of(get(&after, "Program/Bank A/Mine.ns4p")), 141.0_f32);
    let mine = CbinFile::parse(get(&after, "Program/Bank A/Mine.ns4p")).unwrap();
    assert_eq!((mine.header.bank_index, mine.header.location_index), (0, 2));
    assert_eq!(tempo_of(get(&after, "Live/Bank 1/Live 3.ns4l")), 77.0_f32);
    let new_live = CbinFile::parse(get(&after, "Live/Bank 1/Live 5.ns4l")).unwrap();
    assert_eq!(new_live.header.kind, CbinKind::Live);
    assert_eq!(new_live.header.location_index, 5);
    // Untouched: the other Live slot and every program.
    assert_eq!(
        get(&before, "Live/Bank 1/Live 0.ns4l"),
        get(&after, "Live/Bank 1/Live 0.ns4l")
    );
    assert_eq!(
        get(&before, "Program/Bank A/Alpha.ns4p"),
        get(&after, "Program/Bank A/Alpha.ns4p")
    );
    // Export refuses to overwrite the opened file and needs an open backup.
    state.export_path = dir.join("in.ns4b").display().to_string();
    assert!(state.export_backup(&lib).is_err());
    assert!(BackupLibraryState::new().export_backup(&lib).is_err());
}

#[test]
fn live_slots_load_back_from_the_backup() {
    let (mut state, _, mut lib) = open();
    assert!(lib.live(0).is_none());
    let msg = state.import_live_slots(&mut lib).unwrap();
    assert!(msg.contains("2 Live"), "{msg}");
    let mut probe = Stage4Engine::new(48_000.0_f32);
    probe.apply_state(Box::new(lib.live(3).unwrap().clone()));
    assert_eq!(probe.master_clock.bpm, 93.0_f32);
    assert!(lib.live(0).is_some() && lib.live(1).is_none());
    assert!(BackupLibraryState::new()
        .import_live_slots(&mut lib)
        .is_err());
}
