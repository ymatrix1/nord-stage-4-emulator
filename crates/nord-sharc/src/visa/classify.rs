//! Parcel level: instruction type and length, tiling a stream, and field extraction.
use super::types::{Insn, Kind};

/// Type and length (in 16-bit parcels) of the instruction starting at `p0`.
///
/// `p1` is the following parcel, needed to recognise 32-bit forms (low 7 bits `0x3F`).
pub fn classify(p0: u16, p1: Option<u16>) -> (Kind, usize) {
    classify_with(p0, p1, true)
}

/// Type of a fixed 48-bit ISA-mode word (no 16/32-bit forms: `1100`/`1001` prefixes are Types
/// 10a and 6a there). ISA-mode code is the SY1SY2/SMP boot kernel (`docs/research/sharc-sy1sy2-smp.md`).
pub fn classify_isa(p0: u16) -> Kind {
    classify_with(p0, None, false).0
}

fn classify_with(p0: u16, p1: Option<u16>, visa: bool) -> (Kind, usize) {
    use Kind::*;
    let marker = visa && p1.is_some_and(|p| p & 0x7F == 0x3F);
    let top3 = p0 >> 13;
    let top4 = p0 >> 12;
    let sub = (p0 >> 2) & 7;
    if visa && top4 == 0b1100 {
        return (T2c, 1);
    }
    if visa && top4 == 0b1001 {
        if sub & 4 != 0 {
            return (T3c, 1);
        }
        return match sub {
            2 => (T15b, 2),
            1 => (T16b, 2),
            _ => (T16a, 3),
        };
    }
    match top3 {
        0b001 => return if marker { (T1b, 2) } else { (T1a, 3) },
        0b010 => return if marker { (T3b, 2) } else { (T3a, 3) },
        0b011 => {
            let five = (p0 >> 12) & 1 == 1;
            return match (five, marker) {
                (false, true) => (T4b, 2),
                (false, false) => (T4a, 3),
                (true, true) => (T5b, 2),
                (true, false) => (T5a, 3),
            };
        }
        0b100 => return (T6a, 3),
        0b101 => return (T15a, 3),
        0b110 | 0b111 => return (T10a, 3),
        _ => {}
    }
    if matches!(p0 >> 8, 0x18 | 0x19) {
        return (T25, 3);
    }
    let code = (p0 >> 8) & 0x1F;
    let short = visa && (p0 >> 7) & 1 == 1;
    match code {
        0x01 => {
            if short {
                (T2b, 2)
            } else {
                (T2a, 3)
            }
        }
        0x02 => (T6n, 3),
        0x04 => {
            if marker {
                (T7b, 2)
            } else {
                (T7a, 3)
            }
        }
        0x06 | 0x07 => (T8a, 3),
        0x08 | 0x09 => {
            if marker {
                (T9b, 2)
            } else {
                (T9a, 3)
            }
        }
        0x0A | 0x0B => {
            if short {
                (T11c, 1)
            } else {
                (T11a, 3)
            }
        }
        0x0C | 0x0D => (T12a, 3),
        0x0E => (T13a, 3),
        0x0F => {
            if short {
                (T17b, 2)
            } else {
                (T17a, 3)
            }
        }
        0x14 => (T18a, 3),
        0x16 => (T19a, 3),
        0x17 => (T20a, 3),
        0x10..=0x13 => (T14a, 3),
        0x00 => {
            if visa && p0 & 1 == 1 {
                (T21c, 1)
            } else {
                (T21, 3)
            }
        }
        _ => (Unknown, 3),
    }
}

/// Splits parcels into instructions. The flag says the last instruction ends exactly at the end.
pub fn tile(parcels: &[u16]) -> (Vec<Insn>, bool) {
    let mut out = Vec::new();
    let mut i = 0;
    while i < parcels.len() {
        let (kind, len) = classify(parcels[i], parcels.get(i + 1).copied());
        let end = (i + len).min(parcels.len());
        let mut p = [0u16; 3];
        p[..end - i].copy_from_slice(&parcels[i..end]);
        out.push(Insn {
            kind,
            parcels: p,
            len: end - i,
        });
        i += len;
    }
    (out, i == parcels.len())
}

