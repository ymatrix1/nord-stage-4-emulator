//! Stores programs and Live slots into a `.ns4b` backup: locates the entry of a bank location,
//! plans the entry edits and hands them to the writer (`backup_writer.rs`).
//!
//! Spec: `docs/specs/cbin-and-presets.md` section 5.5. Used by `ns4 backup-store` and by the
//! GUI "Export backup" action.

use super::backup::Ns4BackupArchive;
use super::backup_writer::{rewrite_backup_file, EntryEdit, RewriteReport};
use super::container::{CbinError, CbinFile, CbinKind};
use std::collections::{BTreeSet, HashSet};
use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;

/// Archive path of a Live slot file the archive does not have yet.
/// VERIFIED: Factory Restore RevK v1.66 holds `Live/Bank 1/Live 0.ns4l` .. `Live 7.ns4l` with
/// header bank 0 and location = slot (`python tools/ns4_inspect.py` header dump, 8 of 8 entries).
pub fn live_entry_name(slot: u16) -> String {
    format!("Live/Bank 1/Live {slot}.ns4l")
}

/// Bank letter of folder names (`Bank A`): banks are 0-based, `A..H`.
fn bank_letter(bank: u16) -> char {
    (b'A' + bank.min(7) as u8) as char
}

/// Archive file stem for a program name: path separators and control characters become `_`.
pub fn sanitize_stem(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if "/\\:*?\"<>|".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let s = s.trim().to_string();
    if s.is_empty() {
        "Untitled".to_string()
    } else {
        s
    }
}

/// Where the archive keeps each program and Live slot, read from the entry headers.
///
/// VERIFIED: in the Factory Restore archive the `.ns4p` header holds `bank_index` (`0x0C`, `A` = 0)
/// and `location_index` (`0x0E`, `0..63`); banks A-F are full (384 programs), G and H are empty.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SlotIndex {
    /// `(bank, location, entry path)` of every `.ns4p`.
    pub programs: Vec<(u16, u16, String)>,
    /// `(slot, entry path)` of every `.ns4l`.
    pub lives: Vec<(u16, String)>,
    /// Every entry path of the archive.
    pub names: HashSet<String>,
}

impl SlotIndex {
    /// Reads the headers of all program and Live entries.
    pub fn read<R: Read + Seek>(
        archive: &Ns4BackupArchive,
        reader: &mut R,
    ) -> Result<Self, CbinError> {
        let mut idx = Self::default();
        for (i, e) in archive.entries().iter().enumerate() {
            idx.names.insert(e.name.clone());
            let lower = e.name.to_ascii_lowercase();
            let is_prog = lower.ends_with(".ns4p");
            if !is_prog && !lower.ends_with(".ns4l") {
                continue;
            }
            let bytes = archive.read_entry_by_index(reader, i)?;
            let Ok(file) = CbinFile::parse(&bytes) else {
                continue;
            };
            let (bank, loc) = (file.header.bank_index, file.header.location_index);
            if is_prog {
                idx.programs.push((bank, loc, e.name.clone()));
            } else {
                idx.lives.push((loc, e.name.clone()));
            }
        }
        Ok(idx)
    }

    /// Opens `path` and reads its index.
    pub fn read_file(path: &Path) -> Result<Self, CbinError> {
        let archive = Ns4BackupArchive::open(path)?;
        let f = File::open(path).map_err(|e| CbinError::BitstreamError(e.to_string()))?;
        Self::read(&archive, &mut BufReader::new(f))
    }

    /// Entry path of the program at `(bank, loc)`.
    pub fn program_at(&self, bank: u16, loc: u16) -> Option<&str> {
        self.programs
            .iter()
            .find(|(b, l, _)| *b == bank && *l == loc)
            .map(|(_, _, n)| n.as_str())
    }

    /// Entry path of Live slot `slot`: by header, else by the standard name.
    pub fn live_at(&self, slot: u16) -> Option<&str> {
        self.lives
            .iter()
            .find(|(s, _)| *s == slot)
            .map(|(_, n)| n.as_str())
    }
}

