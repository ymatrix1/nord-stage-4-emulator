//! Presets stored with STORE into the Preset Library (manual OS v1.6x p.42).
//!
//! A stored preset has no archive entry yet, so its cell in [`PresetLibrary`] holds a virtual
//! path `~user/<serial>/<name>.<ext>`: Preset Organize moves it like any entry, the browser lists
//! it, and Export backup (`library/preset_export.rs`) turns it into a file. The preset itself is
//! kept as the program snapshot taken at STORE plus the archive entry the section was loaded
//! from (the base the snapshot is patched onto, see `Stage4Engine::capture_section_preset`).

use super::organize_presets::PresetLibrary;
use nord_core::cbin::PresetSection;
use nord_core::engine::ProgramState;

/// Prefix of the virtual path of a stored preset.
pub const USER_PREFIX: &str = "~user/";

/// A preset stored with STORE.
#[derive(Debug, Clone)]
pub struct UserPreset {
    pub name: String,
    pub section: PresetSection,
    /// Panel state at STORE time.
    pub state: Box<ProgramState>,
    /// Archive entry the section was loaded from, if any (header, FX and unmapped fields).
    pub base: Option<String>,
}

/// File-name safe preset name: path separators become `-`; empty becomes `Preset`.
pub fn clean_name(name: &str) -> String {
    let n: String = name
        .trim()
        .chars()
        .map(|c| if matches!(c, '/' | '\\') { '-' } else { c })
        .collect();
    if n.is_empty() {
        "Preset".to_string()
    } else {
        n
    }
}

fn slot(section: PresetSection) -> usize {
    PresetSection::ALL
        .iter()
        .position(|s| *s == section)
        .unwrap_or(0)
}

impl PresetLibrary {
    /// Flat locations of `section` that hold a preset, in order.
    pub fn occupied(&self, section: PresetSection) -> Vec<u16> {
        self.slots[slot(section)].keys().copied().collect()
    }

    /// Entry paths are stored presets (not archive entries).
    pub fn is_user(&self, entry: &str) -> bool {
        self.user.contains_key(entry)
    }

    /// The stored preset behind a virtual path.
    pub fn user(&self, entry: &str) -> Option<&UserPreset> {
        self.user.get(entry)
    }

    /// Puts `preset` at flat location `loc` of its section, replacing what is there. Returns the
    /// virtual path, or `None` when `loc` is outside the section.
    pub fn store_user(&mut self, loc: u16, mut preset: UserPreset) -> Option<String> {
        let section = preset.section;
        if loc >= section.capacity() {
            return None;
        }
        preset.name = clean_name(&preset.name);
        self.serial = self.serial.wrapping_add(1);
        let path = format!(
            "{USER_PREFIX}{}/{}.{}",
            self.serial,
            preset.name,
            section.ext()
        );
        self.slots[slot(section)].insert(loc, path.clone());
        self.user.insert(path.clone(), preset);
        self.loaded[slot(section)] = Some(path.clone());
        self.drop_unreferenced();
        Some(path)
    }

    /// Forgets stored presets no cell points at any more (overwritten or organized away).
    pub(super) fn drop_unreferenced(&mut self) {
        let used: std::collections::HashSet<&String> =
            self.slots.iter().flat_map(|m| m.values()).collect();
        self.user.retain(|k, _| used.contains(k));
    }

    /// Entry last loaded into `section` (the base of a stored preset).
    pub fn loaded(&self, section: PresetSection) -> Option<&str> {
        self.loaded[slot(section)].as_deref()
    }

    /// Records the entry just loaded into `section`.
    pub fn mark_loaded(&mut self, section: PresetSection, entry: &str) {
        self.loaded[slot(section)] = Some(entry.to_string());
    }

    /// The archive entry a stored or loaded preset is based on: the entry itself, or the base of
    /// a stored preset.
    pub fn base_entry(&self, entry: &str) -> Option<String> {
        match self.user.get(entry) {
            Some(u) => u.base.clone(),
            None => Some(entry.to_string()),
        }
    }

    /// Flat location where STORE starts: the cell of the loaded entry, else the first free one.
    pub fn store_start(&self, section: PresetSection) -> u16 {
        let cells = &self.slots[slot(section)];
        if let Some(loaded) = self.loaded(section) {
            if let Some((loc, _)) = cells.iter().find(|(_, e)| e.as_str() == loaded) {
                return *loc;
            }
        }
        (0..section.capacity())
            .find(|i| !cells.contains_key(i))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::engine::Stage4Engine;

    fn preset(name: &str, section: PresetSection) -> UserPreset {
        UserPreset {
            name: name.into(),
            section,
            state: Stage4Engine::new(48_000.0).capture_state(),
            base: Some("Organ Preset/Bank 1/B3.ns4o".into()),
        }
    }

    #[test]
    fn names_are_made_file_safe() {
        assert_eq!(clean_name(" a/b\\c "), "a-b-c");
        assert_eq!(clean_name("  "), "Preset");
    }

    #[test]
    fn a_stored_preset_takes_a_cell_and_replacing_it_forgets_the_old_one() {
        let mut lib = PresetLibrary::default();
        lib.set_factory(
            PresetSection::Organ,
            &[(0, "Organ Preset/Bank 1/A.ns4o".into())],
        );
        let p1 = lib
            .store_user(3, preset("Mine", PresetSection::Organ))
            .unwrap();
        assert_eq!(lib.entry_at(PresetSection::Organ, 3), Some(p1.as_str()));
        assert!(lib.is_user(&p1) && p1.starts_with(USER_PREFIX) && p1.ends_with("Mine.ns4o"));
        assert_eq!(
            lib.name_at(PresetSection::Organ, 3).as_deref(),
            Some("Mine")
        );
        assert_eq!(lib.occupied(PresetSection::Organ), [0, 3]);
        assert_eq!(lib.loaded(PresetSection::Organ), Some(p1.as_str()));
        let p2 = lib
            .store_user(3, preset("Mine", PresetSection::Organ))
            .unwrap();
        assert_ne!(p1, p2, "same name, new path");
        assert!(!lib.is_user(&p1) && lib.is_user(&p2));
        assert!(lib
            .store_user(128, preset("x", PresetSection::Organ))
            .is_none());
    }

    #[test]
    fn base_and_store_start_follow_the_loaded_entry() {
        let mut lib = PresetLibrary::default();
        lib.set_factory(
            PresetSection::Synth,
            &[(0, "S/a.ns4y".into()), (1, "S/b.ns4y".into())],
        );
        assert_eq!(lib.store_start(PresetSection::Synth), 2, "first free");
        lib.mark_loaded(PresetSection::Synth, "S/b.ns4y");
        assert_eq!(lib.store_start(PresetSection::Synth), 1, "the loaded cell");
        assert_eq!(lib.base_entry("S/b.ns4y").as_deref(), Some("S/b.ns4y"));
        let p = lib
            .store_user(5, preset("U", PresetSection::Synth))
            .unwrap();
        assert_eq!(
            lib.base_entry(&p).as_deref(),
            Some("Organ Preset/Bank 1/B3.ns4o")
        );
        assert_eq!(lib.base_entry("~user/none"), Some("~user/none".to_string()));
    }
}
