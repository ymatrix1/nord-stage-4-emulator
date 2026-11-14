//! Export backup and Live import: the program library (STORE, Organize) is written into a copy of
//! the opened `.ns4b`, and the archive's Live slots can be loaded back into the Live slots.
//!
//! Spec: `docs/specs/cbin-and-presets.md` section 5.5. The archive on disk is never modified: the
//! export goes to another file, and every entry the library did not change stays byte-identical.
//!
//! Limitation (ASSUMED): a stored program only carries the fields the engine maps. The rest (FX
//! chains, deep Synth data, reserved bytes) comes from the archive program that sits at the same
//! location, or from defaults when the location is empty.

use super::BackupLibraryState;
use crate::panel::bay_program::catalog::display_name;
use crate::panel::bay_program::store::{UserLibrary, PER_BANK};
use nord_core::cbin::{
    preset_edits, rewrite_backup_file, store_edits, CbinFile, LivePut, Ns4BackupArchive,
    PresetIndex, PresetPut, PresetSection, ProgramDecoded, ProgramPut, SlotIndex, PRESETS_PER_BANK,
};
use nord_core::engine::LiveSlots;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

const SAMPLE_RATE: f32 = 48_000.0_f32;

/// What the library changes in the archive.
#[derive(Debug, Default)]
pub struct ExportPlan {
    pub programs: Vec<ProgramPut>,
    pub cleared: Vec<(u16, u16)>,
    pub lives: Vec<LivePut>,
}

/// Where Export backup writes when the user did not type a path: `<name>-edited.ns4b` next to
/// the opened backup.
pub fn default_export_path(src: &Path) -> PathBuf {
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("backup");
    src.with_file_name(format!("{stem}-edited.ns4b"))
}

fn base_file(
    name: Option<&str>,
    read: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
) -> Result<Option<CbinFile>, String> {
    let Some(name) = name else { return Ok(None) };
    let bytes = read(name)?;
    CbinFile::parse(&bytes)
        .map(Some)
        .map_err(|e| format!("{name}: {e}"))
}

/// Works out the edits for `lib`. `read` returns the bytes of an archive entry.
pub fn plan_export(
    lib: &UserLibrary,
    index: &SlotIndex,
    read: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
) -> Result<ExportPlan, String> {
    let mut plan = ExportPlan::default();
    for (i, cell) in lib.cells().into_iter().enumerate() {
        let (bank, loc) = (i as u16 / PER_BANK, i as u16 % PER_BANK);
        let here = index.program_at(bank, loc);
        match (&cell.user, &cell.factory) {
            (Some(user), factory) => {
                let template = base_file(factory.as_deref(), read)?;
                let mut prog = match &template {
                    Some(t) => ProgramDecoded::from_cbin_file(t).map_err(|e| e.to_string())?,
                    None => ProgramDecoded::default(),
                };
                user.state.export_into(&mut prog);
                let file = match &template {
                    Some(t) => prog.to_cbin_file_like(t),
                    None => prog.to_cbin_file(bank, loc),
                }
                .map_err(|e| e.to_string())?;
                plan.programs.push(
                    ProgramPut::from_file(bank, loc, &user.name, &file)
                        .map_err(|e| e.to_string())?,
                );
            }
            (None, Some(entry)) if here != Some(entry.as_str()) => {
                let file = base_file(Some(entry), read)?.ok_or("missing entry")?;
                let mut put = ProgramPut::from_file(bank, loc, &display_name(entry), &file)
                    .map_err(|e| e.to_string())?;
                // The stem of an existing entry is already a valid file name: keep it as is.
                put.stem = display_name(entry);
                plan.programs.push(put);
            }
            (None, Some(_)) => {}
            (None, None) if here.is_some() => plan.cleared.push((bank, loc)),
            (None, None) => {}
        }
    }
    for slot in 0..8u8 {
        let Some(state) = lib.live(slot) else {
            continue;
        };
        let template = base_file(index.live_at(u16::from(slot)), read)?;
        let mut prog = match &template {
            Some(t) => ProgramDecoded::from_cbin_file(t).map_err(|e| e.to_string())?,
            None => ProgramDecoded::default(),
        };
        state.export_into(&mut prog);
        let file = match &template {
            Some(t) => prog.to_cbin_file_like(t),
            None => prog.to_live_cbin_file(u16::from(slot)),
        }
        .map_err(|e| e.to_string())?;
        plan.lives.push(LivePut {
            slot: u16::from(slot),
            bytes: file.to_bytes(),
        });
    }
    Ok(plan)
}

/// Preset edits of the Preset Organize layout.
#[derive(Debug, Default)]
pub struct PresetPlan {
    pub puts: Vec<PresetPut>,
    pub cleared: Vec<(PresetSection, u16, u16)>,
}

