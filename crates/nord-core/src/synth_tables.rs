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

/// One unison partner voice: pitch ratio and which channel carries it most strongly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnisonVoice {
    pub ratio: f32,
    pub strong_left: bool,
}

/// Number of unison partner voices per Unison setting `0..=3`.
///
/// EMPIRICAL: counted as spectral peaks around the harmonics of a sustained Pure Saw note on a
/// real Stage 4 (Audible Genius "Nord Stage 4 Lesson Pack - Unison" video, 19-44 s, 48 kHz):
/// `2`, `4`, `4` peaks for settings 1, 2, 3, none at the centre pitch. Reproduce with
/// `python tools/sweep_analyze.py unison <clip.wav> --f0 392 --harmonic 10`.
pub const UNISON_VOICE_COUNT: [usize; 4] = [0, 2, 4, 4];

/// Amplitude of a unison voice in its weaker channel relative to its stronger channel.
///
/// EMPIRICAL: peak magnitude ratio `0.43..0.45` (L vs R spectra of the same recording, all three
/// settings); equal-power panning is not assumed, the measured ratio is used.
pub const UNISON_WEAK_CHANNEL_GAIN: f32 = 0.43;

/// Unison partner voices per setting (`[setting][voice]`, unused slots repeat unity).
///
/// VERIFIED: pitch ratios copied from the firmware table `bank0_CODE.bin @0x2f9ee8` (4 x 4 `f32`
/// rows for levels 0..3), checked by `python tools/fw_ratio_scan.py` and by the unit test
/// `unison_ratios_match_the_measured_cents`; the cents also match the hardware measurement above
/// to 0.02 cent (`-2/+2`, `-4/-1.32/+1.33/+4`, `-6/-2/+2/+6`). Which channel is strong is
/// EMPIRICAL (same recording): setting 1 `-2` right, `+2` left; setting 2 `-4` right, `-1.32`
/// left, `+1.33` right, `+4` left; setting 3 `-6` right, `-2` left, `+2` right, `+6` left.
pub const UNISON_VOICES: [[UnisonVoice; 4]; 4] = {
    const fn v(bits: u32, strong_left: bool) -> UnisonVoice {
        UnisonVoice {
            ratio: f32::from_bits(bits),
            strong_left,
        }
    }
    let unity = v(0x3f80_0000, true);
    [
        [unity; 4],
        [
            v(0x3f7f_b44e, false), // -2.00 cents (0.998845)
            v(0x3f80_25e1, true),  // +2.00 cents (1.001156)
            unity,
            unity,
        ],
        [
            v(0x3f7f_68be, false), // -4.00 cents (0.997692)
            v(0x3f7f_cdee, true),  // -1.32 cents (0.999236)
            v(0x3f80_193b, false), // +1.33 cents (1.000770)
            v(0x3f80_4bcb, true),  // +4.00 cents (1.002313)
        ],
        [
            v(0x3f7f_1d3f, false), // -6.00 cents (0.996540)
            v(0x3f7f_b44e, true),  // -2.00 cents (0.998845)
            v(0x3f80_25e1, false), // +2.00 cents (1.001156)
            v(0x3f80_71c5, true),  // +6.00 cents (1.003472)
        ],
    ]
};

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

    #[test]
    fn unison_ratios_match_the_measured_cents() {
        let cents = |r: f32| 1200.0 * r.log2();
        let want: [&[f32]; 4] = [
            &[],
            &[-2.0, 2.0],
            &[-4.0, -1.3226, 1.3334, 4.0],
            &[-6.0, -2.0, 2.0, 6.0],
        ];
        for (level, expected) in want.iter().enumerate() {
            assert_eq!(UNISON_VOICE_COUNT[level], expected.len());
            for (k, &c) in expected.iter().enumerate() {
                let got = cents(UNISON_VOICES[level][k].ratio);
                assert!(
                    (got - c).abs() < 0.01,
                    "level {level} voice {k}: {got} vs {c}"
                );
            }
        }
    }

    #[test]
    fn unison_channels_alternate_so_both_ears_get_inner_and_outer_voices() {
        for level in 1..=3 {
            let n = UNISON_VOICE_COUNT[level];
            let left = UNISON_VOICES[level][..n]
                .iter()
                .filter(|v| v.strong_left)
                .count();
            assert_eq!(left, n / 2, "level {level}");
        }
        let weak = std::hint::black_box(UNISON_WEAK_CHANNEL_GAIN);
        assert!(weak > 0.4 && weak < 0.5);
    }
}
