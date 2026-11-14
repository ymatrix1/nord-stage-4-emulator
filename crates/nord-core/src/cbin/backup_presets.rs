//! Preset Library entries of a `.ns4b` backup (`.ns4o`, `.ns4n`, `.ns4y`): where each preset sits
//! and the entry edits that write a rearranged order (Preset Organize, manual p.45).
//!
//! Spec: `docs/specs/cbin-and-presets.md` section 5.5. Same scheme as the programs of
//! `backup_store.rs`: the location of a preset is in its CBIN header, so a Swap or Move rewrites
//! the headers (and the entry paths, whose folder is the bank).
//!
//! VERIFIED: in `Nord Stage 4 Factory Restore RevK v1.66.ns4b` every preset header holds
//! `bank_index` (`0x0C`, 0-based) and `location_index` (`0x0E`, `0..63`) with no duplicate
//! location: Organ 64 (bank 0), Piano 96 (banks 0 and 1), Synth 384 (banks 0..5). Check: header
//! dump with `python tools/ns4_inspect.py` (see `docs/research/piano-types.md` for the script
//! pattern). The entry folder is `<Organ|Piano|Synth> Preset/Bank <bank + 1>/`.
//! ASSUMED: banks beyond the factory content exist (Organ 2, Piano 2, Synth 8, from the spec
//! table of the backup layout); the manual does not state the capacity of the Preset Library.

use super::backup::Ns4BackupArchive;
use super::backup_writer::EntryEdit;
use super::container::{CbinError, CbinFile, CbinKind};
use std::collections::{BTreeSet, HashSet};
use std::io::{Read, Seek};

/// Locations per preset bank (the header location is `0..63`).
pub const PRESETS_PER_BANK: u16 = 64;

/// One section of the Preset Library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PresetSection {
    Organ,
    Piano,
    Synth,
}

impl PresetSection {
    pub const ALL: [PresetSection; 3] = [Self::Organ, Self::Piano, Self::Synth];

    /// File extension without the dot.
    pub fn ext(self) -> &'static str {
        match self {
            Self::Organ => "ns4o",
            Self::Piano => "ns4n",
            Self::Synth => "ns4y",
        }
    }

    /// Archive folder under which the banks live.
    pub fn folder(self) -> &'static str {
        match self {
            Self::Organ => "Organ Preset",
            Self::Piano => "Piano Preset",
            Self::Synth => "Synth Preset",
        }
    }

    /// CBIN kind a file of this section must have.
    pub fn cbin_kind(self) -> CbinKind {
        match self {
            Self::Organ => CbinKind::OrganPreset,
            Self::Piano => CbinKind::PianoPreset,
            Self::Synth => CbinKind::SynthPreset,
        }
    }

    /// Banks the section can hold. ASSUMED, see the module docs.
    pub fn banks(self) -> u16 {
        match self {
            Self::Organ | Self::Piano => 2,
            Self::Synth => 8,
        }
    }

    /// Locations of the section (`banks * 64`).
    pub fn capacity(self) -> u16 {
        self.banks() * PRESETS_PER_BANK
    }

    /// Section of an entry name, by extension.
    pub fn from_entry(name: &str) -> Option<Self> {
        let lower = name.to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|s| lower.ends_with(&format!(".{}", s.ext())))
    }
}

/// Where the archive keeps each preset, read from the entry headers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PresetIndex {
    /// `(section, bank, location, entry path)` of every preset.
    pub presets: Vec<(PresetSection, u16, u16, String)>,
    /// Every entry path of the archive.
    pub names: HashSet<String>,
}

impl PresetIndex {
    /// Reads the headers of all preset entries.
    pub fn read<R: Read + Seek>(
        archive: &Ns4BackupArchive,
        reader: &mut R,
    ) -> Result<Self, CbinError> {
        let mut idx = Self::default();
        for (i, e) in archive.entries().iter().enumerate() {
            idx.names.insert(e.name.clone());
            let Some(section) = PresetSection::from_entry(&e.name) else {
                continue;
            };
            let bytes = archive.read_entry_by_index(reader, i)?;
            let Ok(file) = CbinFile::parse(&bytes) else {
                continue;
            };
            let h = &file.header;
            idx.presets
                .push((section, h.bank_index, h.location_index, e.name.clone()));
        }
        Ok(idx)
    }

    /// Entry path of the preset at `(section, bank, loc)`.
    pub fn entry_at(&self, section: PresetSection, bank: u16, loc: u16) -> Option<&str> {
        self.presets
            .iter()
            .find(|(s, b, l, _)| *s == section && *b == bank && *l == loc)
            .map(|(_, _, _, n)| n.as_str())
    }

    /// `(flat location, entry path)` of the presets of `section` (`bank * 64 + location`).
    pub fn slots(&self, section: PresetSection) -> Vec<(u16, String)> {
        self.presets
            .iter()
            .filter(|(s, b, l, _)| *s == section && *b < section.banks() && *l < PRESETS_PER_BANK)
            .map(|(_, b, l, n)| (b * PRESETS_PER_BANK + l, n.clone()))
            .collect()
    }
}

