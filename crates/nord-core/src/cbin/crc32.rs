//! CRC-32 (IEEE 802.3, reflected, polynomial `0xEDB88320`) as PKZIP stores it.
//!
//! Needed by the `.ns4b` writer (`backup_writer.rs`) for entries it replaces or adds.

const fn make_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}

/// VERIFIED: standard CRC-32 table (poly `0xEDB88320`), checked by `check_value` against the
/// published check value `0xCBF43926` of `"123456789"`. Against the Factory Restore archive the
/// CRC of every rewritten entry equals the stored one when the data is unchanged (see
/// `docs/specs/cbin-and-presets.md` section 5.5).
static TABLE: [u32; 256] = make_table();

/// CRC-32 of `data`.
pub fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = TABLE[((c ^ u32::from(b)) & 0xFF) as usize] ^ (c >> 8);
    }
    !c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
        assert_ne!(crc32(b"a"), crc32(b"b"));
    }
}
