//! Value-scale tables recovered from the OS v1.66 firmware.
//!
//! Each table is documented with its firmware location and the command that checks it
//! (`.agents/rules/provenance-in-code.md`).

/// LFO modulation amount when the LFO targets Pitch, in semitones, indexed by the 7-bit `lfo_amount`.
///
/// VERIFIED: copied from `bank0_CODE.bin` @0x2ca2c0 (128 little-endian `f32`, monotonic, `0..48`),
/// checked by `python tools/ns4_fw_tables.py target/fw_unpacked/bank0_CODE.bin`, max err 5e-7
/// (values rounded to 6 decimals). The curve is within 0.42 semitone of `48 * (i/127)^2.5`
/// (ns4decode's fit), so the table is used instead of the formula.
pub const LFO_PITCH_SEMITONES: [f32; 128] = [
    0.0, 0.00026, 0.00149, 0.00412, 0.00845, 0.01476, 0.02329, 0.03424, 0.0478, 0.06417, 0.08351,
    0.10598, 0.13173, 0.16091, 0.19367, 0.23012, 0.27042, 0.31467, 0.36301, 0.41554, 0.5, 0.53368,
    0.5995, 0.66996, 0.74518, 0.82524, 0.91026, 1.0, 1.09554, 1.19599, 1.30177, 1.41298, 1.5297,
    1.65203, 1.78004, 1.91382, 2.0, 2.19906, 2.35067, 2.50838, 2.67228, 2.84244, 3.0, 3.20187,
    3.39128, 3.58726, 3.78989, 4.0, 4.25, 4.5, 4.75, 5.0, 5.25, 5.5, 5.75, 6.0, 6.25, 6.5, 6.75,
    7.0, 7.33333, 7.66666, 8.0, 8.33333, 8.66666, 9.0, 9.33333, 9.66666, 10.0, 10.55555, 11.0,
    11.33333, 11.66666, 12.0, 12.5, 13.0, 13.33333, 13.66666, 14.0, 14.5, 15.0, 15.5, 16.0, 16.5,
    17.0, 17.5, 18.0, 18.5, 19.0, 20.0, 20.5, 21.0, 21.5, 22.0, 22.5, 23.0, 24.0, 24.5, 25.0, 26.0,
    26.5, 27.0, 28.0, 28.5, 29.0, 30.0, 30.5, 31.0, 32.0, 33.0, 33.5, 34.0, 35.0, 36.0, 37.0, 37.5,
    38.0, 39.0, 40.0, 41.0, 42.0, 42.5, 43.0, 44.0, 45.0, 46.0, 47.0, 48.0,
];

/// Pitch depth in semitones for a normalised LFO amount `0.0..=1.0` (nearest table entry).
pub fn lfo_pitch_semitones(amount: f32) -> f32 {
    let idx = (amount.clamp(0.0, 1.0) * 127.0).round() as usize;
    LFO_PITCH_SEMITONES[idx.min(127)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lfo_pitch_table_is_monotonic_and_spans_zero_to_48_semitones() {
        assert_eq!(LFO_PITCH_SEMITONES[0], 0.0);
        assert_eq!(LFO_PITCH_SEMITONES[127], 48.0);
        assert!(LFO_PITCH_SEMITONES.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn lfo_pitch_table_tracks_the_power_law_fit_within_half_a_semitone() {
        for (i, &v) in LFO_PITCH_SEMITONES.iter().enumerate() {
            let fit = 48.0 * (i as f32 / 127.0).powf(2.5);
            assert!((v - fit).abs() < 0.5, "i={i} table={v} fit={fit}");
        }
        // ns4decode lists 1 semitone at raw 27 (cents below).
        assert!((LFO_PITCH_SEMITONES[27] - 1.0).abs() < 1e-3);
    }

    #[test]
    fn lfo_pitch_semitones_clamps_and_rounds_to_the_nearest_entry() {
        assert_eq!(lfo_pitch_semitones(-1.0), 0.0);
        assert_eq!(lfo_pitch_semitones(2.0), 48.0);
        assert_eq!(lfo_pitch_semitones(27.0 / 127.0), LFO_PITCH_SEMITONES[27]);
    }
}