/// A preset to place at a location. `bytes` is a complete `.ns4o` / `.ns4n` / `.ns4y`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetPut {
    pub section: PresetSection,
    pub bank: u16,
    pub loc: u16,
    /// File stem of the entry (the preset name).
    pub stem: String,
    pub bytes: Vec<u8>,
}

impl PresetPut {
    /// Places `file` at `(bank, loc)` as preset `stem`: the header location is set.
    pub fn from_file(
        section: PresetSection,
        bank: u16,
        loc: u16,
        stem: &str,
        file: &CbinFile,
    ) -> Result<Self, CbinError> {
        if file.header.kind != section.cbin_kind() {
            return Err(CbinError::UnknownTag(file.header.kind.tag()));
        }
        let mut f = file.clone();
        f.header.bank_index = bank;
        f.header.location_index = loc;
        Ok(Self {
            section,
            bank,
            loc,
            stem: stem.to_string(),
            bytes: f.to_bytes(),
        })
    }
}

/// Entry path of a preset in bank `bank` (0-based): `Synth Preset/Bank 1/<stem>.ns4y`.
pub fn preset_entry_dir(section: PresetSection, bank: u16) -> String {
    format!("{}/Bank {}/", section.folder(), bank + 1)
}

/// Entry edits that place `puts` and remove the entries of `cleared` locations.
///
/// A put replaces whatever entry sits at its location; a path the new layout no longer uses is
/// removed, so a Swap or Move inside one bank (same names, same folders) becomes plain replaces.
/// A name that another entry still holds gets a numbered path instead of overwriting it.
pub fn preset_edits(
    index: &PresetIndex,
    puts: &[PresetPut],
    cleared: &[(PresetSection, u16, u16)],
) -> Vec<EntryEdit> {
    let mut by_loc: Vec<&PresetPut> = Vec::new();
    for p in puts {
        by_loc.retain(|q| (q.section, q.bank, q.loc) != (p.section, p.bank, p.loc));
        by_loc.push(p);
    }
    let vacated: BTreeSet<&str> = by_loc
        .iter()
        .map(|p| (p.section, p.bank, p.loc))
        .chain(cleared.iter().copied())
        .filter_map(|(s, b, l)| index.entry_at(s, b, l))
        .collect();
    let mut taken: HashSet<String> = index
        .names
        .iter()
        .filter(|n| !vacated.contains(n.as_str()))
        .cloned()
        .collect();
    let mut edits = Vec::new();
    let mut new_paths = HashSet::new();
    for p in &by_loc {
        let dir = preset_entry_dir(p.section, p.bank);
        let mut path = format!("{dir}{}.{}", p.stem, p.section.ext());
        let mut n = 2;
        while taken.contains(&path) {
            path = format!("{dir}{} ({n}).{}", p.stem, p.section.ext());
            n += 1;
        }
        taken.insert(path.clone());
        new_paths.insert(path.clone());
        edits.push(EntryEdit::Put {
            name: path,
            data: p.bytes.clone(),
        });
    }
    for old in vacated {
        if !new_paths.contains(old) {
            edits.push(EntryEdit::Remove {
                name: old.to_string(),
            });
        }
    }
    edits
}

#[cfg(test)]
mod tests {
    use super::super::backup_writer::more_tests::build_zip;
    use super::*;
    use std::io::Cursor;

    /// A preset file of `section` with the header location set (payload is one filler byte).
    fn preset_bytes(section: PresetSection, bank: u16, loc: u16, tag: u8) -> Vec<u8> {
        let mut f = CbinFile::parse(&blank(section)).unwrap();
        f.header.bank_index = bank;
        f.header.location_index = loc;
        f.payload[0] = tag;
        f.to_bytes()
    }

    /// A valid empty file of the section: right size, tag and schema id.
    fn blank(section: PresetSection) -> Vec<u8> {
        let kind = section.cbin_kind();
        let schema = kind.expected_schema_id().unwrap();
        let mut b = vec![0u8; kind.expected_file_size().unwrap()];
        b[0..4].copy_from_slice(b"CBIN");
        b[4..8].copy_from_slice(&1u32.to_le_bytes());
        b[8..12].copy_from_slice(&kind.tag());
        b[20..24].copy_from_slice(&u32::from(schema).to_le_bytes());
        b[46..48].copy_from_slice(&schema.to_be_bytes());
        b
    }

    fn archive() -> Vec<u8> {
        let o = |bank, loc, tag| preset_bytes(PresetSection::Organ, bank, loc, tag);
        let s = |bank, loc, tag| preset_bytes(PresetSection::Synth, bank, loc, tag);
        build_zip(&[
            ("meta.xml", b"<backup/>"),
            ("Organ Preset/Bank 1/Alpha.ns4o", &o(0, 0, 1)),
            ("Organ Preset/Bank 1/Beta.ns4o", &o(0, 1, 2)),
            ("Synth Preset/Bank 1/Gamma.ns4y", &s(0, 0, 3)),
            ("Synth Preset/Bank 2/Delta.ns4y", &s(1, 5, 4)),
            ("Program/Bank A/x.ns4p", b"not a preset"),
        ])
    }

