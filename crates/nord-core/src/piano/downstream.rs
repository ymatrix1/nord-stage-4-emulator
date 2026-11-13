//! Level of the piano after the voice: SMP bus scale, voice gain words and the downstream audit.
//!
//! Evidence: `docs/research/piano-downstream-level.md`. The SMP voice loop writes
//! `bus += (y << 17) * gain >> 31` (`smp_voice.rs`, VERIFIED bit-exact against the DSP image),
//! so a unity-gain stroke of full-scale 14-bit samples (`y = 8191`) is half of the Q31 range.
//! The engine keeps decoded strokes as `y / 8192`; the factor between that unit and a bus word
//! read as a fraction of Q31 full scale is [`FIRMWARE_SAMPLE_SCALE`].

/// Q31 full scale of the SMP bus words (`1.0 = 2^31`).
///
/// ASSUMED: the DSP-to-DSP / DAC word convention is "signed 32-bit, full scale `2^31`" (the
/// natural reading of an `SSF` Q31 multiply). The bus words and `y << 17` are VERIFIED; the
/// convention of the receiver of the bus is not decoded. A different receiver scale moves the
/// whole piano level by a constant factor (`docs/OPEN-QUESTIONS.md` OQ20).
pub const BUS_FULL_SCALE: f32 = 2_147_483_648.0;

/// Engine amplitude per decoded `y / 8192` unit and per unit voice gain on the sample path
/// before the firmware bus convention (legacy default).
///
/// ASSUMED: inherited from the first piano engine, no recorded source. It is 0.83 dB above
/// [`FIRMWARE_SAMPLE_SCALE`], so the legacy layer is that much hotter than the bus convention.
pub const LEGACY_SAMPLE_SCALE: f32 = 0.55;

/// Amplitude per `y / 8192` unit and unit voice gain implied by the SMP bus words read as
/// fractions of [`BUS_FULL_SCALE`]: `(y << 17) / 2^31 = (y / 8192) * 0.5`.
///
/// ASSUMED (derived): the shift 17 is VERIFIED (`smp_voice.rs`, test
/// `bus_unit_matches_the_engine_unit_times_the_firmware_scale`); the Q31 receiver convention
/// is [`BUS_FULL_SCALE`]'s assumption. Opt-in via `PianoLayer::firmware_bus_scale`.
pub const FIRMWARE_SAMPLE_SCALE: f32 = 0.5;

/// Synthetic voices (no stroke): the legacy harmonic model keeps its own level.
///
/// ASSUMED: inherited from the first piano engine, no recorded source.
pub const SYNTHETIC_VOICE_SCALE: f32 = 0.2;

/// Per-voice scale of the sample path: `firmware_bus` selects the bus convention.
#[inline]
pub fn sample_scale(has_stroke: bool, firmware_bus: bool) -> f32 {
    match (has_stroke, firmware_bus) {
        (false, _) => SYNTHETIC_VOICE_SCALE,
        (true, true) => FIRMWARE_SAMPLE_SCALE,
        (true, false) => LEGACY_SAMPLE_SCALE,
    }
}

/// Q31 gain word of a voice from its Q23 level (`gain = level_q23 / 2^23` as a float, written
/// to the SMP record; the SMP's `SSF` multiplier is that float times `2^31`).
///
/// ASSUMED: the float-to-Q31 step of the SMP interrupt is not traced; unity (`2^23`) maps to
/// `0x7FFFFFFF`, the value the oracle runs use.
pub fn gain_q31(level_q23: u32) -> i32 {
    (i64::from(level_q23) << 8).min(i64::from(i32::MAX)) as i32
}

