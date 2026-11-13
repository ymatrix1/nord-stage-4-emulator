//! Firmware band-limited falling saw (Core2 helper `0x12501F`) as an opt-in oscillator model.
//!
//! Spec: `docs/specs/synth-engine.md` section 3.1; evidence `docs/research/synth-voice-control.md`
//! section 5; closed form and golden generator `tools/synth_saw_model.py`,
//! `tools/synth_saw_golden.py`.
//!
//! With `p` the phase of the sample in cycles (`0..1`) and `i` the increment in cycles per sample:
//!
//! ```text
//! t    = p / i (p < 0.5), (p - 1) / i (p >= 0.5)       signed distance to the wrap, in increments
//! d(t) = -(1 - t/2)^2 (0 <= t < 2), +(1 + t/2)^2 (-2 < t < 0), else 0
//! y    = g(i) (1 - 2 p + d(t)),  g(i) = C (B + clip(A - 2 i, +-B))
//! ```
//!
//! The wave is FALLING (`+1` just after the wrap, `-1` just before); the classic path of
//! `render.rs` is rising with a one-increment step. `g` is 1 up to `i = 0.1152` and fades linearly
//! to silence at `i = 0.2304`, so notes above about 5.5 kHz at 48 kHz thin out and disappear.

/// Which saw the analog oscillator (`Saw` and `SuperSaw`) uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SawModel {
    /// Legacy: rising saw `2t - 1` with a one-increment PolyBLEP. ASSUMED textbook shape.
    #[default]
    Classic,
    /// Firmware helper `0x12501F`: falling, two-increment quadratic step, high-pitch fade.
    Firmware,
}

/// VERIFIED: `A = 0x3EB0F34D` of the helper prologue (`0x125023`), checked by
/// `tools/synth_saw_model.py check` (512 pairs, max err 5.9e-8) and `tools/tests/test_synth_saw_model.py`.
pub const SAW_A: f32 = 0.345_606_24;
/// VERIFIED: `B = 0x3DEBEF11` (`0x125028`), same check as [`SAW_A`].
pub const SAW_B: f32 = 0.115_202_08;
/// VERIFIED: `C = 0x408AE2EA` (`0x12502E`), same check as [`SAW_A`]; `2 B C = 1`.
pub const SAW_C: f32 = 4.340_199_5;

/// High-pitch fade `g(i)` for an increment of `inc` cycles per sample.
#[inline]
pub fn firmware_saw_fade(inc: f32) -> f32 {
    SAW_C * (SAW_B + (SAW_A - 2.0 * inc).clamp(-SAW_B, SAW_B))
}

/// Quadratic wrap correction `d(t)`; `t` in increments after (positive) or before the wrap.
#[inline]
pub fn firmware_blep_step(t: f32) -> f32 {
    if (0.0..2.0).contains(&t) {
        let u = 1.0 - 0.5 * t;
        -(u * u)
    } else if t > -2.0 && t < 0.0 {
        let u = 1.0 + 0.5 * t;
        u * u
    } else {
        0.0
    }
}

/// One sample of the firmware saw at phase `phase` (`0..1`) and increment `inc` (`> 0`).
///
/// VERIFIED against the SHARC oracle through the Python closed form (grid golden
/// `tests_hw_saw_golden.rs`, tolerance 2e-6 from float32 arithmetic).
#[inline]
pub fn firmware_saw(phase: f32, inc: f32) -> f32 {
    if inc <= 0.0 {
        return 0.0;
    }
    let t = if phase < 0.5 {
        phase / inc
    } else {
        (phase - 1.0) / inc
    };
    firmware_saw_fade(inc) * (1.0 - 2.0 * phase + firmware_blep_step(t))
}

/// Saw sample of the selected model (`Classic`: `(2 t - 1) - poly_blep(t, dt)`).
#[inline]
pub(super) fn saw_wave(model: SawModel, phase: f32, inc: f32) -> f32 {
    match model {
        SawModel::Classic => (2.0 * phase - 1.0) - super::poly_blep(phase, inc),
        SawModel::Firmware => firmware_saw(phase, inc),
    }
}

