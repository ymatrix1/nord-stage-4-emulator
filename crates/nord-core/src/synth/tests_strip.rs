//! Render-level tests of `SynthChain::FirmwareStrip`.

use super::test_util::{energy, render_n};
use super::*;

fn layer(chain: SynthChain) -> SynthLayer {
    let mut s = SynthLayer::new(48_000.0);
    s.chain = chain;
    s.waveform = VaWaveform::Saw;
    s.attack_sec = 0.002;
    s.decay_sec = 45.0;
    s.sustain_level = 1.0;
    s.release_sec = 0.05;
    s.cutoff_hz = 3_000.0;
    s.resonance = 0.2;
    s
}

/// Share of the energy above `split_hz`, from a naive DFT bin sum of the left channel.
fn high_share(frames: &[[f32; 2]], split_hz: f32) -> f32 {
    let n = frames.len();
    let (mut hi, mut all) = (0.0f64, 0.0f64);
    for k in 1..n / 2 {
        let w = std::f64::consts::TAU * k as f64 / n as f64;
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (i, f) in frames.iter().enumerate() {
            let x = f64::from(f[0]);
            re += x * (w * i as f64).cos();
            im -= x * (w * i as f64).sin();
        }
        let p = re * re + im * im;
        all += p;
        if k as f32 * 48_000.0 / n as f32 > split_hz {
            hi += p;
        }
    }
    (hi / all.max(1e-30)) as f32
}

#[test]
fn strip_chain_plays_and_releases_to_silence() {
    let mut s = layer(SynthChain::FirmwareStrip);
    s.note_on(57, 100);
    let on = render_n(&mut s, 4_800);
    assert!(on.iter().all(|f| f[0].is_finite() && f[1].is_finite()));
    assert!(energy(&on) > 1e-3, "strip voice is audible");
    s.note_off(57);
    let _ = render_n(&mut s, 48_000);
    assert!(s.voices.iter().all(|v| !v.active), "released voice freed");
}

#[test]
fn strip_cutoff_follows_the_panel_value() {
    let mut dark = layer(SynthChain::FirmwareStrip);
    dark.cutoff_hz = 300.0;
    let mut bright = layer(SynthChain::FirmwareStrip);
    bright.cutoff_hz = 6_000.0;
    let mut h = [0.0f32; 2];
    for (i, s) in [&mut dark, &mut bright].into_iter().enumerate() {
        s.note_on(45, 100);
        let _ = render_n(s, 2_000);
        h[i] = high_share(&render_n(s, 1_024), 2_000.0);
    }
    assert!(h[1] > 5.0 * h[0], "bright {} vs dark {}", h[1], h[0]);
}

#[test]
fn strip_filter_matches_the_firmware_engine_when_the_cutoff_is_in_range() {
    // Same saw model aside, the strip cutoff law returns the panel cutoff inside its range, so the
    // filter is the `FilterEngine::Firmware` one; envelope and gain differ, so compare spectra.
    let mut strip = layer(SynthChain::FirmwareStrip);
    let mut fw = layer(SynthChain::Legacy);
    fw.filter_engine = FilterEngine::Firmware;
    fw.saw_model = SawModel::FirmwareSy;
    for s in [&mut strip, &mut fw] {
        // the `Lp4` family (the Ladder24 stage has its own model, see below)
        s.filter_model = FilterModel::Lp24;
        s.note_on(45, 100);
        let _ = render_n(s, 3_000);
    }
    let a = high_share(&render_n(&mut strip, 1_024), 1_500.0);
    let b = high_share(&render_n(&mut fw, 1_024), 1_500.0);
    assert!((a / b - 1.0).abs() < 0.1, "strip {a} vs firmware {b}");
}

#[test]
fn idle_strip_layer_renders_like_an_idle_legacy_layer() {
    let mut a = layer(SynthChain::FirmwareStrip);
    let mut b = layer(SynthChain::Legacy);
    for _ in 0..64 {
        assert_eq!(a.render_frame(), b.render_frame());
    }
}

#[test]
fn legacy_chain_is_unchanged_by_the_strip_state() {
    // Two default layers, one of which switched to strip and back, render identically.
    let mut a = layer(SynthChain::Legacy);
    let mut b = layer(SynthChain::FirmwareStrip);
    b.chain = SynthChain::Legacy;
    a.note_on(60, 100);
    b.note_on(60, 100);
    assert_eq!(render_n(&mut a, 2_000), render_n(&mut b, 2_000));
}

#[test]
fn sy_saw_fade_knee_is_in_hz_not_in_engine_increments() {
    use super::hw_saw::saw_wave;
    // 5.4 kHz at 48 kHz: the engine-rate fade is still 1, the SY-rate fade (5.0 kHz knee) is below.
    let inc = 5_400.0 / 48_000.0;
    let a = saw_wave(SawModel::Firmware, 0.25, inc, 1.0 / 48_000.0);
    let b = saw_wave(SawModel::FirmwareSy, 0.25, inc, 1.0 / 48_000.0);
    assert!(b.abs() < a.abs() * 0.95, "engine {a} sy {b}");
    // At the SY frame rate itself the two models are the same wave.
    let f = SY_FRAME_RATE_HZ;
    let inc = 3_000.0 / f;
    let a = saw_wave(SawModel::Firmware, 0.4, inc, 1.0 / f);
    let b = saw_wave(SawModel::FirmwareSy, 0.4, inc, 1.0 / f);
    assert!((a - b).abs() < 1e-6, "{a} vs {b}");
}

