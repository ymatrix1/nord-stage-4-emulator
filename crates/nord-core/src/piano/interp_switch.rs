//! Tests of the sample-interpolation switch of `PianoLayer` (`SampleInterp`).
//!
//! Spec: `docs/specs/piano-engine.md` ("SMP sample interpolation").

use super::{NpnoSound, PianoLayer, SampleInterp};
use std::sync::Arc;

fn render(interp: SampleInterp, note: u8) -> Vec<f32> {
    let fs = 48_000.0f32;
    let pcm: Vec<f32> = (0..48_000)
        .map(|i| (i as f32 / fs * 440.0 * std::f32::consts::TAU).sin() * 0.5)
        .collect();
    let mut piano = PianoLayer::new(fs);
    piano.string_resonance_enabled = false;
    piano.pedal_noise_enabled = false;
    piano.sample_interp = interp;
    piano.set_sound(Some(Arc::new(NpnoSound::new_synthetic("t", 69, &pcm, fs))));
    piano.note_on(note, 100);
    (0..2048).map(|_| piano.render_frame()[0]).collect()
}

#[test]
fn test_default_is_hardware_parity_and_modes_differ_slightly() {
    assert_eq!(SampleInterp::default(), SampleInterp::SmpQ31);
    assert_eq!(
        PianoLayer::new(48_000.0).sample_interp,
        SampleInterp::SmpQ31
    );
    // Root note (integer steps) and a transposed note (fractional steps).
    for note in [69u8, 72] {
        let a = render(SampleInterp::SmpQ31, note);
        let b = render(SampleInterp::Cubic, note);
        assert!(a.iter().chain(&b).all(|v| v.is_finite()));
        let diff = a
            .iter()
            .zip(&b)
            .map(|(x, y)| (x - y).abs())
            .fold(0.0f32, f32::max);
        let peak = a.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.01, "note {note} renders audio");
        // Same signal, different interpolation error: below 1 % of full scale at 440 Hz.
        assert!(diff < 0.01, "note {note}: {diff}");
    }
    // Deterministic offline rendering.
    assert_eq!(
        render(SampleInterp::SmpQ31, 72),
        render(SampleInterp::SmpQ31, 72)
    );
}

#[test]
fn test_cubic_mode_still_selectable_and_differs_on_fractional_steps() {
    let a = render(SampleInterp::SmpQ31, 72);
    let b = render(SampleInterp::Cubic, 72);
    assert_ne!(a, b);
}