/// Works out the preset edits of the Preset Organize layout in `lib`: an entry that is not where
/// the archive keeps it is placed at its new location, a location left empty is cleared.
pub fn plan_presets(
    lib: &UserLibrary,
    index: &PresetIndex,
    read: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
) -> Result<PresetPlan, String> {
    let mut plan = PresetPlan::default();
    for section in PresetSection::ALL {
        for (i, entry) in lib.presets.cells(section).into_iter().enumerate() {
            let (bank, loc) = (i as u16 / PRESETS_PER_BANK, i as u16 % PRESETS_PER_BANK);
            let here = index.entry_at(section, bank, loc);
            match entry {
                Some(e) if here != Some(e.as_str()) => {
                    let file = base_file(Some(&e), read)?.ok_or("missing entry")?;
                    plan.puts.push(
                        PresetPut::from_file(section, bank, loc, &display_name(&e), &file)
                            .map_err(|err| format!("{e}: {err}"))?,
                    );
                }
                None if here.is_some() => plan.cleared.push((section, bank, loc)),
                _ => {}
            }
        }
    }
    Ok(plan)
}

impl BackupLibraryState {
    /// Writes the library (stored programs, Organize layout, Live slots) into a copy of the open
    /// backup at `export_path`.
    pub fn export_backup(&mut self, lib: &UserLibrary) -> Result<String, String> {
        let src = self
            .backup_path
            .clone()
            .ok_or("Open a .ns4b backup first")?;
        let dst = if self.export_path.trim().is_empty() {
            default_export_path(&src)
        } else {
            PathBuf::from(self.export_path.trim())
        };
        let archive = Ns4BackupArchive::open(&src).map_err(|e| e.to_string())?;
        let open = || {
            File::open(&src)
                .map(BufReader::new)
                .map_err(|e| format!("failed to open '{}': {e}", src.display()))
        };
        let mut reader = open()?;
        let index = SlotIndex::read(&archive, &mut reader).map_err(|e| e.to_string())?;
        let plan = plan_export(lib, &index, &mut |name| {
            archive
                .read_entry(&mut reader, name)
                .map_err(|e| e.to_string())
        })?;
        let pindex = PresetIndex::read(&archive, &mut reader).map_err(|e| e.to_string())?;
        let presets = plan_presets(lib, &pindex, &mut |name| {
            archive
                .read_entry(&mut reader, name)
                .map_err(|e| e.to_string())
        })?;
        let mut edits = store_edits(&index, &plan.programs, &plan.cleared, &plan.lives);
        edits.extend(preset_edits(&pindex, &presets.puts, &presets.cleared));
        let rep = rewrite_backup_file(&src, &dst, &edits).map_err(|e| e.to_string())?;
        Ok(format!(
            "Exported {} ({} replaced, {} added, {} removed)",
            dst.display(),
            rep.replaced,
            rep.added,
            rep.removed
        ))
    }

    /// Loads every `.ns4l` entry of the open backup into the Live slots its header names.
    pub fn import_live_slots(&mut self, lib: &mut UserLibrary) -> Result<String, String> {
        let archive = self.archive.clone().ok_or("Open a .ns4b backup first")?;
        let path = self
            .backup_path
            .clone()
            .ok_or("Open a .ns4b backup first")?;
        let mut reader = BufReader::new(
            File::open(&path).map_err(|e| format!("failed to open '{}': {e}", path.display()))?,
        );
        let mut slots = LiveSlots::default();
        let mut loaded = 0;
        for e in archive.entries_with_ext(".ns4l") {
            let bytes = archive
                .read_entry(&mut reader, &e.name)
                .map_err(|err| err.to_string())?;
            slots
                .import_live_bytes(&bytes, SAMPLE_RATE)
                .map_err(|err| format!("{}: {err}", e.name))?;
            loaded += 1;
        }
        for slot in 0..8u8 {
            if let Some(state) = slots.get(usize::from(slot)) {
                lib.put_live(slot, Box::new(state.clone()));
            }
        }
        Ok(format!("Loaded {loaded} Live slot(s) from the backup"))
    }
}

#[cfg(test)]
#[path = "export_tests.rs"]
mod more_tests;

#[cfg(test)]
#[path = "export_presets_tests.rs"]
mod preset_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_path_sits_next_to_the_backup_and_an_empty_library_plans_nothing() {
        let p = default_export_path(Path::new("/x/Factory.ns4b"));
        assert!(p.ends_with("Factory-edited.ns4b"));
        let plan = plan_export(&UserLibrary::default(), &SlotIndex::default(), &mut |_| {
            Err("unused".into())
        })
        .unwrap();
        assert!(plan.programs.is_empty() && plan.cleared.is_empty() && plan.lives.is_empty());
    }
}
