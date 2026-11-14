//! A Preset Library entry as the engine uses it: the decoded section block of an `.ns4o`
//! (Organ), `.ns4n` (Piano) or `.ns4y` (Synth) file, with lossless file encoders.
//!
//! Spec: `docs/specs/cbin-and-presets.md` section 5.6.
//!
//! VERIFIED: the payload of a preset file is byte for byte the section block of a program
//! (`135` B Organ `0x11`, `147` B Piano `0x0C`, `493` B Synth `0x1E`), so the program decoders
//! (`OrganPresetDecoded`, `PianoPresetDecoded`, `SynthPresetDecoded`) decode presets unchanged.
//! Check: `ns4 preset-check <backup>` decodes and re-packs all 544 presets of the Factory
//! Restore backup and compares the payload bytes (see the spec for the output).

use super::backup::{Ns4BackupArchive, ProgramSoundRefs};
use super::backup_presets::PresetSection;
use super::container::{CbinError, CbinFile, CbinHeader, CbinKind};
use super::presets::{OrganPresetDecoded, PianoPresetDecoded, ProgramDecoded};
use super::synth_preset::SynthPresetDecoded;
use crate::morph::LayerId;

/// The decoded block of one preset file.
#[derive(Debug, Clone, PartialEq)]
pub enum SectionPreset {
    Organ(Box<OrganPresetDecoded>),
    Piano(Box<PianoPresetDecoded>),
    Synth(Box<SynthPresetDecoded>),
}

impl SectionPreset {
    /// The factory-default ("init") preset of `section`.
    pub fn init(section: PresetSection) -> Self {
        match section {
            PresetSection::Organ => Self::Organ(Box::default()),
            PresetSection::Piano => Self::Piano(Box::default()),
            PresetSection::Synth => Self::Synth(Box::default()),
        }
    }

    /// Section the preset belongs to.
    pub fn section(&self) -> PresetSection {
        match self {
            Self::Organ(_) => PresetSection::Organ,
            Self::Piano(_) => PresetSection::Piano,
            Self::Synth(_) => PresetSection::Synth,
        }
    }

    /// Decodes an `.ns4o` / `.ns4n` / `.ns4y` file; any other kind is refused.
    pub fn from_file(file: &CbinFile) -> Result<Self, CbinError> {
        Ok(match file.header.kind {
            CbinKind::OrganPreset => Self::Organ(Box::new(file.decode_organ_preset()?)),
            CbinKind::PianoPreset => Self::Piano(Box::new(file.decode_piano_preset()?)),
            CbinKind::SynthPreset => Self::Synth(Box::new(file.decode_synth_preset()?)),
            other => return Err(CbinError::UnknownTag(other.tag())),
        })
    }

