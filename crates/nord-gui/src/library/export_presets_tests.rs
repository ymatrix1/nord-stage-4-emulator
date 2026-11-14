//! Preset Organize written by Export backup, on a synthetic archive with Organ and Synth presets.

use super::super::support::build_synthetic_zip_stored;
use super::*;
use nord_core::library_ops::{OrganizeOp, OrganizeUndo};

/// A valid empty preset file of `section` at `(bank, loc)`; `tag` marks the payload.
fn preset(section: PresetSection, bank: u16, loc: u16, tag: u8) -> Vec<u8> {
    let kind = section.cbin_kind();
    let schema = kind.expected_schema_id().unwrap();
    let mut b = vec![0u8; kind.expected_file_size().unwrap()];
    b[0..4].copy_from_slice(b"CBIN");
    b[4..8].copy_from_slice(&1u32.to_le_bytes());
    b[8..12].copy_from_slice(&kind.tag());
    b[12..14].copy_from_slice(&bank.to_le_bytes());
    b[14..16].copy_from_slice(&loc.to_le_bytes());
    b[20..24].copy_from_slice(&u32::from(schema).to_le_bytes());
    b[46..48].copy_from_slice(&schema.to_be_bytes());
    b[48] = tag;
    b
}

const OA: &str = "Organ Preset/Bank 1/Alpha.ns4o";
const OB: &str = "Organ Preset/Bank 1/Beta.ns4o";
const SG: &str = "Synth Preset/Bank 1/Gamma.ns4y";