/// A program to place at a bank location. `bytes` is a complete `.ns4p`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramPut {
    pub bank: u16,
    pub loc: u16,
    /// File stem of the entry (usually the program name).
    pub stem: String,
    pub bytes: Vec<u8>,
}

impl ProgramPut {
    /// Places `file` at `(bank, loc)` as program `name`: the header location is set. The program
    /// name is not part of the `.ns4p` payload, only of the archive file name (the stem).
    pub fn from_file(bank: u16, loc: u16, name: &str, file: &CbinFile) -> Result<Self, CbinError> {
        if file.header.kind != CbinKind::Program {
            return Err(CbinError::UnknownTag(file.header.kind.tag()));
        }
        let mut f = file.clone();
        f.header.bank_index = bank;
        f.header.location_index = loc;
        Ok(Self {
            bank,
            loc,
            stem: sanitize_stem(name),
            bytes: f.to_bytes(),
        })
    }
}

/// A Live slot file to place into slot `slot`. `bytes` is a complete `.ns4l`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LivePut {
    pub slot: u16,
    pub bytes: Vec<u8>,
}

/// First path `base` (`<dir><stem>.ns4p`) that no other entry uses; `taken` is updated.
fn free_path(dir: &str, stem: &str, ext: &str, taken: &mut HashSet<String>) -> String {
    let mut path = format!("{dir}{stem}.{ext}");
    let mut n = 2;
    while taken.contains(&path) {
        path = format!("{dir}{stem} ({n}).{ext}");
        n += 1;
    }
    taken.insert(path.clone());
    path
}

/// Entry edits that place `programs` and `lives` into the archive described by `index`.
///
/// A program replaces whatever entry sits at its location; its new path is
/// `Program/Bank <letter>/<stem>.ns4p`. Paths the new set no longer uses are removed, so Swap and
/// Move results (every entry rewritten with a new header location) become plain replaces when the
/// names stay in the same bank folder. Later puts for the same location win. `cleared` locations
/// lose their entry without a replacement.
pub fn store_edits(
    index: &SlotIndex,
    programs: &[ProgramPut],
    cleared: &[(u16, u16)],
    lives: &[LivePut],
) -> Vec<EntryEdit> {
    let mut by_loc: Vec<&ProgramPut> = Vec::new();
    for p in programs {
        by_loc.retain(|q| (q.bank, q.loc) != (p.bank, p.loc));
        by_loc.push(p);
    }
    // Entries being overwritten free their path; everything else stays taken.
    let vacated: BTreeSet<&str> = by_loc
        .iter()
        .map(|p| (p.bank, p.loc))
        .chain(cleared.iter().copied())
        .filter_map(|(b, l)| index.program_at(b, l))
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
        let dir = format!("Program/Bank {}/", bank_letter(p.bank));
        let path = free_path(&dir, &p.stem, "ns4p", &mut taken);
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
    for l in lives {
        let name = index
            .live_at(l.slot)
            .map_or_else(|| live_entry_name(l.slot), str::to_string);
        edits.push(EntryEdit::Put {
            name,
            data: l.bytes.clone(),
        });
    }
    edits
}

/// Reads `src`, places `programs` and `lives`, writes the result to `dst`.
pub fn store_into_backup(
    src: &Path,
    dst: &Path,
    programs: &[ProgramPut],
    lives: &[LivePut],
) -> Result<RewriteReport, CbinError> {
    let index = SlotIndex::read_file(src)?;
    rewrite_backup_file(src, dst, &store_edits(&index, programs, &[], lives))
}

#[cfg(test)]
mod tests {
    use super::super::backup_writer::more_tests::build_zip;
    use super::super::presets::ProgramDecoded;
    use super::*;
    use std::io::Cursor;

