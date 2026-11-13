//! Opt-in laws measured on the Audible Genius lesson clips (`SynthLayer::measured_laws`).
//!
//! Every constant is EMPIRICAL (lossy YouTube audio of a real Stage 4, panel values read from the
//! OLED); the evidence, errors and caveats are in `docs/research/audible-genius-synth-lessons.md`.
//! Defaults stay on the older scales, so `render_hash` does not move.

/// Voices of the hardware Super Saw: equal level, equally spaced in cents, no centre voice.
///
/// EMPIRICAL: 16 equal peaks at harmonics 3 and 5 of a G4 at Detune 10.0 (spacing 12.5 cents at
/// every harmonic, outer voices -93.7 / +93.8 cents); 16 peaks at Detune 6.0 (3.7 cents apart).
pub const SUPERSAW_VOICES: usize = 16;

/// Outer-voice offset in cents against the Detune knob `0..10` (`d`, `outer`).
///
/// EMPIRICAL: half the outer span of the voice comb, read at `ag_audio` windows of 0.7 to 1.4 s on
/// steady notes whose OLED value was stable: 1.4 -> 6.4, 3.0 -> 11.0, 6.0 -> 27.9, 10.0 -> 93.75
/// (5.8 -> about 26 while the knob was still moving). Accuracy about +/-1.5 cents below 6, +/-1 cent
/// at 10. Between the points the law is linear in `ln(outer)`; below 1.4 it is linear to 0 at 0
/// (ASSUMED: the 0.0 reading is resolution limited at +/-2 cents).
pub const SUPERSAW_OUTER_CENTS: [(f32, f32); 4] =
    [(1.4, 6.4), (3.0, 11.0), (6.0, 27.9), (10.0, 93.75)];

/// Octaves of cutoff sweep at full Filter Env Amt (`0..1` of the panel `0..10`).
///
/// EMPIRICAL: laser-zap clip, resonance peak 4.8 octaves above the displayed 415 Hz at Amt 7.7
/// (9.2 kHz at the first frame after the trigger) and 4.6 octaves above 468 Hz at Amt 7.4 in the
/// earlier read of the same clip: 0.62 octave per panel unit, 6.2 at 10. Linearity in Amt is
/// assumed (two nearby amounts).
pub const FILTER_ENV_OCTAVES_MEASURED: f32 = 6.2;

/// Time constant of the filter and oscillator envelope decay as a fraction of the displayed Decay.
///
/// EMPIRICAL: `1 / ln(100)`: the displayed time is the time to fall 40 dB. Pitch-blip clip: tau
/// 21.7 ms for 100 ms on three notes (0.217); laser-zap clip: tau 0.40 to 0.47 s for 2.07 s
/// (0.19 to 0.23).
pub const ENV_DECAY_TAU_FRACTION_MEASURED: f32 = 0.2171;

/// Octaves of pitch at full Osc Env Amt with Env To Pitch.
///
/// EMPIRICAL: pitch-blip clip, Amt +5.5: the exponential fit of the tracked pitch (same pipeline
/// on hardware and emulator) extrapolates to 14.9 and 13.1 semitones on two notes against 17.6 for
/// an emulator render with 2.21 octaves and 8.5 for the 1.0 octave default, so the true onset is
/// about 11.6 semitones: 0.97 octave at 0.55 -> 1.76 octaves at 1.0. One amount only; linearity
/// ASSUMED.
pub const OSC_ENV_PITCH_OCTAVES_MEASURED: f32 = 1.76;

/// Scale of the vibrato depth (cents) against the `VIBRATO_MAX_CENTS = 92` scale of `synth_params`.
///
/// EMPIRICAL: the same f0 pipeline reads 43.6 / 88.1 cents on an emulator render of 46 / 92 cents
/// (factor 0.949) and 46.5..47.5 / 92.5 cents on the hardware at Amount 5.0 / 10.0, so the hardware
/// peaks at 49.0 / 97.5 cents: 9.75 cents per panel unit, linear over the two amounts (rate is
/// the displayed value x 1.01).
pub const VIBRATO_DEPTH_SCALE_MEASURED: f32 = 1.06;

