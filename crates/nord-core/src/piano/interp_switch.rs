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
    assert_eq!(SampleInterp::default(), SampleInterp::SmpQ31Hw);
    assert_eq!(
        PianoLayer::new(48_000.0).sample_interp,
        SampleInterp::SmpQ31Hw
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

/// Integer-PCM sound: samples quantised to `y / 8192` and flagged as a CNSP decode.
fn render_integer(interp: SampleInterp, note: u8) -> Vec<f32> {
    let fs = 48_000.0f32;
    let pcm: Vec<f32> = (0..48_000)
        .map(|i| {
            let v = (i as f32 / fs * 440.0 * std::f32::consts::TAU).sin() * 0.5;
            (v * 8192.0).round() / 8192.0
        })
        .collect();
    let mut sound = NpnoSound::new_synthetic("t", 69, &pcm, fs);
    sound.strokes[0].integer_pcm = true;
    let mut piano = PianoLayer::new(fs);
    piano.string_resonance_enabled = false;
    piano.pedal_noise_enabled = false;
    piano.sample_interp = interp;
    piano.set_sound(Some(Arc::new(sound)));
    piano.note_on(note, 100);
    (0..2048).map(|_| piano.render_frame()[0]).collect()
}

#[test]
fn test_hw_mode_uses_the_integer_mac_only_for_integer_pcm() {
    // Float material (the synthetic fixture is not flagged): Hw falls back to the f32 kernel.
    assert_eq!(
        render(SampleInterp::SmpQ31Hw, 72),
        render(SampleInterp::SmpQ31, 72)
    );
    // Integer material on fractional steps: the integer MAC differs from the f32 path by less
    // than half a y unit per sample before the voice gain, so the two renders are close but not
    // identical.
    let hw = render_integer(SampleInterp::SmpQ31Hw, 72);
    let f = render_integer(SampleInterp::SmpQ31, 72);
    assert_ne!(hw, f);
    let diff = hw
        .iter()
        .zip(&f)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    let peak = hw.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(peak > 0.01 && diff < 1e-3, "peak {peak} diff {diff}");
    assert_eq!(
        hw,
        render_integer(SampleInterp::SmpQ31Hw, 72),
        "deterministic"
    );
}
