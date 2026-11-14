//! Export of presets stored with STORE: the preset file of a stored section.
//!
//! The file is the panel snapshot patched onto the archive preset it was based on, encoded
//! against that file (`SectionPreset::to_file_like`: header, category and every bit the panel
//! did not change stay as they were). Without a base the file is built from the init preset.
//! See `docs/specs/cbin-and-presets.md` section 5.6.

use crate::panel::bay_program::preset_user::UserPreset;
use nord_core::cbin::{CbinFile, PresetPut, SectionPreset};

/// The put that places stored preset `u` at `(bank, loc)`. `read` returns archive entry bytes.
pub fn user_put(
    u: &UserPreset,
    bank: u16,
    loc: u16,
    read: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
) -> Result<PresetPut, String> {
    let template = match &u.base {
        Some(b) => {
            let file = CbinFile::parse(&read(b)?).map_err(|e| format!("{b}: {e}"))?;
            (file.header.kind == u.section.cbin_kind()).then_some(file)
        }
        None => None,
    };
    let base = template
        .as_ref()
        .map(SectionPreset::from_file)
        .transpose()
        .map_err(|e| e.to_string())?;
    let preset = u.state.capture_section_preset(u.section, base.as_ref());
    let file = match &template {
        Some(t) => preset.to_file_like(t),
        None => preset.to_file(bank, loc, 0),
    }
    .map_err(|e| e.to_string())?;
    PresetPut::from_file(u.section, bank, loc, &u.name, &file).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::cbin::PresetSection;
    use nord_core::engine::Stage4Engine;

    fn user(base: Option<&str>) -> UserPreset {
        let mut e = Stage4Engine::new(48_000.0);
        e.organ_a.drawbars = [8, 0, 8, 0, 8, 0, 8, 0, 8];
        UserPreset {
            name: "Mine".into(),
            section: PresetSection::Organ,
            state: e.capture_state(),
            base: base.map(String::from),
        }
    }

    #[test]
    fn a_stored_preset_without_a_base_is_a_fresh_file_at_the_location() {
        let put = user_put(&user(None), 1, 7, &mut |_| Err("unused".into())).unwrap();
        let f = CbinFile::parse(&put.bytes).unwrap();
        assert_eq!((f.header.bank_index, f.header.location_index), (1, 7));
        assert!(f.checksum_ok());
        let SectionPreset::Organ(o) = SectionPreset::from_file(&f).unwrap() else {
            panic!("organ expected")
        };
        assert_eq!(o.drawbars_a, [8, 0, 8, 0, 8, 0, 8, 0, 8]);
        assert_eq!(put.stem, "Mine");
    }

    #[test]
    fn a_base_keeps_its_header_category_and_untouched_bits() {
        let mut template = SectionPreset::init(PresetSection::Organ)
            .to_file(0, 3, 19)
            .unwrap();
        let spare = template.payload.len() - 1;
        template.payload[spare] |= 1;
        template.refresh_checksum();
        let bytes = template.to_bytes();
        let put = user_put(&user(Some("Organ Preset/Bank 1/B.ns4o")), 1, 7, &mut |n| {
            assert_eq!(n, "Organ Preset/Bank 1/B.ns4o");
            Ok(bytes.clone())
        })
        .unwrap();
        let f = CbinFile::parse(&put.bytes).unwrap();
        assert_eq!(f.header.category_id, 19);
        assert_eq!(f.header.location_index, 7, "the put sets the location");
        assert_eq!(f.payload[spare] & 1, 1, "unmapped bit survives");
        assert!(f.checksum_ok());
    }

    #[test]
    fn an_unreadable_or_wrong_kind_base_is_reported_or_ignored() {
        assert!(user_put(&user(Some("x.ns4o")), 0, 0, &mut |_| Err("gone".into())).is_err());
        let piano = SectionPreset::init(PresetSection::Piano)
            .to_file(0, 0, 0)
            .unwrap()
            .to_bytes();
        let put = user_put(&user(Some("x.ns4n")), 0, 0, &mut |_| Ok(piano.clone())).unwrap();
        assert!(CbinFile::parse(&put.bytes).is_ok());
    }
}