    /// Parses and decodes preset file bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CbinError> {
        Self::from_file(&CbinFile::parse(bytes)?)
    }

    /// The block bytes (`135` / `147` / `493`). Fields the decoder does not map keep the bits of
    /// the file this preset was decoded from, so an unedited preset packs bit-identically.
    pub fn pack(&self) -> Result<Vec<u8>, CbinError> {
        match self {
            Self::Organ(p) => p.pack(),
            Self::Piano(p) => p.pack(),
            Self::Synth(p) => p.pack(),
        }
    }

    /// A new file at `(bank, loc)`: header from scratch (reserved bytes zero), CRC-32 checksum.
    /// `category_id` is the header category (open question OQ26: no name mapping yet).
    pub fn to_file(&self, bank: u16, loc: u16, category_id: u32) -> Result<CbinFile, CbinError> {
        let kind = self.section().cbin_kind();
        let schema_id = kind
            .expected_schema_id()
            .ok_or(CbinError::UnknownTag(kind.tag()))?;
        let mut file = CbinFile {
            header: CbinHeader {
                format_version: 1,
                kind,
                bank_index: bank,
                location_index: loc,
                category_id,
                schema_id,
                checksum: 0,
                reserved: [0; 18],
            },
            payload: self.pack()?,
        };
        file.refresh_checksum();
        Ok(file)
    }

    /// Layers a load of this preset fills: the whole section, or `single` when that is a layer of
    /// the section (an Organ preset always fills both layers).
    pub fn target_layers(&self, single: Option<LayerId>) -> Vec<LayerId> {
        use LayerId::*;
        let all: &[LayerId] = match self {
            Self::Organ(_) => return vec![OrganA, OrganB],
            Self::Piano(_) => &[PianoA, PianoB],
            Self::Synth(_) => &[SynthA, SynthB, SynthC],
        };
        match single {
            Some(id) if all.contains(&id) => vec![id],
            _ => all.to_vec(),
        }
    }

    /// The `.npno` / `.nsmp4` entries of `archive` the preset names (Piano layer `name_hash`,
    /// Synth Sample `sample_id`), exactly as a program resolves them. An Organ preset has none.
    pub fn resolve_sounds(&self, archive: &Ns4BackupArchive) -> ProgramSoundRefs {
        let mut prog = ProgramDecoded::default();
        match self {
            Self::Organ(_) => return ProgramSoundRefs::default(),
            Self::Piano(p) => prog.piano = (**p).clone(),
            Self::Synth(s) => prog.synth = (**s).clone(),
        }
        archive.resolve_program_sounds(&prog)
    }

    /// Encodes against `template` (same kind): header, bank, location and category are kept, and
    /// so is every payload bit the edit did not change (AGENTS.md rule 5).
    ///
    /// The decoder is lossy for some fields (an unknown raw value collapses to an enum variant:
    /// 4 Piano and 136 Synth presets of the factory backup do not re-pack to their own bits).
    /// So the payload is merged at bit level: a bit changes only where the encoding of `self`
    /// differs from the encoding of the preset decoded from `template`. An unedited preset
    /// therefore re-encodes to the identical file; the checksum is refreshed only when a bit
    /// changed.
    pub fn to_file_like(&self, template: &CbinFile) -> Result<CbinFile, CbinError> {
        if template.header.kind != self.section().cbin_kind() {
            return Err(CbinError::UnknownTag(template.header.kind.tag()));
        }
        let base = Self::from_file(template)?.pack()?;
        let new = self.pack()?;
        if base.len() != template.payload.len() || new.len() != base.len() {
            return Err(CbinError::BitstreamError(
                "preset block length changed".into(),
            ));
        }
        let mut file = template.clone();
        let mut changed = false;
        for ((out, b), n) in file.payload.iter_mut().zip(&base).zip(&new) {
            let diff = b ^ n;
            if diff != 0 {
                *out = (*out & !diff) | (n & diff);
                changed = true;
            }
        }
        if changed {
            file.refresh_checksum();
        }
        Ok(file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> [SectionPreset; 3] {
        PresetSection::ALL.map(SectionPreset::init)
    }

    #[test]
    fn init_presets_encode_to_files_of_the_right_kind_size_and_checksum() {
        for p in all() {
            let f = p.to_file(1, 7, 19).unwrap();
            let kind = p.section().cbin_kind();
            assert_eq!(f.to_bytes().len(), kind.expected_file_size().unwrap());
            assert!(f.checksum_ok());
            assert_eq!(f.header.category_id, 19);
            let back = SectionPreset::from_bytes(&f.to_bytes()).unwrap();
            assert_eq!(back.section(), p.section());
            assert_eq!(back.pack().unwrap(), p.pack().unwrap());
        }
    }

    #[test]
    fn like_template_is_byte_identical_unedited_and_refreshes_the_checksum_when_edited() {
        for p in all() {
            let mut template = p.to_file(2, 9, 6).unwrap();
            template.header.reserved = [5; 18];
            template.header.checksum = 0xDEAD_BEEF; // pretend hardware value
            let bytes = template.to_bytes();
            let parsed = CbinFile::parse(&bytes).unwrap();
            let decoded = SectionPreset::from_file(&parsed).unwrap();
            assert_eq!(decoded.to_file_like(&parsed).unwrap().to_bytes(), bytes);
        }
        let template = SectionPreset::init(PresetSection::Organ)
            .to_file(2, 9, 6)
            .unwrap();
        let SectionPreset::Organ(mut o) = SectionPreset::from_file(&template).unwrap() else {
            unreachable!()
        };
        o.drawbars_a = [1, 2, 3, 4, 5, 6, 7, 8, 0];
        let out = SectionPreset::Organ(o).to_file_like(&template).unwrap();
        assert!(out.checksum_ok());
        assert_eq!((out.header.bank_index, out.header.location_index), (2, 9));
        assert_ne!(out.payload, template.payload);
    }

    #[test]
    fn an_edit_changes_only_the_bits_of_the_edited_fields() {
        // Bits the decoder does not map (here: set by hand in the template) survive an edit.
        let mut template = SectionPreset::init(PresetSection::Organ)
            .to_file(0, 0, 0)
            .unwrap();
        let spare = template.payload.len() - 1;
        template.payload[spare] |= 0x01;
        template.refresh_checksum();
        let SectionPreset::Organ(mut o) = SectionPreset::from_file(&template).unwrap() else {
            unreachable!()
        };
        o.layer_a_level = 0.25;
        let out = SectionPreset::Organ(o).to_file_like(&template).unwrap();
        assert_eq!(out.payload[spare], template.payload[spare]);
        assert_ne!(out.payload, template.payload);
        assert!(out.checksum_ok());
    }

    #[test]
    fn sounds_resolve_like_a_program_does() {
        use super::super::backup_writer::more_tests::build_zip;
        use super::super::presets::nord_name_hash;
        let zip = build_zip(&[
            ("Piano/Grand/Astoria Grand Lrg 6.1.npno", b"npno"),
            (
                "Samp Lib/Guitar/Ac Guitar Finger_ST Mart 4.1.nsmp4",
                b"nsmp",
            ),
        ]);
        let archive = Ns4BackupArchive::from_bytes(&zip).unwrap();
        let mut p = PianoPresetDecoded::default();
        p.layer_b.name_hash = nord_name_hash("Astoria Grand Lrg 6.1");
        let refs = SectionPreset::Piano(Box::new(p)).resolve_sounds(&archive);
        assert_eq!(refs.piano_a, None);
        assert_eq!(
            refs.piano_b.as_deref(),
            Some("Piano/Grand/Astoria Grand Lrg 6.1.npno")
        );
        let mut s = SynthPresetDecoded::default();
        s.layer_c.osc_mode = crate::synth::SynthOscMode::Sample;
        s.layer_c.sample_id = nord_name_hash("Ac Guitar Finger_ST Mart 4.1");
        let refs = SectionPreset::Synth(Box::new(s)).resolve_sounds(&archive);
        assert!(refs.synth_c.is_some() && refs.synth_a.is_none() && refs.piano_a.is_none());
        let refs = SectionPreset::init(PresetSection::Organ).resolve_sounds(&archive);
        assert_eq!(refs, ProgramSoundRefs::default());
    }

    #[test]
    fn target_layers_follow_the_section_and_the_single_layer_choice() {
        let o = SectionPreset::init(PresetSection::Organ);
        let p = SectionPreset::init(PresetSection::Piano);
        let s = SectionPreset::init(PresetSection::Synth);
        assert_eq!(o.target_layers(Some(LayerId::PianoA)).len(), 2);
        assert_eq!(p.target_layers(None), [LayerId::PianoA, LayerId::PianoB]);
        assert_eq!(p.target_layers(Some(LayerId::PianoB)), [LayerId::PianoB]);
        assert_eq!(
            p.target_layers(Some(LayerId::SynthA)).len(),
            2,
            "wrong section"
        );
        assert_eq!(s.target_layers(Some(LayerId::SynthC)), [LayerId::SynthC]);
        assert_eq!(s.target_layers(None).len(), 3);
    }

    #[test]
    fn wrong_kinds_are_refused() {
        let organ = SectionPreset::init(PresetSection::Organ);
        let piano = SectionPreset::init(PresetSection::Piano)
            .to_file(0, 0, 0)
            .unwrap();
        assert!(organ.to_file_like(&piano).is_err());
        let mut prog = crate::cbin::ProgramDecoded::default()
            .to_cbin_file(0, 0)
            .unwrap();
        assert!(SectionPreset::from_file(&prog).is_err());
        prog.header.kind = CbinKind::Settings;
        assert!(SectionPreset::from_file(&prog).is_err());
    }
}
