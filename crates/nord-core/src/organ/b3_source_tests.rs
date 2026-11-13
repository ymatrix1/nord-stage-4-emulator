//! `ToneWheelSource::B3Wheels` wired into the organ layer (resolution, pitch, Fine Tune, state).

use super::{b3_note_hz, OrganLayer, OrganModel, ToneWheelSource};

const FS: f32 = 48_000.0;

fn layer(model: OrganModel, source: ToneWheelSource) -> OrganLayer {
    let mut o = OrganLayer::new(FS);
    o.model = model;
    o.tonewheel_source = source;
    o.crosstalk_level = 0.0;
    o
}

/// Rising zero crossings of wheel `w` over one second.
fn crossings(o: &mut OrganLayer, w: usize) -> usize {
    let mut prev = o.render_tonewheels(1.0 / FS)[w];
    let mut n = 0;
    for _ in 0..FS as usize {
        let cur = o.render_tonewheels(1.0 / FS)[w];
        if prev < 0.0 && cur >= 0.0 {
            n += 1;
        }
        prev = cur;
    }
    n
}

#[test]
fn b3_wheels_applies_to_the_b3_model_only() {
    use OrganModel::{Farfisa, Pipe1, Pipe2, Vox, B3};
    assert_eq!(
        ToneWheelSource::B3Wheels.resolve(B3),
        ToneWheelSource::B3Wheels
    );
    for m in [Vox, Farfisa, Pipe1, Pipe2] {
        assert_eq!(
            ToneWheelSource::B3Wheels.resolve(m),
            ToneWheelSource::Legacy
        );
    }
    // the default of the B3 model is untouched (opt-in source)
    assert_eq!(
        ToneWheelSource::ModelDefault.resolve(B3),
        ToneWheelSource::Legacy
    );
}

#[test]
fn wheel_pitch_follows_the_firmware_ladder() {
    let mut o = layer(OrganModel::B3, ToneWheelSource::B3Wheels);
    // wheel 36 = MIDI 60 (C4, 261.63 Hz), wheel 43 = MIDI 67 (G4, ladder 392.0 Hz)
    let c = crossings(&mut o, 36) as f64;
    assert!((c - b3_note_hz(60)).abs() <= 1.0, "C4 {c}");
    let mut o = layer(OrganModel::B3, ToneWheelSource::B3Wheels);
    let g = crossings(&mut o, 43) as f64;
    assert!((g - b3_note_hz(67)).abs() <= 1.0, "G4 {g}");
    // the ladder's fifth is 2^(7/12) = 1.4983 within 0.4 cent, not the 1.5 of a pure fifth
    assert!((b3_note_hz(67) / b3_note_hz(60) / 1.498_307 - 1.0).abs() < 3e-4);
}

#[test]
fn fine_tune_moves_the_wheels() {
    let mut up = layer(OrganModel::B3, ToneWheelSource::B3Wheels);
    up.fine_tune_cents = 100.0;
    let c = crossings(&mut up, 36) as f64;
    assert!((c - b3_note_hz(61)).abs() < 2.0, "{c}");
}

#[test]
fn renders_differently_from_legacy_and_is_deterministic() {
    let render = |source: ToneWheelSource| {
        let mut o = layer(OrganModel::B3, source);
        o.rotary_enabled = false;
        o.note_on(60);
        (0..4_000)
            .map(|_| o.render_frame()[0])
            .collect::<Vec<f32>>()
    };
    let a = render(ToneWheelSource::B3Wheels);
    assert_eq!(a, render(ToneWheelSource::B3Wheels));
    let legacy = render(ToneWheelSource::Legacy);
    assert!(a.iter().all(|v| v.is_finite()));
    assert!(a.iter().map(|v| v.abs()).fold(0.0f32, f32::max) > 1e-3);
    assert!(a.iter().zip(&legacy).any(|(x, y)| (x - y).abs() > 1e-4));
}

#[test]
fn the_source_falls_back_to_legacy_on_other_models() {
    let mut vox = layer(OrganModel::Vox, ToneWheelSource::B3Wheels);
    let mut legacy = layer(OrganModel::Vox, ToneWheelSource::Legacy);
    for _ in 0..500 {
        assert_eq!(
            vox.render_tonewheels(1.0 / FS),
            legacy.render_tonewheels(1.0 / FS)
        );
    }
}