    fn index() -> PresetIndex {
        let zip = archive();
        let a = Ns4BackupArchive::from_bytes(&zip).unwrap();
        PresetIndex::read(&a, &mut Cursor::new(&zip)).unwrap()
    }

    #[test]
    fn sections_know_their_files_folders_and_capacity() {
        assert_eq!(
            PresetSection::from_entry("A/B.NS4N"),
            Some(PresetSection::Piano)
        );
        assert_eq!(PresetSection::from_entry("A/B.ns4p"), None);
        assert_eq!(PresetSection::Synth.folder(), "Synth Preset");
        assert_eq!(PresetSection::Synth.capacity(), 512);
        assert_eq!(PresetSection::Organ.capacity(), 128);
        assert_eq!(
            preset_entry_dir(PresetSection::Piano, 1),
            "Piano Preset/Bank 2/"
        );
    }

    #[test]
    fn index_reads_locations_by_section_and_flattens_slots() {
        let idx = index();
        assert_eq!(idx.presets.len(), 4);
        assert_eq!(
            idx.entry_at(PresetSection::Organ, 0, 1),
            Some("Organ Preset/Bank 1/Beta.ns4o")
        );
        assert_eq!(idx.entry_at(PresetSection::Piano, 0, 0), None);
        assert_eq!(
            idx.slots(PresetSection::Synth),
            [
                (0, "Synth Preset/Bank 1/Gamma.ns4y".to_string()),
                (69, "Synth Preset/Bank 2/Delta.ns4y".to_string())
            ]
        );
    }

    #[test]
    fn put_checks_the_kind_and_sets_the_header_location() {
        let f = CbinFile::parse(&blank(PresetSection::Organ)).unwrap();
        let p = PresetPut::from_file(PresetSection::Organ, 1, 9, "X", &f).unwrap();
        let back = CbinFile::parse(&p.bytes).unwrap();
        assert_eq!((back.header.bank_index, back.header.location_index), (1, 9));
        assert!(PresetPut::from_file(PresetSection::Synth, 0, 0, "X", &f).is_err());
    }

    #[test]
    fn a_swap_inside_a_bank_is_two_plain_replaces() {
        let idx = index();
        let a = CbinFile::parse(&preset_bytes(PresetSection::Organ, 0, 0, 1)).unwrap();
        let b = CbinFile::parse(&preset_bytes(PresetSection::Organ, 0, 1, 2)).unwrap();
        let edits = preset_edits(
            &idx,
            &[
                PresetPut::from_file(PresetSection::Organ, 0, 0, "Beta", &b).unwrap(),
                PresetPut::from_file(PresetSection::Organ, 0, 1, "Alpha", &a).unwrap(),
            ],
            &[],
        );
        assert_eq!(edits.len(), 2);
        assert!(edits.iter().all(|e| matches!(e, EntryEdit::Put { .. })));
        assert!(
            matches!(&edits[0], EntryEdit::Put { name, .. } if name == "Organ Preset/Bank 1/Beta.ns4o")
        );
    }

    #[test]
    fn moving_across_banks_changes_the_folder_and_clearing_removes() {
        let idx = index();
        let g = CbinFile::parse(&preset_bytes(PresetSection::Synth, 0, 0, 3)).unwrap();
        let moved = PresetPut::from_file(PresetSection::Synth, 1, 5, "Gamma", &g).unwrap();
        let edits = preset_edits(&idx, &[moved], &[(PresetSection::Synth, 0, 0)]);
        // Gamma lands in bank 2 (replacing Delta's location), Delta's old path and Gamma's old
        // path both go away.
        assert!(edits.iter().any(
            |e| matches!(e, EntryEdit::Put { name, .. } if name == "Synth Preset/Bank 2/Gamma.ns4y")
        ));
        for gone in [
            "Synth Preset/Bank 2/Delta.ns4y",
            "Synth Preset/Bank 1/Gamma.ns4y",
        ] {
            assert!(
                edits.contains(&EntryEdit::Remove { name: gone.into() }),
                "{gone} {edits:?}"
            );
        }
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        let idx = index();
        let f = CbinFile::parse(&preset_bytes(PresetSection::Organ, 0, 9, 7)).unwrap();
        let p = PresetPut::from_file(PresetSection::Organ, 0, 9, "Alpha", &f).unwrap();
        let edits = preset_edits(&idx, &[p], &[]);
        assert!(
            matches!(&edits[0], EntryEdit::Put { name, .. } if name == "Organ Preset/Bank 1/Alpha (2).ns4o")
        );
    }
}