    fn prog_bytes(name: &str, bank: u16, loc: u16) -> Vec<u8> {
        let _ = name;
        ProgramDecoded::default()
            .to_cbin_file(bank, loc)
            .unwrap()
            .to_bytes()
    }

    fn live_bytes(slot: u16) -> Vec<u8> {
        ProgramDecoded::default()
            .to_live_cbin_file(slot)
            .unwrap()
            .to_bytes()
    }

    fn archive() -> Vec<u8> {
        build_zip(&[
            ("meta.xml", b"<backup/>"),
            ("Program/Bank A/Alpha.ns4p", &prog_bytes("Alpha", 0, 0)),
            ("Program/Bank A/Beta.ns4p", &prog_bytes("Beta", 0, 1)),
            ("Program/Bank B/Gamma.ns4p", &prog_bytes("Gamma", 1, 0)),
            ("Live/Bank 1/Live 0.ns4l", &live_bytes(0)),
            ("Live/Bank 1/Live 1.ns4l", &live_bytes(1)),
        ])
    }

    fn index() -> (SlotIndex, Vec<u8>) {
        let zip = archive();
        let a = Ns4BackupArchive::from_bytes(&zip).unwrap();
        (SlotIndex::read(&a, &mut Cursor::new(&zip)).unwrap(), zip)
    }

    fn apply(zip: &[u8], edits: &[EntryEdit]) -> Vec<(String, Vec<u8>)> {
        let a = Ns4BackupArchive::from_bytes(zip).unwrap();
        let mut out = Vec::new();
        super::super::backup_writer::rewrite_backup(&a, &mut Cursor::new(zip), &mut out, edits)
            .unwrap();
        let b = Ns4BackupArchive::from_bytes(&out).unwrap();
        let mut cur = Cursor::new(&out);
        (0..b.entries().len())
            .map(|i| {
                (
                    b.entries()[i].name.clone(),
                    b.read_entry_by_index(&mut cur, i).unwrap(),
                )
            })
            .collect()
    }

    #[test]
    fn index_reads_program_and_live_locations() {
        let (idx, _) = index();
        assert_eq!(idx.program_at(0, 1), Some("Program/Bank A/Beta.ns4p"));
        assert_eq!(idx.program_at(1, 0), Some("Program/Bank B/Gamma.ns4p"));
        assert_eq!(idx.program_at(7, 63), None);
        assert_eq!(idx.live_at(1), Some("Live/Bank 1/Live 1.ns4l"));
        assert_eq!(idx.live_at(5), None);
        assert!(idx.names.contains("meta.xml"));
    }

    #[test]
    fn modified_program_changes_only_its_entry_and_keeps_meta_xml() {
        let (idx, zip) = index();
        let edited = prog_bytes("Beta", 0, 1);
        let mut file = CbinFile::parse(&edited).unwrap();
        file.payload[3] ^= 0x10; // an edited parameter bit
        let put = ProgramPut::from_file(0, 1, "Beta", &file).unwrap();
        let edits = store_edits(&idx, std::slice::from_ref(&put), &[], &[]);
        assert_eq!(edits.len(), 1, "same name and bank: a plain replace");
        let before = apply(&zip, &[]);
        let after = apply(&zip, &edits);
        for (b, a) in before.iter().zip(&after) {
            if a.0 == "Program/Bank A/Beta.ns4p" {
                assert_eq!(a.1, put.bytes);
                assert_ne!(a.1, b.1);
            } else {
                assert_eq!(a, b);
            }
        }
    }

