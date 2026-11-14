//! `CBIN` header checksum (`0x18`): CRC-32 over the file bytes from `0x2C` to the end.
//!
//! VERIFIED: for every `.ns4p` (384), `.ns4l` (8), `.ns4o` (64), `.ns4n` (96), `.ns4y` (384) and
//! `.ns4t` (1) file of `Nord Stage 4 Factory Restore RevK v1.66.ns4b` the stored `u32` at `0x18`
//! equals `zlib.crc32(bytes[0x2C..])` (standard CRC-32, reflected `0xEDB88320`, init and final
//! xor `0xFFFFFFFF`), 937 of 937 files, 0 mismatches. Check: `python tools/preset_check.py
//! <backup>` (and `ns4 preset-check <backup>`). The range starts two bytes before the big-endian
//! schema repeat (`0x2E`), so it covers the last two reserved bytes (zero in every file). The
//! earlier rule (wrapping byte sum of the payload) matched none of them.

use super::container::CbinFile;
use super::crc32::crc32;

/// First byte covered by the checksum.
pub const CHECKSUM_START: usize = 0x2C;

impl CbinFile {
    /// The checksum this file should carry (see the module docs).
    pub fn compute_checksum(&self) -> u32 {
        crc32(&self.to_bytes()[CHECKSUM_START..])
    }

    /// The stored checksum matches the file content.
    pub fn checksum_ok(&self) -> bool {
        self.header.checksum == self.compute_checksum()
    }

    /// Writes the checksum the file content needs into the header.
    pub fn refresh_checksum(&mut self) {
        self.header.checksum = self.compute_checksum();
    }
}

#[cfg(test)]
mod tests {
    use super::super::container::{CbinHeader, CbinKind};
    use super::*;

    fn file(payload: Vec<u8>) -> CbinFile {
        CbinFile {
            header: CbinHeader {
                format_version: 1,
                kind: CbinKind::OrganPreset,
                bank_index: 0,
                location_index: 3,
                category_id: 18,
                schema_id: 0x00CD,
                checksum: 0,
                reserved: [0; 18],
            },
            payload,
        }
    }

    #[test]
    fn checksum_is_the_crc_of_the_bytes_from_0x2c_and_tracks_edits() {
        let mut f = file(vec![1, 2, 3, 4]);
        assert!(!f.checksum_ok());
        f.refresh_checksum();
        assert!(f.checksum_ok());
        // bytes 0x2C, 0x2D (reserved zeros), 0x2E 0xCD (schema BE), payload.
        assert_eq!(f.header.checksum, crc32(&[0, 0, 0x00, 0xCD, 1, 2, 3, 4]));
        // header fields before 0x2C (bank, location, category) are outside the range.
        let before = f.header.checksum;
        f.header.location_index = 9;
        assert_eq!(f.compute_checksum(), before);
        f.payload[0] = 9;
        assert!(!f.checksum_ok());
    }
}