#[cfg(test)]
mod tests {
    use super::super::SynthLayer;
    use super::*;

    #[test]
    fn constants_match_image_words() {
        assert_eq!(SAW_A, f32::from_bits(0x3EB0_F34D));
        assert_eq!(SAW_B, f32::from_bits(0x3DEB_EF11));
        assert_eq!(SAW_C, f32::from_bits(0x408A_E2EA));
        assert!((2.0 * SAW_B * SAW_C - 1.0).abs() < 1e-6);
    }

    #[test]
    fn fade_is_unity_below_the_knee_and_silent_above_the_end() {
        assert!((firmware_saw_fade(0.05) - 1.0).abs() < 1e-6);
        assert!((firmware_saw_fade(0.1152) - 1.0).abs() < 1e-4);
        assert!((firmware_saw_fade(SAW_A / 2.0) - 0.5).abs() < 1e-6);
        assert!(firmware_saw_fade(0.2304) < 1e-4);
        assert_eq!(firmware_saw_fade(0.4), 0.0);
    }

    #[test]
    fn step_values_measured_in_the_oracle() {
        assert_eq!(firmware_blep_step(0.0), -1.0);
        assert_eq!(firmware_blep_step(1.0), -0.25);
        assert_eq!(firmware_blep_step(-1.0), 0.25);
        assert_eq!(firmware_blep_step(2.0), 0.0);
        assert_eq!(firmware_blep_step(-2.0), 0.0);
    }

    #[test]
    fn saw_falls_and_is_zero_on_the_wrap_sample() {
        let inc = 1.0 / 256.0;
        assert_eq!(firmware_saw(0.0, inc), 0.0);
        assert!((firmware_saw(0.25, inc) - 0.5).abs() < 1e-6);
        assert!((firmware_saw(0.75, inc) + 0.5).abs() < 1e-6);
        assert!(firmware_saw(0.3, inc) > firmware_saw(0.6, inc));
        // the classic saw rises
        assert!(saw_wave(SawModel::Classic, 0.3, 0.001) < saw_wave(SawModel::Classic, 0.6, 0.001));
    }

    #[test]
    fn high_pitch_fades_to_silence() {
        assert_eq!(firmware_saw(0.3, 0.25), 0.0);
        assert!(firmware_saw(0.3, 0.2).abs() < firmware_saw(0.3, 0.05).abs());
        assert_eq!(firmware_saw(0.3, 0.0), 0.0);
    }

    fn raw_layer(model: SawModel, note: u8) -> Vec<f32> {
        let mut s = SynthLayer::new(48_000.0);
        s.filter_on = false;
        s.saw_model = model;
        s.attack_sec = 0.0005;
        s.note_on(note, 127);
        (0..4800).map(|_| s.render_frame()[0]).collect()
    }

    #[test]
    fn layer_firmware_saw_is_opposite_in_sign_to_the_classic_saw() {
        let a = raw_layer(SawModel::Classic, 60);
        let b = raw_layer(SawModel::Firmware, 60);
        let dot: f32 = a[2400..].iter().zip(&b[2400..]).map(|(x, y)| x * y).sum();
        let ea: f32 = a[2400..].iter().map(|x| x * x).sum();
        assert!(ea > 0.0 && dot < -0.8 * ea, "dot {dot} energy {ea}");
    }

    #[test]
    fn layer_firmware_saw_is_silent_above_the_fade_and_classic_is_not() {
        // note 127 = 12.5 kHz: 0.26 cycles per sample, past the 0.2304 end of the fade
        assert!(raw_layer(SawModel::Firmware, 127).iter().all(|&x| x == 0.0));
        assert!(raw_layer(SawModel::Classic, 127).iter().any(|&x| x != 0.0));
    }

    #[test]
    fn default_model_is_classic() {
        assert_eq!(SynthLayer::new(48_000.0).saw_model, SawModel::Classic);
    }
}
