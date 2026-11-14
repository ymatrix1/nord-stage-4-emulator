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

/// Gain the engine applies for a piano layer level `level` (`v / 127`, 0..1).
///
/// VERIFIED (run): the layer level handler `0xC011A7A4` (ids `0xB7`, `0xB8`), executed in the
/// ARM harness (`tools/arm_layer_level.py`, `docs/research/piano-bus-convention.md` s3) stores
/// `(v / 127)^2` for `v` = 0, 1, 2, 10, 32, 63, 64, 100, 126, 127 (relative error below 1e-6)
/// and the mute sentinel `-0.05` for 0. Whether the stored word reaches the SMP voice gains
/// unchanged is not traced, so the squared law is opt-in (`PianoLayer::firmware_layer_law`).
#[inline]
pub fn layer_gain(level: f32, firmware_law: bool) -> f32 {
    if firmware_law {
        let x = level.clamp(0.0, 1.0);
        x * x
    } else {
        level
    }
}

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

/// SMP ring float per bus word: the post stage is unity and converts with `FLOAT BY -29`.
///
/// VERIFIED: `ring = bus * 2^-29` (oracle run through the audio interrupt,
/// `docs/research/piano-bus-convention.md` s2b; `smp_post_stage.scn`).
pub const SMP_RING_PER_BUS_WORD: f32 = 1.0 / 536_870_912.0;

/// Master gain word of the FX Core1 output stage, `DM(0xB651A)` of the image (`0x3E3F1412`).
///
/// VERIFIED (run): with the level word `DM(0xB6518)` at 1.0 the stage multiplies the master
/// buffer by this word and negates it; PNO lanes and the SMP stream reach the output words with
/// the same gain, -0.1862 (PNO) and -0.1869 (SMP) after 5000 passes of `sharc-org`
/// (`tools/fx_input_path.py`, `docs/research/piano-receiver-scale.md`). ASSUMED: that the ARM
/// leaves `DM(0xB6518)` at 1.0 (it is 0 in the image and no writer was found).
pub const FX_MASTER_GAIN_WORD: u32 = 0x3E3F_1412;

/// DAC conversion of the FX Core1 pass: `word = fix(clip(2 * x, 0.999)) * 2^31`.
///
/// VERIFIED (run): output words preset to 0.25 give the DAC word `0x40000000`; 0.5 and above give
/// `0x7FDF3B80` (`0.999 * 2^31`); the constant is the immediate `0x3F7FBE77` at `0x121321`.
pub const FX_DAC_CLIP_WORD: u32 = 0x3F7F_BE77;

/// Float full scale of the FX master buffer: `x = 0.5` is the DAC clip (`clip / 2`).
#[inline]
pub fn fx_dac_full_scale() -> f32 {
    f32::from_bits(FX_DAC_CLIP_WORD) * 0.5
}

/// DAC word as a fraction of `2^31` for a float `x` on the FX master buffer (level word 1.0).
#[inline]
pub fn fx_dac_fraction(x: f32) -> f32 {
    let clip = f32::from_bits(FX_DAC_CLIP_WORD);
    (2.0 * f32::from_bits(FX_MASTER_GAIN_WORD) * x).clamp(-clip, clip)
}

/// DAC fraction of one SMP bus word through the verified receiver path (unity copy, unity master
/// IIR, master gain, DAC doubling).
#[inline]
pub fn bus_word_dac_fraction(word: i32) -> f32 {
    fx_dac_fraction(word as f32 * SMP_RING_PER_BUS_WORD)
}

/// Engine amplitude per `y / 8192` unit and unit voice gain if the DAC fraction were the engine
/// unit: `(y << 17)` is `y / 8192 * 2^30`, so the DAC fraction is `4 * G * y / 8192`.
///
/// ASSUMED (derived, not wired): [`FIRMWARE_SAMPLE_SCALE`] is `0.5`, so the receiver path is
/// 3.5 dB above the bus reading. Not an engine default: the clip calibration of the organ (`K`)
/// sees the same path, see `docs/research/piano-receiver-scale.md` s4.
#[inline]
pub fn receiver_path_sample_scale() -> f32 {
    4.0 * f32::from_bits(FX_MASTER_GAIN_WORD)
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
    fn layer_gain_is_linear_or_the_squared_firmware_law() {
        assert_eq!(layer_gain(0.5, false), 0.5);
        assert_eq!(layer_gain(0.5, true), 0.25);
        assert_eq!(layer_gain(1.0, true), 1.0);
        assert_eq!(layer_gain(0.0, true), 0.0);
        // v = 100: (100 / 127)^2 = 0.62000 (the live handler stored 0.6200012)
        assert!((layer_gain(100.0 / 127.0, true) - 0.620_001_2).abs() < 1e-6);
        assert_eq!(layer_gain(1.7, true), 1.0);
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

    #[test]
    fn receiver_path_constants_follow_the_verified_words() {
        assert!((f32::from_bits(FX_MASTER_GAIN_WORD) - 0.1866).abs() < 1e-7);
        assert!((fx_dac_full_scale() - 0.4995).abs() < 1e-6);
        // float 0.25 on the master buffer -> 2 * 0.1866 * 0.25 of 2^31
        assert!((fx_dac_fraction(0.25) - 0.0933).abs() < 1e-6);
        // the clip is reached from x = 0.999 / (2 G)
        assert!((fx_dac_fraction(10.0) - 0.999).abs() < 1e-6);
        assert!((fx_dac_fraction(-10.0) + 0.999).abs() < 1e-6);
    }

    #[test]
    fn bus_word_to_dac_matches_the_ring_arithmetic() {
        // 2^28 bus word -> ring 0.5 -> 2 * 0.1866 * 0.5 = 0.1866 of 2^31
        let w = 1_i32 << 28;
        assert!((w as f32 * SMP_RING_PER_BUS_WORD - 0.5).abs() < 1e-9);
        assert!((bus_word_dac_fraction(w) - 0.1866).abs() < 1e-6);
        // a full-scale unity stroke (2^30, ring 2.0) reads 0.7464 of 2^31 through the receiver path
        assert!((bus_word_dac_fraction(1 << 30) - 0.7464).abs() < 1e-6);
    }

    #[test]
    fn receiver_path_scale_is_the_bus_reading_times_four_g() {
        let g = f32::from_bits(FX_MASTER_GAIN_WORD);
        assert!((receiver_path_sample_scale() - 4.0 * g).abs() < 1e-9);
        // per decoded unit: the bus word is y/8192 * 2^30, so the DAC fraction matches
        let word = 1_i32 << 29; // y / 8192 = 0.5 at unity gain: 0.5 * 2^30
        let want = 0.5 * receiver_path_sample_scale();
        assert!((bus_word_dac_fraction(word) - want).abs() < 1e-6);
        let ratio_db = 20.0 * (receiver_path_sample_scale() / FIRMWARE_SAMPLE_SCALE).log10();
        assert!((ratio_db - 3.48).abs() < 0.01, "{ratio_db}");
    }
}
