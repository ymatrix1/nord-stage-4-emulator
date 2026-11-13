//! `ToneWheelSource::FarfisaWheels` wired into the organ layer (resolution, pitch, state, default).

use super::{OrganLayer, OrganModel, ToneWheelSource};

const FS: f32 = 48_000.0;

fn layer(model: OrganModel, source: ToneWheelSource) -> OrganLayer {
    let mut o = OrganLayer::new(FS);
    o.model = model;
    o.tonewheel_source = source;
    o.crosstalk_level = 0.0;
    o
}

/// Rising zero crossings of wheel `w` over two seconds, per second.
fn crossings(o: &mut OrganLayer, w: usize) -> f64 {
    let mut prev = o.render_tonewheels(1.0 / FS)[w];
    let mut n = 0u32;
    for _ in 0..2 * FS as usize {
        let cur = o.render_tonewheels(1.0 / FS)[w];
        if prev < 0.0 && cur >= 0.0 {
            n += 1;
        }
        prev = cur;
    }
    f64::from(n) / 2.0
}

#[test]
fn farfisa_wheels_applies_to_the_farfisa_model_only() {
    use OrganModel::{Farfisa, Pipe1, Pipe2, Vox, B3};
    assert_eq!(
        ToneWheelSource::FarfisaWheels.resolve(Farfisa),
        ToneWheelSource::FarfisaWheels
    );
    for m in [B3, Vox] {
        assert_eq!(
            ToneWheelSource::FarfisaWheels.resolve(m),
            ToneWheelSource::Legacy
        );
    }
    for m in [Pipe1, Pipe2] {
        assert_eq!(
            ToneWheelSource::FarfisaWheels.resolve(m),
            ToneWheelSource::Firmware
        );
    }
    // the default of the Farfisa model is untouched (opt-in source)
    assert_eq!(
        ToneWheelSource::ModelDefault.resolve(Farfisa),
        ToneWheelSource::Legacy
    );
}

#[test]
fn wheel_pitch_is_twelve_tet_at_the_host_rate() {
    // wheel 36 = note 60 = C4, wheel 45 = note 69 = A4 (440 Hz)
    let mut o = layer(OrganModel::Farfisa, ToneWheelSource::FarfisaWheels);
    for (w, hz) in [(36usize, 261.6256f64), (45, 440.0)] {
        let got = crossings(&mut o, w);
        assert!((got - hz).abs() < 1.0, "wheel {w}: {got} vs {hz}");
    }
}

#[test]
fn fine_tune_follows_the_layer_setting() {
    let mut a = layer(OrganModel::Farfisa, ToneWheelSource::FarfisaWheels);
    let mut b = layer(OrganModel::Farfisa, ToneWheelSource::FarfisaWheels);
    b.fine_tune_cents = 100.0;
    let (fa, fb) = (crossings(&mut a, 36), crossings(&mut b, 36));
    assert!((1200.0 * (fb / fa).log2() - 100.0).abs() < 2.0, "{fa} {fb}");
}

#[test]
fn farfisa_source_differs_from_legacy_and_is_deterministic() {
    let render = |source: ToneWheelSource| {
        let mut o = layer(OrganModel::Farfisa, source);
        (0..4000)
            .map(|_| o.render_tonewheels(1.0 / FS)[40])
            .collect::<Vec<f32>>()
    };
    let a = render(ToneWheelSource::FarfisaWheels);
    assert_eq!(a, render(ToneWheelSource::FarfisaWheels));
    assert_ne!(a, render(ToneWheelSource::Legacy));
    // the top of the bus has no divider note
    let mut o = layer(OrganModel::Farfisa, ToneWheelSource::FarfisaWheels);
    for _ in 0..500 {
        assert_eq!(o.render_tonewheels(1.0 / FS)[90], 0.0);
    }
}

#[test]
fn other_models_are_unchanged_by_the_new_source() {
    // selecting it on B3 falls back to Legacy: identical to an explicit Legacy layer
    let render = |source: ToneWheelSource| {
        let mut o = layer(OrganModel::B3, source);
        (0..1000)
            .map(|_| o.render_tonewheels(1.0 / FS)[20])
            .collect::<Vec<f32>>()
    };
    assert_eq!(
        render(ToneWheelSource::FarfisaWheels),
        render(ToneWheelSource::Legacy)
    );
}
