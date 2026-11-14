//! Piano type (`Grand`, `Upright`, `Electric`, `Clav`, `Digital`, `Misc`) of a library model.
//!
//! The type of a `.npno` model is the `bank_index` of its 48-byte CBIN header (`u16` LE at offset
//! 12), with the same index order as the `.ns4p` piano layer type field and the TYPE LEDs
//! (manual p.24): Grand 0, Upright 1, Electric 2, Clav 3, Digital 4, Misc 5. The following
//! `location_index` (`u16` LE at offset 14) is the position of the model inside its type on the
//! hardware (Grand 0 = White Grand XL ... 7 = EGrand 3 Sml).
//!
//! VERIFIED: all 38 `.npno` of `Nord Stage 4 Factory Restore RevK v1.66.ns4b` carry the type at
//! header offset 12 and it equals the archive folder (`Piano/<Folder>/`): Grand 8, Upright 7,
//! Electric 11, Clavinet 4, Digital 6, Misc 2 (0 mismatches), and `location_index` runs
//! `0..N-1` without gaps inside every type. Check:
//! `python tools/npno_types.py "<backup>.ns4b"` (see `docs/research/piano-types.md`).

use super::PianoCategory;

/// `(bank_index, location_index)` of a `.npno` CBIN header, `None` when `bytes` is not one.
fn npno_header_words(bytes: &[u8]) -> Option<(u16, u16)> {
    if bytes.len() < 16 || &bytes[0..4] != b"CBIN" || &bytes[8..12] != b"npno" {
        return None;
    }
    Some((
        u16::from_le_bytes([bytes[12], bytes[13]]),
        u16::from_le_bytes([bytes[14], bytes[15]]),
    ))
}

/// Position of a `.npno` model inside its type on the hardware (`location_index`, header offset
/// 14); `None` when `bytes` is not a `.npno` header.
pub fn npno_header_location(bytes: &[u8]) -> Option<u16> {
    npno_header_words(bytes).map(|w| w.1)
}

impl PianoCategory {
    /// The six types in TYPE LED order (manual p.24).
    pub const ALL: [PianoCategory; 6] = [
        PianoCategory::Grand,
        PianoCategory::Upright,
        PianoCategory::Electric,
        PianoCategory::Clav,
        PianoCategory::Digital,
        PianoCategory::Misc,
    ];