/// Little-endian 16-bit parcels of `data` (an odd trailing byte is dropped).
pub fn parcels_le(data: &[u8]) -> Vec<u16> {
    (0..data.len() / 2)
        .map(|i| u16::from_le_bytes([data[2 * i], data[2 * i + 1]]))
        .collect()
}

/// MSB-aligned 48-bit value of an instruction (shorter ones are left-aligned).
pub fn word48(parcels: &[u16]) -> u64 {
    let mut w: u64 = 0;
    for p in parcels {
        w = (w << 16) | u64::from(*p);
    }
    w << (48 - 16 * parcels.len())
}

/// Bits `hi..=lo` of a 48-bit instruction word.
pub fn bits(w: u64, hi: u32, lo: u32) -> u64 {
    (w >> lo) & ((1u64 << (hi - lo + 1)) - 1)
}

/// `(ureg code, immediate)` of a Type 17a/17b instruction.
pub fn ureg_load(insn: &Insn) -> Option<(u8, u32)> {
    let p = insn.words();
    match insn.kind {
        Kind::T17b if p.len() >= 2 => Some(((p[0] & 0x7F) as u8, u32::from(p[1]))),
        Kind::T17a if p.len() >= 3 => Some((
            (p[0] & 0x7F) as u8,
            (u32::from(p[1]) << 16) | u32::from(p[2]),
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_known_forms() {
        assert_eq!(classify(0x721A, Some(0x003F)), (Kind::T5b, 2));
        assert_eq!(classify(0x1901, None), (Kind::T25, 3));
        assert_eq!(classify(0xC123, None), (Kind::T2c, 1));
        assert_eq!(classify(0x0FA4, Some(0x006C)), (Kind::T17b, 2));
        assert_eq!(classify(0x0F15, Some(0x000C)), (Kind::T17a, 3));
        assert_eq!(classify(0x1486, Some(0x0000)), (Kind::T18a, 3));
        assert_eq!(classify(0x023E, Some(0x2019)), (Kind::T6n, 3));
        assert_eq!(classify(0x0001, Some(0x06BE)), (Kind::T21c, 1));
        assert_eq!(classify(0x0000, Some(0x0000)), (Kind::T21, 3));
    }

    #[test]
    fn isa_words_are_always_48_bit() {
        // `lcntr=r2, do` and `r1=0x1420`-like words share prefixes with VISA short forms
        assert_eq!(classify_isa(0x0c00), Kind::T12a);
        assert_eq!(classify_isa(0x1000), Kind::T14a);
        assert_eq!(classify_isa(0x0001), Kind::T21, "no 16-bit NOP in ISA mode");
        assert_eq!(classify_isa(0xC123), Kind::T10a, "1100 is Type 10a, not 2c");
        assert_eq!(
            classify_isa(0x9000),
            Kind::T6a,
            "1001 is Type 6a, not 3c/15b/16"
        );
        assert_eq!(
            classify_isa(0x0f81),
            Kind::T17a,
            "short bit 7 is not a length flag"
        );
        assert_eq!(classify_isa(0x013e), Kind::T2a);
    }

    #[test]
    fn bit_fields_and_word_assembly() {
        assert_eq!(bits(0xABCD_0000_0000, 47, 40), 0xAB);
        assert_eq!(word48(&[0x1486, 0, 0x20]), 0x1486_0000_0020);
        assert_eq!(word48(&[0xFA4, 0x70]), 0x0FA4_0070_0000);
    }

    #[test]
    fn tile_splits_and_reports_exactness() {
        // 17a (3 parcels) + 17b (2 parcels)
        let p = [0x0F15, 0x000C, 0x0440, 0x0FA4, 0x0070];
        let (insns, exact) = tile(&p);
        assert!(exact && insns.len() == 2);
        assert_eq!(insns[0].kind, Kind::T17a);
        assert_eq!(insns[1].words(), &[0x0FA4, 0x0070]);
        let (_, exact) = tile(&p[..4]);
        assert!(!exact);
        assert_eq!(ureg_load(&insns[0]), Some((0x15, 0x000C_0440)));
        assert_eq!(ureg_load(&insns[1]), Some((0x24, 0x70)));
    }
}
