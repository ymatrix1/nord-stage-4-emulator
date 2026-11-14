//! Generator weights of the B3 Bass (model 5): `DM 0x95122 + (note - 36)`, the factor of the ring
//! `ring = wheel x weight`.
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1.9. Research: `docs/research/organ-farfisa-bass.md`
//! s12, tool `tools/organ_bass_gen.py`. The ARM key-down writes one weight word per footage item
//! (drawbar items and percussion items) and wheel. The value is a pure function of the wheel and of
//! the number `n` of earlier writes to that wheel that are still held:
//!
//! `weight(note, n) = T[note][n]`, `T[note][0]` = the first-write amplitude `A` ([`super::b3_keys`]).
//!
//! A key-up removes the writes of the key one by one and leaves `T[note][held - 1]` (`T[note][0]`
//! when no other key holds the wheel, the window weights are then zero).

const CSV: &str = include_str!("../../../../docs/research/data/bass-gen-weights.csv");
/// First wheel note of the table (`DM 0x95122`); words below are zero in the image (VERIFIED).
const FIRST_NOTE: usize = 36;
const NOTES: usize = 79; // notes 36..=114
const FIRST_KEY: usize = 24;
const KEYS: usize = 85; // keys 24..=108

/// Table `T[note][n]` and the writes per wheel of one key-down, parsed once at construction.
#[derive(Debug, Clone)]
pub(super) struct GenWeights {
    /// `table[note - 36][n]`.
    table: Vec<Vec<f32>>,
    /// `(note, writes)` of the key-down of key `24 + i`.
    keys: Vec<Vec<(u8, u8)>>,
    /// Held writes per wheel note.
    held: [u16; 128],
}

impl GenWeights {
    /// Parses the derived table. Allocates here only (setup time).
    pub(super) fn new() -> Self {
        let mut table = vec![Vec::new(); NOTES];
        let mut keys = vec![Vec::new(); KEYS];
        for line in CSV.lines().skip(1) {
            let f: Vec<&str> = line.split(',').collect();
            if f.len() != 4 {
                continue;
            }
            let (a, b) = (
                f[1].parse::<usize>().unwrap_or(0),
                f[2].parse::<usize>().unwrap_or(0),
            );
            if f[0] == "w" {
                let bits = u32::from_str_radix(f[3].trim_start_matches("0x"), 16).unwrap_or(0);
                table[a - FIRST_NOTE].push(f32::from_bits(bits));
            } else {
                keys[a - FIRST_KEY].push((b as u8, f[3].parse::<u8>().unwrap_or(0)));
            }
        }
        Self {
            table,
            keys,
            held: [0; 128],
        }
    }

    fn row(key: usize) -> Option<usize> {
        key.checked_sub(FIRST_KEY).filter(|&i| i < KEYS)
    }

    /// Key-down: the writes of the key join the held counts of their wheels.
    pub(super) fn key_down(&mut self, key: usize) {
        if let Some(i) = Self::row(key) {
            for &(note, n) in &self.keys[i] {
                self.held[usize::from(note)] += u16::from(n);
            }
        }
    }

    /// Key-up: the writes of the key leave the held counts.
    pub(super) fn key_up(&mut self, key: usize) {
        if let Some(i) = Self::row(key) {
            for &(note, n) in &self.keys[i] {
                let h = &mut self.held[usize::from(note)];
                *h = h.saturating_sub(u16::from(n));
            }
        }
    }

    /// Weight word of wheel `note` (0 while no held key writes the wheel).
    pub(super) fn weight(&self, note: usize) -> f32 {
        let held = usize::from(self.held[note.min(127)]);
        note.checked_sub(FIRST_NOTE)
            .and_then(|i| self.table.get(i))
            .and_then(|row| row.get(held.checked_sub(1)?))
            .copied()
            .unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(keys: &[usize]) -> GenWeights {
        let mut g = GenWeights::new();
        for &k in keys {
            g.key_down(k);
        }
        g
    }

    #[test]
    fn chord_words_match_the_arm_key_down() {
        // DM 0x95122 + (note - 36) after the real ARM key-down of 60, 64, 67 (drawbars 888888888,
        // drawbar and percussion items), `tools/organ_bass_gen.py`: 297 replayed writes bit exact.
        let g = chord(&[60, 64, 67]);
        for (note, want) in [(79, 0.24357), (88, 0.43671), (91, 0.5252), (67, 0.27843)] {
            assert!((g.weight(note) - want).abs() < 5e-5, "{note}");
        }
        assert!((chord(&[60]).weight(60) - 0.29402).abs() < 5e-6);
        assert!((chord(&[60]).weight(79) - 0.35321).abs() < 5e-6);
    }

    #[test]
    fn weights_depend_on_the_key_set_not_on_the_order() {
        let (a, b) = (chord(&[60, 64, 67]), chord(&[67, 60, 64]));
        for note in 36..=114 {
            assert_eq!(a.weight(note), b.weight(note));
        }
    }

    #[test]
    fn key_up_restores_the_weights_of_the_remaining_keys() {
        let mut g = chord(&[60, 64, 67]);
        g.key_up(64);
        let rest = chord(&[60, 67]);
        for note in 36..=114 {
            assert_eq!(g.weight(note), rest.weight(note), "{note}");
        }
        // real MIDI path capture: after key-up of 64 wheel 88 is back at its first write 0.54649
        assert!((g.weight(88) - 0.54649).abs() < 5e-6);
        g.key_up(60);
        g.key_up(67);
        assert_eq!(g.weight(88), 0.0);
    }

    #[test]
    fn all_keys_pull_the_weights_down_by_the_arm_table() {
        let all: Vec<usize> = (24..=108).collect();
        let g = chord(&all);
        // wheel 60 (note 60) ring factor after 85 keys is 0.2313 of its first write (11 writes)
        assert!((g.weight(60) / chord(&[60]).weight(60) - 0.2313).abs() < 2e-3);
        assert_eq!(g.weight(20), 0.0);
        assert_eq!(g.weight(200), 0.0);
    }
}
