//! Level law of the hardware piano sampler: attenuation index to the Q23 amplitude.
//!
//! Evidence: `docs/research/piano-dsp-path.md` section 7 (PR #125). The ARM routine `0xC00A1D5C`
//! computes the voice amplitude in Q23 as `G[n] >> octaves`, with the 61-word table
//! `G[n] = round(2^23 * 2^(-n / 60))` at ARM `0xC02A1A08` and the index
//! `n = max(0, S.i16[0x5E] + 2 * X + 60 - b1 - b2 - b3)` reduced by 60 per octave. One index step
//! is `20 * log10(2^(1/60))` = 0.1003 dB. The three `b` values are signed per-key bytes and `X` is
//! a call-frame word; how velocity reaches `X` is not derived yet, so this module only provides the
//! table and the index to amplitude map.

use std::sync::OnceLock;

/// Entries of the level table: `G[0..=60]` (index 60 is one octave down, `0x400000`).
pub const TABLE_LEN: usize = 61;
/// Index steps per octave (6.02 dB / 60).
pub const STEPS_PER_OCTAVE: u32 = 60;
/// Largest Q23 amplitude (`2^23 - 1`): the table entry 0 (`0x800000`) saturates to it at use.
pub const Q23_MAX: u32 = 0x007F_FFFF;

/// VERIFIED: `G[n] = round(2^23 * 2^(-n / 60))` for `n = 0..=60` equals the firmware table at
/// `bank0_CODE@0xC02A1A08` (OS v1.66, 61 of 61 entries, max error 0), checked by
/// `tools/arm_piano_path.py level` and by `table_matches_the_firmware_fingerprint`.
pub fn level_table() -> &'static [u32; TABLE_LEN] {
    static TABLE: OnceLock<[u32; TABLE_LEN]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0u32; TABLE_LEN];
        for (n, slot) in table.iter_mut().enumerate() {
            *slot = (8_388_608.0 * (-(n as f64) / 60.0).exp2()).round() as u32;
        }
        table
    })
}

/// EMPIRICAL (read from the listing of `0xC00A1D5C`, not run end to end): the Q23 amplitude for an
/// attenuation `index`. Negative indices clamp to 0 (the ARM applies `max(0, ..)`), the index is
/// reduced by 60 per octave, and the result saturates to [`Q23_MAX`].
pub fn level_q23(table: &[u32; TABLE_LEN], index: i32) -> u32 {
    let n = index.max(0) as u32;
    let octaves = n / STEPS_PER_OCTAVE;
    let remainder = (n % STEPS_PER_OCTAVE) as usize;
    let amplitude = table[remainder].checked_shr(octaves).unwrap_or(0);
    amplitude.min(Q23_MAX)
}

/// The amplitude as a linear gain (`level_q23 / 2^23`, full scale is `0x7FFFFF / 2^23`).
pub fn level_gain(amplitude: u32) -> f64 {
    f64::from(amplitude) / 8_388_608.0
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

    /// Fingerprint of the 61 unsaturated words (entry 0 is `0x800000`, entry 60 is `0x400000`).
    #[test]
    fn table_matches_the_firmware_fingerprint() {
        let table = level_table();
        assert_eq!(table[0], 0x0080_0000);
        assert_eq!(table[1], 0x007E_87A1);
        assert_eq!(table[60], 0x0040_0000);
        assert_eq!(fingerprint(table), 0x5E6F_2496_33B3_09C8);
    }

    #[test]
    fn index_maps_to_amplitude_with_octave_reduction() {
        let table = level_table();
        assert_eq!(level_q23(table, 0), Q23_MAX, "entry 0 saturates");
        assert_eq!(level_q23(table, -5), Q23_MAX, "negative indices clamp to 0");
        assert_eq!(level_q23(table, 1), 0x007E_87A1);
        assert_eq!(level_q23(table, 30), 0x005A_827A);
        assert_eq!(level_q23(table, 60), 0x0040_0000);
        assert_eq!(level_q23(table, 61), 0x003F_43D0);
        assert_eq!(level_q23(table, 120), 0x0020_0000);
        assert_eq!(level_q23(table, 60 * 40), 0, "far attenuation shifts out");
        assert_eq!(level_q23(table, i32::MAX), 0);
    }

    /// 60 steps per octave: the gain halves every 60 steps and one step is 0.1003 dB.
    #[test]
    fn steps_are_a_tenth_of_a_decibel() {
        let table = level_table();
        for index in [1, 7, 30, 59, 60, 97, 150] {
            let exact = 2f64.powf(-f64::from(index) / 60.0);
            let got = level_gain(level_q23(table, index));
            let db_error = 20.0 * (got / exact).log10();
            assert!(db_error.abs() < 0.01, "index {index}: {db_error} dB");
        }
        let step_db =
            20.0 * (level_gain(level_q23(table, 1)) / level_gain(level_q23(table, 2))).log10();
        assert!((step_db - 0.1003).abs() < 0.001, "{step_db}");
    }

    /// Every table word is within half a Q23 unit of the exact power.
    #[test]
    fn table_is_correctly_rounded() {
        for (n, &word) in level_table().iter().enumerate() {
            let exact = 8_388_608.0 * (-(n as f64) / 60.0).exp2();
            assert!((f64::from(word) - exact).abs() <= 0.5, "entry {n}");
        }
    }
}