fn open() -> (BackupLibraryState, PathBuf, UserLibrary) {
    let dir = std::env::temp_dir().join(format!(
        "ns4-export-presets-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("in.ns4b");
    let o = PresetSection::Organ;
    let zip = build_synthetic_zip_stored(&[
        ("meta.xml", b"<backup product_id=\"46\"/>"),
        (OA, &preset(o, 0, 0, 1)),
        (OB, &preset(o, 0, 1, 2)),
        (SG, &preset(PresetSection::Synth, 0, 0, 3)),
        ("Piano/Grand/Big.npno", b"piano-bytes"),
    ]);
    std::fs::write(&src, zip).unwrap();
    let mut state = BackupLibraryState::new();
    state.open_backup_path(&src).unwrap();
    let mut lib = UserLibrary::default();
    for s in PresetSection::ALL {
        let slots: Vec<(u16, String)> = state
            .preset_slots
            .iter()
            .filter(|(sec, _, _)| *sec == s)
            .map(|(_, i, n)| (*i, n.clone()))
            .collect();
        lib.presets.set_factory(s, &slots);
    }
    (state, dir, lib)
}

fn header(path: &Path, name: &str) -> (u16, u16, u8) {
    let a = Ns4BackupArchive::open(path).unwrap();
    let bytes = a.read_entry_from_file(path, name).unwrap();
    let h = CbinFile::parse(&bytes).unwrap().header;
    (h.bank_index, h.location_index, bytes[48])
}

#[test]
fn the_preset_layout_comes_from_the_entry_headers() {
    let (state, _, lib) = open();
    assert_eq!(state.preset_slots.len(), 3);
    assert_eq!(lib.presets.entry_at(PresetSection::Organ, 1), Some(OB));
    assert_eq!(lib.presets.count(PresetSection::Piano), 0);
}

#[test]
fn an_untouched_preset_library_adds_no_edits() {
    let (mut state, dir, lib) = open();
    let msg = state.export_backup(&lib).unwrap();
    assert!(msg.contains("0 replaced"), "{msg}");
    assert_eq!(
        std::fs::read(dir.join("in.ns4b")).unwrap(),
        std::fs::read(default_export_path(&dir.join("in.ns4b"))).unwrap()
    );
}

#[test]
fn a_swap_rewrites_only_the_two_preset_headers_and_move_to_another_bank_renames() {
    let (mut state, dir, mut lib) = open();
    let mut undo = OrganizeUndo::default();
    let o = PresetSection::Organ;
    assert!(lib.presets.organize(&mut undo, o, OrganizeOp::Swap, (0, 1)));
    // Synth: move Gamma from 1:01 to 2:03 (bank 2 is a new folder).
    let mut undo = OrganizeUndo::default();
    assert!(lib
        .presets
        .organize(&mut undo, PresetSection::Synth, OrganizeOp::Move, (0, 66)));
    state.export_path = dir.join("out.ns4b").display().to_string();
    state.export_backup(&lib).unwrap();

    let (src, out) = (dir.join("in.ns4b"), dir.join("out.ns4b"));
    // Alpha now sits at location 1 and Beta at 0, payload bytes (tags) unchanged.
    assert_eq!(header(&out, OA), (0, 1, 1));
    assert_eq!(header(&out, OB), (0, 0, 2));
    assert_eq!(header(&out, "Synth Preset/Bank 2/Gamma.ns4y"), (1, 2, 3));
    let after = Ns4BackupArchive::open(&out).unwrap();
    assert!(after.entries().iter().all(|e| e.name != SG));
    for n in ["meta.xml", "Piano/Grand/Big.npno"] {
        let a = Ns4BackupArchive::open(&src).unwrap();
        assert_eq!(
            a.read_entry_from_file(&src, n).unwrap(),
            after.read_entry_from_file(&out, n).unwrap(),
            "{n}"
        );
    }
}

#[test]
fn undo_before_export_leaves_the_archive_identical() {
    let (mut state, dir, mut lib) = open();
    let mut undo = OrganizeUndo::default();
    let o = PresetSection::Organ;
    lib.presets.organize(&mut undo, o, OrganizeOp::Swap, (0, 1));
    assert!(lib.presets.undo_organize(&mut undo, o));
    state.export_backup(&lib).unwrap();
    assert_eq!(
        std::fs::read(dir.join("in.ns4b")).unwrap(),
        std::fs::read(default_export_path(&dir.join("in.ns4b"))).unwrap()
    );
}

#[test]
fn a_cleared_location_removes_its_entry() {
    let mut lib = UserLibrary::default();
    // The planner compares the library with an index that still knows both entries.
    let zip = build_synthetic_zip_stored(&[
        (OA, &preset(PresetSection::Organ, 0, 0, 1)),
        (OB, &preset(PresetSection::Organ, 0, 1, 2)),
    ]);
    let a = Ns4BackupArchive::from_bytes(&zip).unwrap();
    let pindex = PresetIndex::read(&a, &mut std::io::Cursor::new(&zip)).unwrap();
    lib.presets
        .set_factory(PresetSection::Organ, &[(0, OA.into())]);
    let plan = plan_presets(&lib, &pindex, &mut |n| {
        a.read_entry(&mut std::io::Cursor::new(&zip), n)
            .map_err(|e| e.to_string())
    })
    .unwrap();
    assert!(plan.puts.is_empty());
    assert_eq!(plan.cleared, [(PresetSection::Organ, 0, 1)]);
}

#[test]
fn a_stored_preset_is_exported_as_a_file_at_its_location() {
    use crate::panel::bay_program::preset_user::UserPreset;
    use nord_core::cbin::SectionPreset;
    use nord_core::engine::Stage4Engine;
    let (mut state, dir, mut lib) = open();
    let mut e = Stage4Engine::new(48_000.0);
    e.organ_a.drawbars = [8, 8, 0, 0, 0, 0, 0, 0, 1];
    let user = UserPreset {
        name: "Mine".into(),
        section: PresetSection::Organ,
        state: e.capture_state(),
        base: None,
    };
    lib.presets.store_user(70, user).expect("location 2:07"); // bank 2 is a new folder
    state.export_path = dir.join("stored.ns4b").display().to_string();
    let msg = state.export_backup(&lib).unwrap();
    assert!(msg.contains("1 added"), "{msg}");

    let out = dir.join("stored.ns4b");
    let a = Ns4BackupArchive::open(&out).unwrap();
    let name = "Organ Preset/Bank 2/Mine.ns4o";
    let bytes = a.read_entry_from_file(&out, name).unwrap();
    let f = CbinFile::parse(&bytes).unwrap();
    assert_eq!((f.header.bank_index, f.header.location_index), (1, 6));
    assert!(f.checksum_ok());
    let SectionPreset::Organ(o) = SectionPreset::from_file(&f).unwrap() else {
        panic!("organ expected")
    };
    assert_eq!(o.drawbars_a, [8, 8, 0, 0, 0, 0, 0, 0, 1]);
    // the archive's own presets are untouched
    let src = dir.join("in.ns4b");
    let before = Ns4BackupArchive::open(&src).unwrap();
    assert_eq!(
        before.read_entry_from_file(&src, OA).unwrap(),
        a.read_entry_from_file(&out, OA).unwrap()
    );
}