    /// LED / header index (`0..=5`).
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|c| *c == self).unwrap_or(0)
    }

    /// Type of a header / preset index; `None` above 5.
    pub fn from_index(i: u32) -> Option<Self> {
        Self::ALL.get(i as usize).copied()
    }

    /// Name shown on the panel and in the LIST / INFO views.
    pub fn label(self) -> &'static str {
        ["Grand", "Upright", "Electric", "Clav", "Digital", "Misc"][self.index()]
    }

    /// Type from the library folder name (`Clavinet` is the folder of the Clav type).
    pub fn from_folder(folder: &str) -> Option<Self> {
        let f = folder.trim();
        Self::ALL
            .iter()
            .copied()
            .find(|c| c.label().eq_ignore_ascii_case(f))
            .or_else(|| f.eq_ignore_ascii_case("Clavinet").then_some(Self::Clav))
    }

    /// Type of a `.npno` from the CBIN header of its bytes (`bank_index`, `u16` at offset 12).
    pub fn from_npno_header(bytes: &[u8]) -> Option<Self> {
        Self::from_index(u32::from(npno_header_words(bytes)?.0))
    }

    /// Type of an archive entry name such as `Piano/Grand/Astoria Grand Lrg 6.1.npno`: the
    /// folder right under `Piano/` (or the last folder of the path). `None` when no folder names
    /// a type.
    pub fn from_entry_path(entry: &str) -> Option<Self> {
        let parts: Vec<&str> = entry.split(['/', '\\']).collect();
        let dirs = parts.split_last().map_or(&[][..], |(_, d)| d);
        dirs.iter().rev().find_map(|d| Self::from_folder(d))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(cat: u32, tag: &[u8; 4]) -> Vec<u8> {
        let mut b = vec![0u8; 48];
        b[0..4].copy_from_slice(b"CBIN");
        b[4] = 1;
        b[8..12].copy_from_slice(tag);
        b[12..14].copy_from_slice(&(cat as u16).to_le_bytes());
        b[14..16].copy_from_slice(&7u16.to_le_bytes());
        b
    }

    #[test]
    fn index_label_and_back_round_trip() {
        for (i, c) in PianoCategory::ALL.iter().enumerate() {
            assert_eq!(c.index(), i);
            assert_eq!(PianoCategory::from_index(i as u32), Some(*c));
        }
        assert_eq!(PianoCategory::from_index(6), None);
        assert_eq!(PianoCategory::Clav.label(), "Clav");
    }

    #[test]
    fn folder_names_map_to_types() {
        assert_eq!(
            PianoCategory::from_folder("Clavinet"),
            Some(PianoCategory::Clav)
        );
        assert_eq!(
            PianoCategory::from_folder("grand"),
            Some(PianoCategory::Grand)
        );
        assert_eq!(PianoCategory::from_folder("Sfz"), None);
    }

    #[test]
    fn entry_paths_use_the_folder_under_piano() {
        let t = |p| PianoCategory::from_entry_path(p);
        assert_eq!(
            t("Piano/Grand/Astoria Grand Lrg 6.1.npno"),
            Some(PianoCategory::Grand)
        );
        assert_eq!(
            t("Piano/Clavinet/Clavinet D6  6.1.npno"),
            Some(PianoCategory::Clav)
        );
        assert_eq!(
            t("Piano\\Misc\\Marimba  6.1.npno"),
            Some(PianoCategory::Misc)
        );
        assert_eq!(
            t("Piano/Electric/Wurlitzer 1  6.3.npno"),
            Some(PianoCategory::Electric)
        );
        assert_eq!(t("Grand.npno"), None, "a bare file name has no folder");
        assert_eq!(t("Piano/Other/x.npno"), None);
    }

    #[test]
    fn header_category_matches_the_ns4p_index_order() {
        for (i, c) in PianoCategory::ALL.iter().enumerate() {
            assert_eq!(
                PianoCategory::from_npno_header(&header(i as u32, b"npno")),
                Some(*c)
            );
        }
        assert_eq!(PianoCategory::from_npno_header(&header(9, b"npno")), None);
        assert_eq!(PianoCategory::from_npno_header(&header(1, b"nsmp")), None);
        assert_eq!(PianoCategory::from_npno_header(b"CBIN"), None);
        assert_eq!(npno_header_location(&header(2, b"npno")), Some(7));
        assert_eq!(npno_header_location(b"CBIN"), None);
    }

    /// Output of `process_timbre` for a fixed two-tone input (800 Hz + 6 kHz).
    fn timbre_run(cat: PianoCategory, timbre: crate::piano::PianoTimbre) -> Vec<f32> {
        let fs = 48_000.0_f32;
        let mut l = crate::piano::PianoLayer::new(fs);
        l.category = cat;
        l.timbre = timbre;
        (0..2048)
            .map(|n| {
                let t = n as f32 / fs;
                let x = (std::f32::consts::TAU * 800.0 * t).sin()
                    + (std::f32::consts::TAU * 6000.0 * t).sin();
                l.process_timbre(x, x, 1.0 / fs)[0]
            })
            .collect()
    }

    #[test]
    fn electric_tailors_soft_mid_bright_and_other_types_keep_the_acoustic_set() {
        use crate::piano::PianoTimbre as T;
        for t in [T::Soft, T::Mid, T::Bright] {
            let grand = timbre_run(PianoCategory::Grand, t);
            assert_eq!(grand, timbre_run(PianoCategory::Upright, t), "{t:?}");
            assert_eq!(grand, timbre_run(PianoCategory::Digital, t), "{t:?}");
            assert_ne!(grand, timbre_run(PianoCategory::Electric, t), "{t:?}");
        }
        for t in [T::Off, T::Dyno1, T::Dyno2] {
            assert_eq!(
                timbre_run(PianoCategory::Grand, t),
                timbre_run(PianoCategory::Electric, t),
                "{t:?}"
            );
        }
    }

    #[test]
    fn electric_soft_cuts_the_treble_more_than_bright_and_stays_finite() {
        use crate::piano::PianoTimbre as T;
        let rms = |v: &[f32]| (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt();
        let soft = timbre_run(PianoCategory::Electric, T::Soft);
        let bright = timbre_run(PianoCategory::Electric, T::Bright);
        assert!(soft.iter().chain(&bright).all(|x| x.is_finite()));
        assert!(rms(&soft) < rms(&bright), "{} {}", rms(&soft), rms(&bright));
    }
}