    #[test]
    fn rename_move_to_a_new_location_and_collisions() {
        let (idx, zip) = index();
        // New name at an occupied location: old entry removed, new one added.
        let renamed = ProgramPut::from_file(
            0,
            1,
            "Delta",
            &CbinFile::parse(&prog_bytes("Delta", 0, 1)).unwrap(),
        )
        .unwrap();
        let names: Vec<String> = apply(&zip, &store_edits(&idx, &[renamed], &[], &[]))
            .into_iter()
            .map(|e| e.0)
            .collect();
        assert!(names.contains(&"Program/Bank A/Delta.ns4p".to_string()));
        assert!(!names.contains(&"Program/Bank A/Beta.ns4p".to_string()));
        // Empty location in bank C: the entry is added under that bank folder.
        let fresh = ProgramPut::from_file(
            2,
            5,
            "Alpha",
            &CbinFile::parse(&prog_bytes("Alpha", 2, 5)).unwrap(),
        )
        .unwrap();
        // A put named like an entry elsewhere gets a numbered path instead of overwriting it.
        let e = store_edits(
            &idx,
            &[ProgramPut {
                bank: 0,
                loc: 9,
                ..fresh.clone()
            }],
            &[],
            &[],
        );
        assert!(
            matches!(&e[0], EntryEdit::Put { name, .. } if name == "Program/Bank A/Alpha (2).ns4p")
        );
        let e = store_edits(&idx, &[fresh], &[], &[]);
        assert!(
            matches!(&e[0], EntryEdit::Put { name, .. } if name == "Program/Bank C/Alpha.ns4p")
        );
    }

    #[test]
    fn swapped_programs_become_plain_replaces() {
        let (idx, zip) = index();
        let a = CbinFile::parse(&prog_bytes("Alpha", 0, 0)).unwrap();
        let b = CbinFile::parse(&prog_bytes("Beta", 0, 1)).unwrap();
        let edits = store_edits(
            &idx,
            &[
                ProgramPut::from_file(0, 0, "Beta", &b).unwrap(),
                ProgramPut::from_file(0, 1, "Alpha", &a).unwrap(),
            ],
            &[],
            &[],
        );
        assert!(
            edits.iter().all(|e| matches!(e, EntryEdit::Put { .. })),
            "{edits:?}"
        );
        let after = apply(&zip, &edits);
        let at = |n: &str| CbinFile::parse(&after.iter().find(|e| e.0 == n).unwrap().1).unwrap();
        assert_eq!(at("Program/Bank A/Beta.ns4p").header.location_index, 0);
        assert_eq!(at("Program/Bank A/Alpha.ns4p").header.location_index, 1);
    }

    #[test]
    fn live_slot_replaces_by_header_or_is_added_and_stems_are_clean() {
        let (idx, zip) = index();
        let mut changed = live_bytes(1);
        changed[100] ^= 1;
        let edits = store_edits(
            &idx,
            &[],
            &[],
            &[
                LivePut {
                    slot: 1,
                    bytes: changed.clone(),
                },
                LivePut {
                    slot: 6,
                    bytes: live_bytes(6),
                },
            ],
        );
        let after = apply(&zip, &edits);
        assert_eq!(
            after
                .iter()
                .find(|e| e.0 == "Live/Bank 1/Live 1.ns4l")
                .unwrap()
                .1,
            changed
        );
        assert!(after.iter().any(|e| e.0 == "Live/Bank 1/Live 6.ns4l"));
        assert_eq!(sanitize_stem(" A/B:C "), "A_B_C");
        assert_eq!(sanitize_stem("  "), "Untitled");
        assert_eq!(live_entry_name(3), "Live/Bank 1/Live 3.ns4l");
        assert!(
            ProgramPut::from_file(0, 0, "x", &CbinFile::parse(&live_bytes(0)).unwrap()).is_err()
        );
    }

    #[test]
    fn cleared_locations_lose_their_entry() {
        let (idx, zip) = index();
        let edits = store_edits(&idx, &[], &[(0, 1), (7, 7)], &[]);
        assert_eq!(
            edits,
            [EntryEdit::Remove {
                name: "Program/Bank A/Beta.ns4p".into()
            }]
        );
        let names: Vec<String> = apply(&zip, &edits).into_iter().map(|e| e.0).collect();
        assert!(!names.contains(&"Program/Bank A/Beta.ns4p".to_string()));
        assert_eq!(names.len(), 5);
    }
}
