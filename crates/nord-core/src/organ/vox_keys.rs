//! Vox key-down map: which oscillator rows a held key drives, and with what weight.
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1.7. Research: `docs/research/keydown-zones.md`
//! section 5 (captured Vox key-down flags and weight messages, 24 keys 21..108, table
//! `docs/research/data/keydown-vox-flags.csv`). A key-down sets one flag per footage slot `s` at
//! row `fold(key - 36 + SLOT_OFFSETS[s])` and sends `DM(0x95E46 + row) = sum of the level slots of
//! the flagged slots`; there is no velocity (VERIFIED negative, `keydown-zones.md` section 4).

use super::vox_bank::NOTES;

/// Semitone offset of each of the 20 footage slots (slot 2, the 8', lands on row `key - 24`).
///
/// VERIFIED: all 20 flags of each of 24 captured keys match (`keydown-vox-flags.csv`,
/// `tests::flag_table_matches_the_capture`).
pub(super) const SLOT_OFFSETS: [i32; 20] = [
    0, 7, 12, 19, 19, 24, 24, 28, 31, 31, 36, 36, 36, 40, 40, 43, 48, 48, 52, 60,
];

/// Slots with a non-zero level in the default program: 16', 8', 4', 2' (offsets 0, 12, 24, 36).
///
/// VERIFIED: the level slots read after boot (`keydown-zones.md` section 5, "Level slots in the
/// default program"); the other 16 slots hold 0. ASSUMED: they follow no edit id in the Rust
/// engine (the drawbars do not act on the Vox source, the ARM mapping of the Vox footage edits to
/// level slots was not captured).
pub(super) const DEFAULT_SLOTS: [usize; 4] = [0, 2, 5, 10];

/// Level of one default slot, `2076180 / 2^23` (0.2475).
///
/// VERIFIED: the weight message of a key row with one flagged slot is `0x3E7D70A0` = 0.24749994
/// (`keydown-organ-keys.csv`, model 1, 24 notes); two folded slots send `0x3EFD70A0` (twice).
pub(super) const SLOT_LEVEL: f32 = 2_076_180.0 / 8_388_608.0;

/// Rows of the key tables: 84 note oscillators plus the 12 buckets of the unported 36-sine bank.
const ROWS: i32 = 96;

/// Row of footage slot `slot` for `key`: `fold(key - 36 + SLOT_OFFSETS[slot])`, folding by octaves
/// into `0..96`.
pub(super) fn slot_row(key: i32, slot: usize) -> usize {
    let mut r = key - 36 + SLOT_OFFSETS[slot];
    while r < 0 {
        r += 12;
    }
    while r >= ROWS {
        r -= 12;
    }
    r as usize
}

/// Adds the default-slot weights of `key` to `targets`; rows 84..95 (the unported sine bank) get
/// no oscillator and are dropped.
pub(super) fn add_key(targets: &mut [f32; NOTES], key: i32) {
    for &s in &DEFAULT_SLOTS {
        if let Some(t) = targets.get_mut(slot_row(key, s)) {
            *t += SLOT_LEVEL;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_table_matches_the_capture() {
        let csv = include_str!("../../../../docs/research/data/keydown-vox-flags.csv");
        let mut rows = 0;
        for line in csv.lines().skip(1) {
            let f: Vec<usize> = line.split(',').map(|v| v.parse().unwrap()).collect();
            assert_eq!(
                slot_row(f[0] as i32, f[2]),
                f[1],
                "note {} slot {}",
                f[0],
                f[2]
            );
            rows += 1;
        }
        assert_eq!(rows, 24 * 20);
    }

    #[test]
    fn the_8_foot_slot_lands_on_note_minus_24_and_levels_add_when_rows_fold() {
        assert_eq!(slot_row(60, 2), 36);
        let mut t = [0.0f32; NOTES];
        add_key(&mut t, 60); // rows 24, 36, 48, 60
        assert_eq!(t.iter().filter(|&&x| x != 0.0).count(), 4);
        assert_eq!(t[36], SLOT_LEVEL);
        // note 21: slots 0 and 2 fold onto rows 9 and 21? (rows 9, 21, 33, 45): all distinct here
        let mut u = [0.0f32; NOTES];
        add_key(&mut u, 21);
        assert!((u.iter().sum::<f32>() - 4.0 * SLOT_LEVEL).abs() < 1e-6);
        // C4 and its octave share rows: 16' of C5 is 8' of C4 (row 36)
        let mut c = [0.0f32; NOTES];
        add_key(&mut c, 60);
        add_key(&mut c, 72);
        assert!((c[36] - 2.0 * SLOT_LEVEL).abs() < 1e-7);
        assert!((SLOT_LEVEL - 0.247_499_94).abs() < 1e-7);
    }

    #[test]
    fn high_rows_belong_to_the_unported_sine_bank() {
        let mut t = [0.0f32; NOTES];
        add_key(&mut t, 108); // rows 72, 84 (dropped), 96 -> 84 (dropped), 108 -> 72 ...
        assert!(slot_row(108, 2) >= 84);
        assert!(t.iter().sum::<f32>() < 4.0 * SLOT_LEVEL);
    }
}