/// Bus word as a fraction of Q31 full scale.
#[inline]
pub fn bus_unit(word: i32) -> f32 {
    word as f32 / BUS_FULL_SCALE
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::piano::smp_page::{play_page, synthetic_page, PlayConfig};

    /// Decoded value of the synthetic page's first frames at unity gain and step 1: the bus is
    /// `y << 17`, which read as Q31 is `(y / 8192) * FIRMWARE_SAMPLE_SCALE`.
    #[test]
    fn bus_unit_matches_the_engine_unit_times_the_firmware_scale() {
        let page = synthetic_page(1, 6, 2);
        let c = PlayConfig {
            step: 1.0,
            ..PlayConfig::default()
        };
        let frames = play_page(&page, &[[0; 4]; 2], &c, 200);
        let mut checked = 0;
        for f in &frames {
            // `last` is R2 after the shift (`y << 17`); unity gain is 0x7FFFFFFF, one LSB under
            // 1.0: bus = (R2 * g) >> 31 (floor).
            let r2 = f.last[0];
            let want_word = (i64::from(r2) * i64::from(i32::MAX)) >> 31;
            assert_eq!(i64::from(f.bus[0]), want_word);
            let engine_unit = (r2 >> 17) as f32 / 8192.0;
            let got = bus_unit(f.bus[0]);
            assert!((got - engine_unit * FIRMWARE_SAMPLE_SCALE).abs() < 1e-6);
            if r2 != 0 {
                checked += 1;
            }
        }
        assert!(checked > 20, "the page must carry non-zero samples");
    }

    #[test]
    fn full_scale_stroke_sits_at_half_of_the_bus() {
        assert!((bus_unit(8191 << 17) - 8191.0 / 16384.0).abs() < 1e-7);
        assert!(bus_unit(8191 << 17) < 0.5);
    }

    #[test]
    fn gain_word_is_the_level_times_two_to_the_eight_and_saturates_at_unity() {
        assert_eq!(gain_q31(0), 0);
        assert_eq!(gain_q31(1 << 22), 1 << 30);
        assert_eq!(gain_q31(0x7F_FFFF), 0x7FFF_FF00);
        assert_eq!(gain_q31(1 << 23), i32::MAX);
        // the engine's voice gain level/2^23 equals gain_q31 / 2^31 to one LSB
        for level in [1u32, 203_311, 2_097_152, 3_780_118, 0x7F_FFFF] {
            let a = level as f32 / 8_388_608.0;
            let b = gain_q31(level) as f32 / BUS_FULL_SCALE;
            assert!((a - b).abs() < 1e-7);
        }
    }

    #[test]
    fn gain_scales_the_bus_linearly_at_the_voice_gain() {
        let page = synthetic_page(1, 6, 2);
        let level = 3_780_118u32;
        let unity = PlayConfig {
            step: 1.0,
            ..PlayConfig::default()
        };
        let scaled = PlayConfig {
            gain: gain_q31(level),
            ..unity
        };
        let a = play_page(&page, &[[0; 4]; 2], &unity, 200);
        let b = play_page(&page, &[[0; 4]; 2], &scaled, 200);
        let g = level as f32 / 8_388_608.0;
        for (fa, fb) in a.iter().zip(&b) {
            let want = bus_unit(fa.bus[0]) * g;
            assert!((bus_unit(fb.bus[0]) - want).abs() < 2e-7);
        }
    }

    /// The layer flag scales a stroke voice by exactly `0.5 / 0.55`; default is legacy.
    #[test]
    fn layer_flag_scales_the_sample_path_by_the_bus_convention() {
        use crate::piano::{NpnoSound, PianoLayer};
        use std::sync::Arc;
        let sound = Arc::new(NpnoSound::new_synthetic("bus", 60, &[0.4; 512], 48_000.0));
        let peak = |bus: bool| {
            let mut l = PianoLayer::new(48_000.0);
            l.set_sound(Some(sound.clone()));
            assert!(!l.firmware_bus_scale, "default is the legacy scale");
            l.firmware_bus_scale = bus;
            l.note_on(60, 100);
            (0..200)
                .map(|_| l.render_frame()[0].abs())
                .fold(0.0f32, f32::max)
        };
        let (legacy, firmware) = (peak(false), peak(true));
        assert!(legacy > 0.0);
        let want = FIRMWARE_SAMPLE_SCALE / LEGACY_SAMPLE_SCALE;
        assert!(
            (firmware / legacy - want).abs() < 1e-3,
            "{}",
            firmware / legacy
        );
    }

    #[test]
    fn sample_scale_selects_the_convention() {
        assert_eq!(sample_scale(true, false), LEGACY_SAMPLE_SCALE);
        assert_eq!(sample_scale(true, true), FIRMWARE_SAMPLE_SCALE);
        assert_eq!(sample_scale(false, true), SYNTHETIC_VOICE_SCALE);
        assert_eq!(sample_scale(false, false), SYNTHETIC_VOICE_SCALE);
        let db = 20.0 * (LEGACY_SAMPLE_SCALE / FIRMWARE_SAMPLE_SCALE).log10();
        assert!(
            (db - 0.83).abs() < 0.01,
            "legacy is 0.83 dB hotter, got {db}"
        );
    }
}
