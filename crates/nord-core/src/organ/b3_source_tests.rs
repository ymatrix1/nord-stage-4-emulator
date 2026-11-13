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

fn b3_path() -> OrganLayer {
    let mut o = layer(OrganModel::B3, ToneWheelSource::B3Wheels);
    o.percussion_enabled = false;
    o.rotary_enabled = false;
    o.key_click_enabled = false;
    o.drawbars = [8, 8, 8, 0, 0, 0, 0, 0, 0];
    o
}

#[test]
fn key_weights_replace_the_legacy_drawbar_gains_on_the_b3_wheels_path() {
    let mut o = b3_path();
    assert!(o.b3_weights_active());
    o.note_on(60);
    let peak = (0..24_000)
        .map(|_| o.render_frame()[0].abs())
        .fold(0.0f32, f32::max);
    // A * W of the three footages of C4 (0.294 * 0.0329 and neighbours) times the corrected
    // tail (about +8 dB at 65 to 330 Hz) gives a peak of about 0.07
    assert!(peak > 0.03 && peak < 0.15, "peak {peak}");
    // other models and B3 Bass mode keep their paths
    let mut bass = b3_path();
    bass.b3_bass_mode = true;
    assert!(!bass.b3_weights_active());
    assert!(!layer(OrganModel::B3, ToneWheelSource::Legacy).b3_weights_active());
}

#[test]
fn b3_keys_have_no_velocity_and_release_to_exact_silence() {
    let render = |gain: f32| {
        let mut o = b3_path();
        o.note_on_with_gain(64, gain);
        (0..9_600).map(|_| o.render_frame()[0]).collect::<Vec<_>>()
    };
    assert_eq!(render(0.2), render(1.0)); // VERIFIED negative: keydown-zones.md section 4
    let mut o = b3_path();
    o.note_on(64);
    for _ in 0..4_800 {
        o.render_frame();
    }
    o.note_off(64);
    let mut last = [1.0f32; 2];
    for _ in 0..4_800 {
        last = o.render_frame();
    }
    assert_eq!(last, [0.0, 0.0]);
}

#[test]
fn the_b3_chord_level_follows_a_times_w_not_the_legacy_bus_scale() {
    let rms = |source: ToneWheelSource| {
        let mut o = layer(OrganModel::B3, source);
        o.percussion_enabled = false;
        o.rotary_enabled = false;
        o.key_click_enabled = false;
        for n in [60u8, 64, 67] {
            o.note_on(n);
        }
        let e: f32 = (0..48_000)
            .map(|_| {
                let [l, _] = o.render_frame();
                l * l
            })
            .sum();
        (e / 48_000.0).sqrt()
    };
    // the legacy path sums full-scale drawbars at 0.08 each; the captured weights are lower
    assert!(rms(ToneWheelSource::B3Wheels) < 0.5 * rms(ToneWheelSource::Legacy));
}

/// Amplitude of the line at `hz` (Hann window, +-0.4 bin peak) of `x` rendered at `fs`.
fn line(x: &[f32], hz: f64, fs: f64) -> f64 {
    let n = x.len();
    let win: Vec<f64> = (0..n)
        .map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos())
        .collect();
    let norm: f64 = win.iter().sum();
    (-8..=8)
        .map(|d| {
            let f = hz + f64::from(d) * 0.05 * fs / n as f64;
            let (mut re, mut im) = (0.0, 0.0);
            for (i, (&v, &w)) in x.iter().zip(&win).enumerate() {
                let a = std::f64::consts::TAU * f * i as f64 / fs;
                re += f64::from(v) * w * a.cos();
                im += f64::from(v) * w * a.sin();
            }
            2.0 * re.hypot(im) / norm
        })
        .fold(0.0, f64::max)
}

#[test]
fn the_tail_puts_the_chord_lines_where_the_oracle_has_them() {
    // Oracle chord C4 E4 G4, real key-down words, 16' 5 1/3' 8' 4', `DM(0xB44C0) = 1.0`
    // (docs/research/organ-b3-tail.md section 4): line amplitudes of the SHARC pass.
    let fs = 43_402.343_75f64;
    let mut o = layer(OrganModel::B3, ToneWheelSource::B3Wheels);
    o.sample_rate = fs as f32;
    o.percussion_enabled = false;
    o.rotary_enabled = false;
    o.key_click_enabled = false;
    o.drawbars = [8, 8, 8, 8, 0, 0, 0, 0, 0];
    for n in [60u8, 64, 67] {
        o.note_on(n);
    }
    let x: Vec<f32> = (0..11_192).map(|_| o.render_frame()[0]).collect();
    let tail = &x[3_000..3_000 + 8_192];
    // captured keys only (60, 64): the lines of wheels 52, 60, 64, 71 carry no interpolated W
    for (note, want) in [
        (52usize, 0.029_19f64),
        (60, 0.021_19),
        (64, 0.019_79),
        (71, 0.018_64),
    ] {
        let got = line(tail, b3_note_hz(note), fs);
        let db = 20.0 * (got / want).log10();
        assert!(db.abs() < 1.0, "wheel {note}: {got} vs {want} = {db} dB");
    }
}