/// Outer-voice offset in cents at Detune `d` (panel `0..10`).
pub fn supersaw_outer_cents(d: f32) -> f32 {
    let d = d.clamp(0.0, 10.0);
    let t = &SUPERSAW_OUTER_CENTS;
    if d <= t[0].0 {
        return t[0].1 * d / t[0].0;
    }
    for w in t.windows(2) {
        let ((d0, c0), (d1, c1)) = (w[0], w[1]);
        if d <= d1 {
            let a = (d - d0) / (d1 - d0);
            return (c0.ln() * (1.0 - a) + c1.ln() * a).exp();
        }
    }
    t[t.len() - 1].1
}

/// Frequency ratio of Super Saw voice `i` (`0..16`) at Detune `d` (panel `0..10`).
pub fn supersaw_voice_ratio(i: usize, d: f32) -> f32 {
    let half = (SUPERSAW_VOICES as f32 - 1.0) * 0.5;
    let cents = (i as f32 - half) / half * supersaw_outer_cents(d);
    (cents / 1200.0).exp2()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outer_law_hits_the_measured_points_and_is_monotonic() {
        for (d, c) in SUPERSAW_OUTER_CENTS {
            assert!((supersaw_outer_cents(d) - c).abs() < 1e-3, "{d}");
        }
        assert_eq!(supersaw_outer_cents(0.0), 0.0);
        let mut prev = 0.0;
        for k in 1..=100 {
            let c = supersaw_outer_cents(k as f32 * 0.1);
            assert!(c > prev);
            prev = c;
        }
        assert!((supersaw_outer_cents(5.8) - 26.0).abs() < 2.0);
    }

    #[test]
    fn voices_are_symmetric_and_evenly_spaced_in_cents() {
        let d = 10.0;
        let c = |i: usize| 1200.0 * supersaw_voice_ratio(i, d).log2();
        assert!((c(0) + 93.75).abs() < 0.01 && (c(15) - 93.75).abs() < 0.01);
        for i in 0..15 {
            assert!((c(i + 1) - c(i) - 12.5).abs() < 0.01, "gap {i}");
        }
        assert!((c(7) + c(8)).abs() < 1e-3, "no centre voice");
    }

    #[test]
    fn scales_are_the_documented_values() {
        assert!((ENV_DECAY_TAU_FRACTION_MEASURED - 1.0 / 100f32.ln()).abs() < 1e-4);
        assert!((0.55 * OSC_ENV_PITCH_OCTAVES_MEASURED * 12.0 - 11.6).abs() < 0.1);
        assert!((0.74 * FILTER_ENV_OCTAVES_MEASURED - 4.6).abs() < 0.05);
    }

    fn render(measured: bool, detune: f32) -> Vec<f32> {
        use crate::synth::{SynthLayer, VaWaveform};
        let mut l = SynthLayer::new(48_000.0);
        l.waveform = VaWaveform::SuperSaw;
        l.supersaw_detune = detune;
        l.measured_laws = measured;
        l.note_on(67, 100);
        (0..4_800).map(|_| l.render_frame()[0]).collect()
    }

    #[test]
    fn default_render_is_untouched_and_the_switch_changes_the_super_saw() {
        use crate::synth::SynthLayer;
        assert!(!SynthLayer::new(48_000.0).measured_laws);
        let (a, b) = (render(false, 1.0), render(true, 1.0));
        assert!(a.iter().zip(&b).any(|(x, y)| (x - y).abs() > 1e-3));
        assert_eq!(a, render(false, 1.0), "default path is deterministic");
        assert!(b.iter().all(|v| v.is_finite()));
    }
}
