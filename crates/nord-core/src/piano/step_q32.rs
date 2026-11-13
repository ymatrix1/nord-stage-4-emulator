//! Pitch step of the hardware piano sampler: cents to the Q32 `step_frac` word of an SMP voice
//! record.
//!
//! Evidence: `docs/research/piano-dsp-path.md` (PR #120, section "Pitch law"). The ARM writer
//! `0xC00B7B68` sends `step_frac = min(T[n] >> octaves, 0xFFFFFF) << 8` with `n = -cents`,
//! `octaves = n / 1200`, and the 1,200-entry table `T` at ARM `0xC02A6168`
//! (`T[r] = 2^24 * 2^(-r / 1200)`, followed by `0x800000` and `0`). The ARM only produces cents
//! that are zero or negative, so the step never exceeds unity: the SMP loop decodes one sample per
//! call and cannot skip residuals.
//!
//! The table is not stored here: it is rebuilt once from its closed form (a constant 4.8 KB).

use std::sync::OnceLock;

/// Entries of the cents table: `T[0..=1200]` (index 1200 is the octave, `0x800000`).
pub const TABLE_LEN: usize = 1201;
/// Cents per octave.
pub const CENTS_PER_OCTAVE: i32 = 1200;
/// Largest Q24 ratio the ARM writes before the left shift (`0xFFFFFF`): unity is clamped to it.
pub const Q24_MAX: u32 = 0x00FF_FFFF;

/// VERIFIED: `T[n] = round(2^24 * 2^(-n / 1200))` for `n = 0..=1200` equals the firmware table at
/// `bank0_CODE@0xC02A6168` (OS v1.66, 0 mismatches; max error 0.4998 Q24 units against the exact
/// power, i.e. the table is correctly rounded). Checked by `tools/arm_piano_path.py` and by
/// `table_matches_the_firmware_fingerprint` (FNV-1a over the little-endian 32-bit words).
pub fn cents_table() -> &'static [u32; TABLE_LEN] {
    static TABLE: OnceLock<[u32; TABLE_LEN]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0u32; TABLE_LEN];
        for (n, slot) in table.iter_mut().enumerate() {
            *slot = (16_777_216.0 * (-(n as f64) / 1200.0).exp2()).round() as u32;
        }
        table
    })
}

/// VERIFIED (emulator run of `0xC00B7B68` and the goldens below): the Q32 step word the ARM writes
/// for `cents` of detune relative to the recorded pitch. Positive cents are clamped to unity
/// (ASSUMED: the ARM never produces them, so their behaviour is not observed).
pub fn step_frac_q32(table: &[u32; TABLE_LEN], cents: i32) -> u32 {
    let n = cents.saturating_neg().max(0) as u32;
    let octaves = n / CENTS_PER_OCTAVE as u32;
    let remainder = (n % CENTS_PER_OCTAVE as u32) as usize;
    let q24 = table[remainder].checked_shr(octaves).unwrap_or(0);
    q24.min(Q24_MAX) << 8
}

/// The step as a playback ratio (`step_frac / 2^32`, unity is `0xFFFFFF00 / 2^32` = 0.99999994).
pub fn step_ratio(step_frac: u32) -> f64 {
    f64::from(step_frac) / 4_294_967_296.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FNV-1a 64 over the little-endian bytes of the 32-bit table words.
    fn fingerprint(table: &[u32; TABLE_LEN]) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for word in table {
            for byte in word.to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        hash
    }

    /// Fingerprint computed from the 1,201 words of `tools/arm_piano_path.py` (0 mismatches against
    /// the firmware table at ARM `0xC02A6168`).
    #[test]
    fn table_matches_the_firmware_fingerprint() {
        let table = cents_table();
        assert_eq!(table[0], 0x0100_0000);
        assert_eq!(table[1200], 0x0080_0000);
        assert_eq!(fingerprint(table), 0xD8A6_7986_7A8B_A945);
    }

    /// Goldens of the Q32 word for the cents values named in the research note.
    #[test]
    fn step_words_match_the_goldens() {
        let table = cents_table();
        assert_eq!(step_frac_q32(table, 0), 0xFFFF_FF00);
        assert_eq!(step_frac_q32(table, -100), 0xF1A1_BF00);
        assert_eq!(step_frac_q32(table, -600), 0xB504_F300);
        assert_eq!(step_frac_q32(table, -1200), 0x8000_0000);
        assert_eq!(step_frac_q32(table, -2400), 0x4000_0000);
    }

    /// The table is a correctly rounded 12-TET ladder: each word is within half a Q24 unit of the
    /// exact power, and 100 cents is the semitone ratio.
    #[test]
    fn table_is_an_equal_tempered_ladder() {
        let table = cents_table();
        for (n, &word) in table.iter().enumerate() {
            let exact = 16_777_216.0 * (-(n as f64) / 1200.0).exp2();
            assert!((f64::from(word) - exact).abs() <= 0.5, "entry {n}");
        }
        let semitone = f64::from(table[100]) / f64::from(table[0]);
        assert!(
            (semitone - 2f64.powf(-1.0 / 12.0)).abs() < 1e-7,
            "{semitone}"
        );
    }

    /// Positive cents clamp to unity; a huge negative value shifts the word to zero.
    #[test]
    fn extreme_cents_are_clamped() {
        let table = cents_table();
        assert_eq!(step_frac_q32(table, 300), step_frac_q32(table, 0));
        assert_eq!(step_frac_q32(table, i32::MIN), 0);
        assert_eq!(step_frac_q32(table, -1200 * 40), 0);
    }

    /// The ratio never exceeds unity and halves per octave.
    #[test]
    fn ratios_halve_per_octave() {
        let table = cents_table();
        let unity = step_ratio(step_frac_q32(table, 0));
        assert!(unity < 1.0 && unity > 0.999_999);
        let octave = step_ratio(step_frac_q32(table, -1200));
        assert_eq!(octave, 0.5);
        assert!((step_ratio(step_frac_q32(table, -2400)) - 0.25).abs() < 1e-12);
        let fifth = step_ratio(step_frac_q32(table, -700));
        assert!((fifth - 2f64.powf(-7.0 / 12.0)).abs() < 1e-6, "{fifth}");
    }
}
