//! Header-preserving encoders for `.ns4p` / `.ns4l` program files.
//!
//! `ProgramDecoded::to_cbin_file` rebuilds the header from scratch (reserved bytes zero). Stored
//! programs written back over an existing file must keep the original 48-byte header, so this
//! module encodes against a template. Spec: `docs/specs/cbin-and-presets.md` (Program write-back).

use super::container::{CbinError, CbinFile, CbinKind};
use super::presets::ProgramDecoded;

/// Payload checksum used by `ProgramDecoded::to_cbin_file`: wrapping byte sum.
/// ASSUMED: matches the hardware sum; the existing encoder and its round-trip test use the same rule.
fn payload_sum(payload: &[u8]) -> u32 {
    payload
        .iter()
        .fold(0u32, |acc, &b| acc.wrapping_add(u32::from(b)))
}

impl ProgramDecoded {
    /// Encodes against `template`, keeping its header (format version, kind, bank, location,
    /// category and the 18 reserved bytes). The checksum is kept when the payload is unchanged,
    /// so an unedited program re-encodes to the identical file.
    pub fn to_cbin_file_like(&self, template: &CbinFile) -> Result<CbinFile, CbinError> {
        let mut file =
            self.to_cbin_file(template.header.bank_index, template.header.location_index)?;
        let mut header = template.header.clone();
        header.category_id = self.category_id;
        header.checksum = if file.payload == template.payload {
            template.header.checksum
        } else {
            payload_sum(&file.payload)
        };
        file.header = header;
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
        assert_eq!(out.header.checksum, payload_sum(&out.payload));
        assert_ne!(out.header.checksum, 0xDEAD_BEEF);
        let back =
            ProgramDecoded::from_cbin_file(&CbinFile::parse(&out.to_bytes()).unwrap()).unwrap();
        assert_eq!(back.global.tempo_bpm, 97.0);
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