#[test]
fn strip_voice_level_follows_the_key_velocity_series() {
    let level = |on: bool, vel: u8| {
        let mut s = layer(SynthChain::FirmwareStrip);
        s.strip_key_velocity = on;
        s.note_on(57, vel);
        energy(&render_n(&mut s, 9_600)).sqrt()
    };
    let (soft, loud) = (level(true, 64), level(true, 127));
    // g(64) / g(127) = 32 / 127: far under the linear 0.5.
    assert!(soft / loud < 0.35, "series ratio {}", soft / loud);
    let (lin_soft, lin_loud) = (level(false, 64), level(false, 127));
    assert!(
        (lin_soft / lin_loud - 0.5).abs() < 0.05,
        "linear ratio {}",
        lin_soft / lin_loud
    );
}

#[test]
fn legacy_chain_ignores_the_key_velocity_series() {
    let rms = |on: bool| {
        let mut s = layer(SynthChain::Legacy);
        s.strip_key_velocity = on;
        s.note_on(57, 64);
        energy(&render_n(&mut s, 4_800))
    };
    assert_eq!(rms(true), rms(false));
}

#[test]
fn shape_stage_replaces_the_saw_placeholder_only_when_enabled() {
    let render = |shape: Option<f32>| {
        let mut s = layer(SynthChain::FirmwareStrip);
        s.waveform = VaWaveform::Triangle;
        s.osc_shape_category = true;
        s.strip_shape = shape;
        s.note_on(57, 100);
        render_n(&mut s, 2_400)
    };
    let saw = render(None);
    let stage = render(Some(0.25));
    assert!(stage.iter().all(|f| f[0].is_finite()));
    assert!(energy(&stage) > 1e-3);
    assert_ne!(saw, stage, "the stage changes the waveform");
    assert_eq!(render(None), saw, "the default stays the placeholder");
}

#[test]
fn stage_oscillators_play_only_with_the_strip_chain_and_the_opt_in() {
    use super::FwStage;
    for stage in [
        FwStage::Noise,
        FwStage::ShapeSquare,
        FwStage::Super,
        FwStage::Multi,
    ] {
        let render = |chain: SynthChain, on: bool| {
            let mut s = layer(chain);
            s.strip_stage = Some(stage);
            s.strip_stages = on;
            s.note_on(57, 100);
            render_n(&mut s, 2_400)
        };
        let base = render(SynthChain::FirmwareStrip, false);
        let staged = render(SynthChain::FirmwareStrip, true);
        assert!(staged.iter().all(|f| f[0].is_finite()), "{stage:?}");
        assert!(energy(&staged) > 1e-4, "{stage:?} is silent");
        assert_ne!(base, staged, "{stage:?} changes the waveform");
        assert_eq!(render(SynthChain::FirmwareStrip, false), base);
        // the legacy chain never reads the opt-in
        assert_eq!(
            render(SynthChain::Legacy, true),
            render(SynthChain::Legacy, false)
        );
    }
}

#[test]
fn table_type_factor_opens_lp12_and_band_pass_but_not_lp24() {
    // Same layer with the P table on (type factor) and with it off: the level changes with the
    // row gain, so compare the share of energy above the cutoff, which is level free.
    let share = |model: FilterModel, table: bool| {
        let mut s = layer(SynthChain::FirmwareStrip);
        s.p_table = table.then_some(PTable::FITTED);
        s.p_wave = PWave::Other; // idle row: only the type factor acts
        s.filter_model = model;
        s.cutoff_hz = 800.0;
        s.note_on(45, 100);
        let _ = render_n(&mut s, 2_000);
        high_share(&render_n(&mut s, 1_024), 1_500.0)
    };
    assert!(share(FilterModel::Lp12, true) > 1.2 * share(FilterModel::Lp12, false));
    assert!(share(FilterModel::BandPass, true) != share(FilterModel::BandPass, false));
    let (a, b) = (
        share(FilterModel::Lp24, true),
        share(FilterModel::Lp24, false),
    );
    assert!((a / b - 1.0).abs() < 1e-4, "Lp24 factor is 1: {a} vs {b}");
}

#[test]
fn ladder24_runs_the_firmware_ladder_stage_only_with_the_strip_chain_and_the_opt_in() {
    let render = |chain: SynthChain, on: bool| {
        let mut s = layer(chain);
        s.filter_model = FilterModel::Ladder24;
        s.strip_stages = on;
        s.note_on(57, 100);
        render_n(&mut s, 2_400)
    };
    let base = render(SynthChain::FirmwareStrip, false);
    let staged = render(SynthChain::FirmwareStrip, true);
    assert!(staged.iter().all(|f| f[0].is_finite() && f[0].abs() < 8.0));
    assert!(energy(&staged) > 1e-6, "ladder stage is silent");
    assert_ne!(base, staged, "the Ladder24 stage replaces the assumed Lp4");
    assert_eq!(render(SynthChain::FirmwareStrip, false), base);
    assert_eq!(
        render(SynthChain::Legacy, true),
        render(SynthChain::Legacy, false),
        "the legacy chain never reads the opt-in"
    );
}
