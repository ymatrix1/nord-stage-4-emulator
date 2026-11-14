//! Vox key-down map: which oscillator rows a held key drives, and with what weight.
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1.7. Research: `docs/research/keydown-zones.md`
//! section 5 (captured Vox key-down flags and weight messages, 24 keys 21..108, table
//! `docs/research/data/keydown-vox-flags.csv`). A key-down sets one flag per footage slot `s` at
//! row `fold(key - 36 + SLOT_OFFSETS[s])` and sends `DM(0x95E46 + row) = sum of the level slots of
//! the flagged slots`; there is no velocity (VERIFIED negative, `keydown-zones.md` section 4).

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

/// Rows of the 96-word block: 84 note oscillators plus the 12 buckets of the 36-sine bank.
pub const ROWS: usize = 96;

/// Level of every footage slot in the default program: `SLOT_LEVEL` on [`DEFAULT_SLOTS`], else 0.
pub(super) fn default_levels() -> [f32; 20] {
    let mut l = [0.0; 20];
    for &s in &DEFAULT_SLOTS {
        l[s] = SLOT_LEVEL;
    }
    l
}

/// Row of footage slot `slot` for `key`: `fold(key - 36 + SLOT_OFFSETS[slot])`, folding by octaves
/// into `0..96`.
pub(super) fn slot_row(key: i32, slot: usize) -> usize {
    let mut r = key - 36 + SLOT_OFFSETS[slot];
    while r < 0 {
        r += 12;
    }
    while r >= ROWS as i32 {
        r -= 12;
    }
    r as usize
}

/// Adds the weights of `key` to `targets`: every slot with a non-zero level adds its level to its
/// row (rows 84..95 are the sine buckets).
pub(super) fn add_key(targets: &mut [f32; ROWS], key: i32, levels: &[f32; 20]) {
    for (slot, &level) in levels.iter().enumerate().filter(|(_, &l)| l != 0.0) {
        targets[slot_row(key, slot)] += level;
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
        let levels = default_levels();
        assert_eq!(slot_row(60, 2), 36);
        let mut t = [0.0f32; ROWS];
        add_key(&mut t, 60, &levels); // rows 24, 36, 48, 60
        assert_eq!(t.iter().filter(|&&x| x != 0.0).count(), 4);
        assert_eq!(t[36], SLOT_LEVEL);
        let mut u = [0.0f32; ROWS];
        add_key(&mut u, 21, &levels);
        assert!((u.iter().sum::<f32>() - 4.0 * SLOT_LEVEL).abs() < 1e-6);
        // C4 and its octave share rows: 16' of C5 is 8' of C4 (row 36)
        let mut c = [0.0f32; ROWS];
        add_key(&mut c, 60, &levels);
        add_key(&mut c, 72, &levels);
        assert!((c[36] - 2.0 * SLOT_LEVEL).abs() < 1e-7);
        assert!((SLOT_LEVEL - 0.247_499_94).abs() < 1e-7);
    }

    #[test]
    fn high_keys_reach_the_sine_bucket_rows_and_every_slot_adds_its_level() {
        let levels = default_levels();
        let mut t = [0.0f32; ROWS];
        add_key(&mut t, 90, &levels); // 2' lands on row 90, 4' on 78, 8' on 66, 16' on 54
        assert_eq!(slot_row(90, 10), 90);
        assert_eq!(t[90], SLOT_LEVEL);
        assert!(
            (t.iter().sum::<f32>() - 4.0 * SLOT_LEVEL).abs() < 1e-6,
            "no row is dropped"
        );
        let mut one = [0.0f32; 20];
        one[19] = 0.5; // the 1/2' slot (offset 60)
        let mut u = [0.0f32; ROWS];
        add_key(&mut u, 60, &one);
        assert_eq!(u[84], 0.5);
    }
}
