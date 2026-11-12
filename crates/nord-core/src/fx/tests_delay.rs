//! Delay stage tests: ping-pong, slew, stereo/mono variants, flam and space diffusion.

use super::support::*;
use super::*;

#[test]
fn test_delay_ping_pong_and_analog_pitch_slew() {
    let mut fx = LayerFxChain::new(48_000.0);
    fx.reverb_model = ReverbModel::Off;
    fx.delay_enabled = true;
    fx.delay_time_ms = 20.0; // 960 samples at 48 kHz
    fx.delay_feedback = 0.6;
    fx.delay_mix = 1.0;
    fx.delay_ping_pong = true;
    fx.delay_analog = false;

    // Prime impulse at frame 0 (skip input on subsequent frames)
    let mut echo1_l = 0.0f32;
    let mut echo1_r = 0.0f32;
    let mut echo2_l = 0.0f32;
    let mut echo2_r = 0.0f32;
    for i in 0..2400 {
        let inp = if i == 0 { [1.0, 1.0] } else { [0.0, 0.0] };
        let [l, r] = fx.process_frame(inp);
        if (940..985).contains(&i) {
            echo1_l += l.abs();
            echo1_r += r.abs();
        }
        if (1900..1950).contains(&i) {
            echo2_l += l.abs();
            echo2_r += r.abs();
        }
    }
    assert!(
        echo1_l > 0.5 && echo1_r < 1e-4,
        "first ping-pong tap hits Left first"
    );
    assert!(
        echo2_r > 0.25 && echo2_l < 1e-4,
        "second ping-pong tap bounces to Right"
    );

    // Verify Analog tape pitch-slew glides smoothly when delay time changes
    let mut analog = LayerFxChain::new(48_000.0);
    analog.reverb_model = ReverbModel::Off;
    analog.delay_enabled = true;
    analog.delay_analog = true;
    analog.delay_time_ms = 50.0;
    analog.snap_delay_time(120.0);
    analog.delay_mix = 0.8;
    let before = analog.delay_smoothed_samples;
    analog.delay_time_ms = 200.0;
    for _ in 0..480 {
        analog.process_frame([0.2, 0.2]);
    }
    let mid = analog.delay_smoothed_samples;
    assert!(
        mid > before && mid < 0.200 * 48_000.0,
        "analog delay must slew smoothly toward new target (before={before}, mid={mid})"
    );
}

#[test]
fn test_delay_fx_stereo_and_mono_variants() {
    // Baseline: mono source, no effect, no ping-pong -> identical channels
    let (base, _, _) = corr_after_delay_fx(DelayFeedbackFx::Off, false, true);
    assert!(base > 0.9999, "baseline must be mono, got {base}");
    // Chorus decorrelates a mono source (widening), variation at least as wide
    let (chor, _, _) = corr_after_delay_fx(DelayFeedbackFx::Chorus, false, true);
    let (chor_v, _, _) = corr_after_delay_fx(DelayFeedbackFx::Chorus, true, true);
    assert!(chor < 0.95, "chorus must widen: {chor}");
    assert!(chor_v < 0.95, "chorus variation must widen: {chor_v}");
    // Vibe: stereo version decorrelates, variation (mono) does not
    let (vibe, _, _) = corr_after_delay_fx(DelayFeedbackFx::Vibe, false, true);
    let (vibe_m, _, _) = corr_after_delay_fx(DelayFeedbackFx::Vibe, true, true);
    assert!(vibe < 0.95, "vibe stereo: {vibe}");
    assert!(vibe_m > 0.9999, "vibe variation is mono: {vibe_m}");
    // Ensemble: stereo version decorrelates, variation (mono) does not
    let (ens, _, _) = corr_after_delay_fx(DelayFeedbackFx::Ensemble, false, true);
    let (ens_m, _, _) = corr_after_delay_fx(DelayFeedbackFx::Ensemble, true, true);
    assert!(ens < 0.97, "ensemble stereo: {ens}");
    assert!(ens_m > 0.9999, "ensemble variation is mono: {ens_m}");
}

#[test]
fn test_delay_flam_offsets_right_channel_by_20_and_40_ms() {
    for (var, ms) in [(false, 20.0f32), (true, 40.0)] {
        let mut fx = LayerFxChain::new(48_000.0);
        fx.reverb_model = ReverbModel::Off;
        fx.delay_enabled = true;
        fx.delay_time_ms = 20.0; // first repeat at 960 samples
        fx.delay_feedback = 0.0;
        fx.delay_mix = 1.0;
        fx.delay_fx = DelayFeedbackFx::Flam;
        fx.delay_fx_variation = var;
        let (mut first_l, mut first_r) = (None, None);
        for i in 0..6000usize {
            let x = if i == 0 { 1.0 } else { 0.0 };
            let [l, r] = fx.process_frame([x, x]);
            if i > 100 && first_l.is_none() && l.abs() > 0.2 {
                first_l = Some(i);
            }
            if i > 100 && first_r.is_none() && r.abs() > 0.2 {
                first_r = Some(i);
            }
        }
        let shift = (first_r.unwrap() as f32 - first_l.unwrap() as f32) / 48.0;
        assert!((shift - ms).abs() < 1.0, "flam shift {shift} ms, want {ms}");
    }
}

#[test]
fn test_delay_space_is_stable_and_diffuses() {
    for var in [false, true] {
        let (corr, ll, rr) = corr_after_delay_fx(DelayFeedbackFx::Space, var, false);
        assert!(ll > 0.0 && rr > 0.0 && corr.abs() <= 1.0);
    }
    // impulse smears over time: more than one non-zero sample burst after the first repeat
    let mut fx = LayerFxChain::new(48_000.0);
    fx.reverb_model = ReverbModel::Off;
    fx.delay_enabled = true;
    fx.delay_time_ms = 20.0;
    fx.delay_feedback = 0.0;
    fx.delay_mix = 1.0;
    fx.delay_fx = DelayFeedbackFx::Space;
    fx.delay_fx_variation = true;
    let mut nonzero = 0;
    for i in 0..4000usize {
        let x = if i == 0 { 1.0 } else { 0.0 };
        let [l, _] = fx.process_frame([x, x]);
        if i > 900 && l.abs() > 1e-4 {
            nonzero += 1;
        }
    }
    assert!(
        nonzero > 20,
        "diffusion must smear the repeat, got {nonzero}"
    );
}
