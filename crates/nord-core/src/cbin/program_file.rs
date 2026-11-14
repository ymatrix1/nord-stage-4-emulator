//! Header-preserving encoders for `.ns4p` / `.ns4l` program files.
//!
//! `ProgramDecoded::to_cbin_file` rebuilds the header from scratch (reserved bytes zero). Stored
//! programs written back over an existing file must keep the original 48-byte header, so this
//! module encodes against a template. Spec: `docs/specs/cbin-and-presets.md` (Program write-back).

use super::container::{CbinError, CbinFile, CbinKind};
use super::presets::ProgramDecoded;

impl ProgramDecoded {
    /// Encodes against `template`, keeping its header (format version, kind, bank, location,
    /// category and the 18 reserved bytes). The checksum is kept when the payload is unchanged,
    /// so an unedited program re-encodes to the identical file.
    pub fn to_cbin_file_like(&self, template: &CbinFile) -> Result<CbinFile, CbinError> {
        let mut file =
            self.to_cbin_file(template.header.bank_index, template.header.location_index)?;
        let mut header = template.header.clone();
        header.category_id = self.category_id;
        // An unchanged payload keeps the template checksum (byte-identical file); an edited one
        // gets the CRC-32 of `cbin/checksum.rs` (VERIFIED on the factory backup).
        // The decoders collapse some raw values (OQ31): merge at bit level, so a bit changes only
        // where the encoding of `self` differs from the encoding of the program decoded from the
        // template (AGENTS.md rule 5; same rule as `SectionPreset::to_file_like`).
        let base = Self::from_cbin_file(template)?
            .to_cbin_file(template.header.bank_index, template.header.location_index)?;
        if base.payload.len() == template.payload.len() && file.payload.len() == base.payload.len()
        {
            for ((out, b), n) in file
                .payload
                .iter_mut()
                .zip(&base.payload)
                .zip(&template.payload)
            {
                let diff = *out ^ b;
                *out = (n & !diff) | (*out & diff);
            }
        }
        let unchanged = file.payload == template.payload;
        file.header = header;
        if !unchanged {
            file.refresh_checksum();
        }
        Ok(file)
    }

    /// Encodes as a Live slot file (`.ns4l`). ASSUMED: bank index 0 and location index = slot,
    /// because no real `.ns4l` header was inspected; confirm against a Live backup entry.
    pub fn to_live_cbin_file(&self, slot: u16) -> Result<CbinFile, CbinError> {
        let mut file = self.to_cbin_file(0, slot)?;
        file.header.kind = CbinKind::Live;
        Ok(file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_template_is_byte_identical_when_unedited_and_updates_checksum_when_edited() {
        let prog = ProgramDecoded::default();
        let mut template = prog.to_cbin_file(3, 17).unwrap();
        template.header.reserved = [7u8; 18];
        template.header.checksum = 0xDEAD_BEEF; // pretend hardware value
        let bytes = template.to_bytes();
        let parsed = CbinFile::parse(&bytes).unwrap();
        let decoded = ProgramDecoded::from_cbin_file(&parsed).unwrap();
        assert_eq!(
            decoded.to_cbin_file_like(&parsed).unwrap().to_bytes(),
            bytes
        );

        let mut edited = decoded;
        edited.global.tempo_bpm = 97.0;
        let out = edited.to_cbin_file_like(&parsed).unwrap();
        assert_eq!(out.header.reserved, [7u8; 18]);
        assert_eq!(out.header.bank_index, 3);
        assert!(out.checksum_ok());
        assert_ne!(out.header.checksum, 0xDEAD_BEEF);
        let back =
            ProgramDecoded::from_cbin_file(&CbinFile::parse(&out.to_bytes()).unwrap()).unwrap();
        assert_eq!(back.global.tempo_bpm, 97.0);
    }

    #[test]
    fn bits_the_decoder_collapses_survive_an_edit() {
        let blank = ProgramDecoded::default().to_cbin_file(1, 2).unwrap();
        // Find a payload bit that decode + pack does not return (OQ31: an unknown raw value).
        let lossy = (0..blank.payload.len() * 8).find_map(|bit| {
            let mut t = blank.clone();
            t.payload[bit / 8] ^= 0x80 >> (bit % 8);
            let back = ProgramDecoded::from_cbin_file(&t).ok()?;
            let again = back.to_cbin_file(1, 2).ok()?;
            (again.payload != t.payload).then_some((bit, t))
        });
        let (bit, mut template) = lossy.expect("the program decoder collapses some bit");
        template.refresh_checksum();
        let decoded = ProgramDecoded::from_cbin_file(&template).unwrap();
        let same = decoded.to_cbin_file_like(&template).unwrap();
        assert_eq!(same.to_bytes(), template.to_bytes(), "unedited, bit {bit}");
        let mut edited = decoded;
        edited.global.tempo_bpm = 99.0;
        let out = edited.to_cbin_file_like(&template).unwrap();
        let kept = out.payload[bit / 8] & (0x80 >> (bit % 8));
        assert_eq!(kept, template.payload[bit / 8] & (0x80 >> (bit % 8)));
        assert_ne!(out.payload, template.payload);
        assert!(out.checksum_ok());
    }

    #[test]
    fn live_file_has_live_kind_and_decodes_like_a_program() {
        let file = ProgramDecoded::default().to_live_cbin_file(5).unwrap();
        let parsed = CbinFile::parse(&file.to_bytes()).unwrap();
        assert_eq!(parsed.header.kind, CbinKind::Live);
        assert_eq!(parsed.header.location_index, 5);
        assert_eq!(
            ProgramDecoded::from_cbin_file(&parsed)
                .unwrap()
                .global
                .tempo_bpm,
            120.0
        );
    }
}
