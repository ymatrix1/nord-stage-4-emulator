//! Pure step-order helpers of the arpeggiator: ZIG ZAG and fractional RANGE (manual p.35-36).
//!
//! Sources: Nord Stage 4 User Manual OS v1.6x p.35 (RANGE "values between each octave ... 2
//! octaves and a fifth"), p.36 ("Zig Zag ... jump by two steps and then back one"); the Range
//! knob moves "in steps of 1/3 of an octave" (Synth tutorial chapter of the same manual).

/// Semitones per range step of 1/3 octave.
/// ASSUMED: 1/3 octave = 4 semitones (a major third); the manual's "2 octaves and a fifth"
/// example does not fit 4-semitone steps exactly. Confirm by arpeggiating one note on hardware at
/// each Range detent and reading the highest pitch.
pub const SEMITONES_PER_RANGE_THIRD: u8 = 4;

/// Largest fractional range in thirds on top of the whole octaves (`0..=2`).
/// ASSUMED: three detents per octave means two between whole octaves.
pub const MAX_RANGE_EXTRA_THIRDS: u8 = 2;

/// ZIG ZAG position for the `k`-th step over `total` positions: `0, 2, 1, 3, 2, 4, 3, ...`
/// (two forward, one back), wrapped at `total`. Period `2 * total`.
pub fn zigzag_index(k: usize, total: usize) -> usize {
    let total = total.max(1);
    let k = k % (2 * total);
    let pos = if k.is_multiple_of(2) {
        k / 2
    } else {
        k / 2 + 2
    };
    pos % total
}

/// Number of held notes (ascending `sorted`) that also play in the partial top octave: those at
/// most `thirds * 4` semitones above the lowest held note.
/// ASSUMED: the partial octave repeats the lowest chord notes; confirm on hardware with a chord.
pub fn extra_notes(sorted: &[u8], thirds: u8) -> usize {
    let thirds = thirds.min(MAX_RANGE_EXTRA_THIRDS);
    if thirds == 0 || sorted.is_empty() {
        return 0;
    }
    let limit = u16::from(thirds) * u16::from(SEMITONES_PER_RANGE_THIRD);
    let lowest = u16::from(sorted[0]);
    sorted
        .iter()
        .take_while(|&&n| u16::from(n) - lowest <= limit)
        .count()
}

/// Maps an ascending pool position to `(chord note index, octave offset)`. The pool is `count`
/// notes for each of `oct_range` octaves, then the partial octave at offset `oct_range`.
pub fn pool_position(pos: usize, count: usize) -> (usize, u8) {
    let count = count.max(1);
    (pos % count, (pos / count).min(usize::from(u8::MAX)) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zigzag_goes_two_forward_one_back() {
        let seq: Vec<usize> = (0..8).map(|k| zigzag_index(k, 100)).collect();
        assert_eq!(seq, [0, 2, 1, 3, 2, 4, 3, 5]);
    }

    #[test]
    fn zigzag_visits_every_position_over_one_period() {
        for total in 1..9 {
            let mut seen = vec![false; total];
            for k in 0..2 * total {
                seen[zigzag_index(k, total)] = true;
            }
            assert!(seen.iter().all(|&s| s), "total {total}");
        }
    }

    #[test]
    fn extra_notes_follow_the_fraction() {
        let chord = [60u8, 64, 67, 72];
        assert_eq!(extra_notes(&chord, 0), 0);
        assert_eq!(extra_notes(&chord, 1), 2); // 60, 64 within 4 semitones
        assert_eq!(extra_notes(&chord, 2), 3); // 67 is 7 above, limit 8
        assert_eq!(extra_notes(&[], 2), 0);
    }

    #[test]
    fn pool_position_maps_octaves() {
        assert_eq!(pool_position(0, 3), (0, 0));
        assert_eq!(pool_position(4, 3), (1, 1));
        assert_eq!(pool_position(6, 3), (0, 2));
    }
}
