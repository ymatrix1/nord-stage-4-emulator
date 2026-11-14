//! Piano model catalog of an opened `.ns4b`: every `.npno` entry with its piano type and its
//! position inside the type, in hardware order (type, then `location_index`).
//!
//! The type and position come from the 16-byte CBIN header of each stored entry
//! (`PianoCategory::from_npno_header`, `npno_header_location`; VERIFIED on the 38 factory models,
//! see `nord_core::piano::category`). Only the header is read, never the multi-megabyte sample
//! data. A model whose header cannot be read falls back to its archive folder, then to `Misc`.

use nord_core::cbin::Ns4BackupArchive;
use nord_core::piano::category::npno_header_location;
use nord_core::piano::PianoCategory;
use std::io::{Read, Seek};

/// One `.npno` model of the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PianoModel {
    /// Archive entry name, e.g. `Piano/Grand/White Grand XL 6.3.npno`.
    pub entry: String,
    /// Piano type (TYPE button / LEDs).
    pub category: PianoCategory,
    /// Position inside the type on the hardware (`None` when the header was not readable).
    pub location: Option<u16>,
}

impl PianoModel {
    /// Build a model from its entry name and the first bytes of the entry.
    pub fn classify(entry: &str, head: &[u8]) -> Self {
        Self {
            entry: entry.to_string(),
            category: PianoCategory::from_npno_header(head)
                .or_else(|| PianoCategory::from_entry_path(entry))
                .unwrap_or(PianoCategory::Misc),
            location: npno_header_location(head),
        }
    }
}

/// Sort into hardware order: type, then header position, then name.
pub fn sort_models(models: &mut [PianoModel]) {
    models.sort_by(|a, b| {
        (a.category.index(), a.location.unwrap_or(u16::MAX), &a.entry).cmp(&(
            b.category.index(),
            b.location.unwrap_or(u16::MAX),
            &b.entry,
        ))
    });
}

/// Read the header of every `.npno` entry of `archive` and return the models in hardware order.
pub fn read_models<R: Read + Seek>(archive: &Ns4BackupArchive, reader: &mut R) -> Vec<PianoModel> {
    let mut models: Vec<PianoModel> = archive
        .entries()
        .iter()
        .enumerate()
        .filter(|(_, e)| e.name.to_ascii_lowercase().ends_with(".npno"))
        .map(|(i, e)| {
            let head = archive
                .read_entry_prefix_by_index(reader, i, 16)
                .unwrap_or_default();
            PianoModel::classify(&e.name, &head)
        })
        .collect();
    sort_models(&mut models);
    models
}

#[cfg(test)]
mod tests {
    use super::super::support::build_synthetic_zip_stored;
    use super::*;

    fn head(cat: u16, loc: u16) -> Vec<u8> {
        let mut b = vec![0u8; 48];
        b[0..4].copy_from_slice(b"CBIN");
        b[4] = 1;
        b[8..12].copy_from_slice(b"npno");
        b[12..14].copy_from_slice(&cat.to_le_bytes());
        b[14..16].copy_from_slice(&loc.to_le_bytes());
        b
    }

    #[test]
    fn header_wins_over_the_folder_and_unreadable_files_fall_back() {
        let m = PianoModel::classify("Piano/Grand/x.npno", &head(2, 4));
        assert_eq!((m.category, m.location), (PianoCategory::Electric, Some(4)));
        let m = PianoModel::classify("Piano/Clavinet/y.npno", b"junk");
        assert_eq!((m.category, m.location), (PianoCategory::Clav, None));
        assert_eq!(
            PianoModel::classify("loose.npno", b"").category,
            PianoCategory::Misc
        );
    }

    #[test]
    fn archive_models_come_back_in_hardware_order_with_their_types() {
        let (a, b, c, d) = (head(0, 1), head(0, 0), head(3, 0), head(2, 0));
        let zip = build_synthetic_zip_stored(&[
            ("Piano/Grand/Zeta.npno", &a),
            ("Piano/Grand/Alpha.npno", &b),
            ("Piano/Clavinet/Clav.npno", &c),
            ("Piano/Electric/Ep.npno", &d),
            ("Program/x.ns4p", b"x"),
        ]);
        let archive = Ns4BackupArchive::from_bytes(&zip).unwrap();
        let models = read_models(&archive, &mut std::io::Cursor::new(zip));
        let got: Vec<_> = models
            .iter()
            .map(|m| (m.entry.as_str(), m.category))
            .collect();
        assert_eq!(
            got,
            [
                ("Piano/Grand/Alpha.npno", PianoCategory::Grand),
                ("Piano/Grand/Zeta.npno", PianoCategory::Grand),
                ("Piano/Electric/Ep.npno", PianoCategory::Electric),
                ("Piano/Clavinet/Clav.npno", PianoCategory::Clav),
            ],
            "Zeta is Grand location 1, Alpha location 0"
        );
    }

    #[test]
    fn sorting_puts_unlocated_models_last_inside_their_type() {
        let mut v = vec![
            PianoModel::classify("Piano/Grand/b.npno", b""),
            PianoModel::classify("Piano/Grand/a.npno", &head(0, 3)),
            PianoModel::classify("Piano/Upright/u.npno", &head(1, 0)),
        ];
        sort_models(&mut v);
        let names: Vec<_> = v.iter().map(|m| m.entry.as_str()).collect();
        assert_eq!(
            names,
            [
                "Piano/Grand/a.npno",
                "Piano/Grand/b.npno",
                "Piano/Upright/u.npno"
            ]
        );
    }
}
