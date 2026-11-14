//! Velocity position and stroke selection of the hardware piano (opt-in, not in the default render).
//!
//! Evidence: `docs/research/piano-sound-load.md` section 8 (firmware run of `tools/arm_piano_load.py`
//! plus disassembly of `0xC00D2024..0xC00D2088`, `0xC00AA6A4..0xC00AA6BC`, `0xC00A2A54`). A MIDI
//! velocity `v` (1..=127) goes through two firmware tables before the level law of
//! [`super::velocity_level`] applies:
//!
//! 1. `r8 = s16[v + 768]` of the firmware velocity-curve table (`0xC02B3140`, 8 curves of 128
//!    entries; curve 6 is the one in force in the default program: `r8(1) = 17`, `r8(64) = 203`,
//!    `r8(127) = 656`).
//! 2. `T = byte[r8]` of velocity curve 11 (`0xC0532664 + 11 * 1024`): the velocity position, 200 at
//!    `v = 1`, 78 at `v = 64`, 0 from `v = 121`.
//!
//! [`T_BY_VELOCITY`] is the composition `T(v)` as a derived list (127 bytes; the two firmware
//! tables themselves are not copied). `X = clamp(d + (((128 - c) * T) >> 7), 0, 249)` with
//! `(c, d) = (0x13, 0)` reproduces all 79 swept `X` values of [`super::velocity_level::X_SWEEP`]
//! exactly. The stroke record of a key is the one with the largest threshold not above `T` in the
//! key's `(record, threshold)` list ([`select_stroke`]) and its level offset is the big-endian
//! halfword at `+0x34` of the 118-byte `.npno` stroke record ([`record_level_offset`], VERIFIED on
//! 38 of 38 Clavinet D6 records against the in-memory records).

/// `T(v)` for `v = 1..=127` (index `v - 1`). VERIFIED: composed from the two firmware tables above.
pub const T_BY_VELOCITY: [u8; 127] = [
    200, 200, 196, 194, 191, 189, 186, 184, 181, 178, 177, 174, 172, 169, 168, 165, 163, 161, 159,
    157, 155, 153, 151, 148, 146, 144, 143, 140, 139, 137, 135, 132, 131, 129, 127, 125, 123, 122,
    120, 118, 116, 114, 113, 111, 109, 107, 106, 104, 102, 101, 99, 97, 96, 94, 92, 91, 89, 87, 86,
    84, 83, 81, 80, 78, 77, 75, 74, 72, 71, 69, 68, 66, 65, 63, 62, 60, 59, 57, 56, 54, 53, 51, 50,
    49, 47, 46, 45, 43, 42, 40, 39, 38, 36, 35, 34, 32, 31, 30, 28, 27, 26, 24, 23, 22, 20, 19, 18,
    17, 15, 14, 13, 11, 10, 9, 8, 7, 5, 4, 3, 2, 0, 0, 0, 0, 0, 0, 0,
];

/// Slope byte `c` of the zone used by the default program (`0xC02A1208 + 0xB80 + 2 * zone`).
pub const ZONE_C: i32 = 0x13;
/// Offset byte `d` of the same zone.
pub const ZONE_D: i32 = 0;
/// Upper clamp of `X` (`0xF9`).
pub const X_MAX: i32 = 0xF9;
/// Offset of the level halfword inside a 118-byte `.npno` stroke record.
pub const RECORD_LEVEL_OFFSET: usize = 0x34;

/// Velocity position `T` of a MIDI velocity (clamped to `1..=127`).
pub fn velocity_position(velocity: u8) -> u8 {
    T_BY_VELOCITY[usize::from(velocity.clamp(1, 127)) - 1]
}

/// `X` from the curve tables: `clamp(d + (((128 - c) * T) >> 7), 0, 249)`.
pub fn x_from_curve(velocity: u8) -> i16 {
    let t = i32::from(velocity_position(velocity));
    let x = (ZONE_D + (((128 - ZONE_C) * t) >> 7)).clamp(0, X_MAX);
    i16::try_from(x).unwrap_or(0)
}

/// Level offset `rec[0x5E]` of a stroke: the big-endian halfword at `+0x34` of the `.npno` stroke
/// record (`None` when the slice is too short).
pub fn record_level_offset(record: &[u8]) -> Option<i16> {
    let bytes = record.get(RECORD_LEVEL_OFFSET..RECORD_LEVEL_OFFSET + 2)?;
    Some(i16::from_be_bytes([bytes[0], bytes[1]]))
}

/// Record of a key's `(record index, threshold)` list for velocity position `position`: the entry
/// with the largest threshold not above it (the first entry when none qualifies). VERIFIED for the
/// Clavinet D6 note 60 list against 65 firmware runs; the thresholds are firmware (loader) values,
/// not derived from the file (they run about 8 below the file's halfword at `+4`, see the note).
pub fn select_stroke(list: &[(u16, u16)], position: u8) -> Option<u16> {
    let mut chosen = list.first()?.0;
    for &(record, threshold) in list {
        if threshold <= u16::from(position) {
            chosen = record;
        }
    }
    Some(chosen)
}

#[cfg(test)]
mod tests {
    use super::super::velocity_level::{stroke_at, CLAVINET_D6_STROKES, X_SWEEP};
    use super::*;

    #[test]
    fn curve_tables_reproduce_every_swept_x() {
        for &(velocity, x) in &X_SWEEP {
            assert_eq!(x_from_curve(velocity), x, "velocity {velocity}");
        }
    }

    #[test]
    fn velocity_position_anchors() {
        assert_eq!(velocity_position(1), 200);
        assert_eq!(velocity_position(64), 78);
        assert_eq!(velocity_position(127), 0);
        assert_eq!(velocity_position(0), 200, "clamped to 1");
        assert_eq!(velocity_position(200), 0, "clamped to 127");
    }

    #[test]
    fn position_is_non_increasing() {
        assert!(T_BY_VELOCITY.windows(2).all(|w| w[0] >= w[1]));
    }

    #[test]
    fn record_offset_is_the_big_endian_halfword_at_0x34() {
        let mut record = [0u8; 118];
        record[0x34] = 0x00;
        record[0x35] = 0x4B;
        assert_eq!(record_level_offset(&record), Some(75));
        record[0x34] = 0xFF;
        record[0x35] = 0xF6;
        assert_eq!(record_level_offset(&record), Some(-10));
        assert_eq!(record_level_offset(&record[..0x35]), None);
    }

    /// Clavinet D6, root key 60: list `(21, 0) (22, 16) (23, 46)`, record offsets 66 / 56 / 36.
    #[test]
    fn clavinet_selection_matches_the_swept_strokes() {
        let list = [(21, 0), (22, 16), (23, 46)];
        let offset = |record: u16| match record {
            21 => 66,
            22 => 56,
            _ => 36,
        };
        for &(velocity, _) in &X_SWEEP {
            let record = select_stroke(&list, velocity_position(velocity)).unwrap();
            assert_eq!(
                offset(record),
                stroke_at(&CLAVINET_D6_STROKES, velocity),
                "velocity {velocity}"
            );
        }
    }

    #[test]
    fn selection_edge_cases() {
        assert_eq!(select_stroke(&[], 10), None);
        assert_eq!(select_stroke(&[(5, 20), (6, 40)], 3), Some(5));
        assert_eq!(select_stroke(&[(5, 20), (6, 40)], 40), Some(6));
    }
}
