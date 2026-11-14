//! `sample_pitch_fw`: the note-to-step ratio of the `SAMPLES` oscillator (opt-in firmware law).

use super::*;
use std::sync::Arc;

fn step(fw: bool, note: u8) -> f64 {
    let pcm: Vec<f32> = (0..512).map(|i| (i as f32 / 32.0).sin()).collect();
    let sound = Arc::new(SynthSampleSound::new_synthetic(
        "PitchLaw",
        60,
        &pcm,
        48_000.0,
        Some((128, 384)),
        16,
    ));
    let mut s = SynthLayer::new(48_000.0);
    s.osc_mode = SynthOscMode::Sample;
    s.sample_pitch_fw = fw;
    s.load_sample(Some(sound));
    s.note_on(note, 100);
    s.voices[0].sample_step
}

#[test]
fn firmware_pitch_law_is_off_by_default_and_exact_at_the_root_key() {
    assert!(!SynthLayer::new(48_000.0).sample_pitch_fw);
    assert!((step(false, 36) - 0.25).abs() < 1e-12);
    assert_eq!(step(true, 60), 1.0);
}

#[test]
fn firmware_pitch_law_differs_from_equal_temperament_by_under_two_tenths_of_a_percent() {
    let (eq, fw) = (step(false, 24), step(true, 24));
    assert!(eq != fw && (fw / eq - 1.0).abs() < 2e-4, "{eq} {fw}");
}
