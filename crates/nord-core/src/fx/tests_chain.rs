//! FX chain integration tests: stability, morph, EQ/amp response, mod and reverb colours.

use super::support::*;
use super::*;

#[test]
fn test_layer_fx_chain_and_master_bus_stability() {
    let mut fx = LayerFxChain::new(48_000.0);
    fx.mod1_type = Mod1Type::AutoPan;
    fx.mod2_type = Mod2Type::Chorus;
    fx.drive_amount = 0.5;
    fx.amp_cabinet = AmpCabinet::Jc;
    fx.comp_enabled = true;
    fx.comp_amount = 0.4;
    fx.delay_enabled = true;
    fx.reverb_model = ReverbModel::Hall;

    let mut master = MasterBus::default();
    let mut clock = MasterClock::default();

    for i in 0..256 {
        clock.advance(48_000.0);
        let x = ((i as f32) * 0.1).sin() * 0.5;
        let [l, r] = fx.process_frame_with_bpm([x, x], clock.bpm);
        let [ml, mr] = master.process_frame([l, r]);
        assert!(ml.is_finite() && mr.is_finite());
        assert!(ml.abs() <= 1.0 && mr.abs() <= 1.0);
    }
    assert!(clock.beat_phase > 0.0);
}

#[test]
fn test_delay_feedback_filters_wah_mod2_and_reverb_colors() {
    // Verify LP feedback filter damps high-frequency square-wave echoes relative to Off
    let mut d_off = LayerFxChain::new(48_000.0);
    d_off.reverb_model = ReverbModel::Off;
    d_off.delay_enabled = true;
    d_off.delay_time_ms = 12.0;
    d_off.delay_feedback = 0.7;
    d_off.delay_mix = 1.0;

    let mut d_lp = d_off.clone();
    d_lp.delay_filter = DelayFilterMode::Lp;

    let mut rms_off = 0.0f32;
    let mut rms_lp = 0.0f32;
    for i in 0..4000 {
        let hi = if i < 200 {
            if i % 2 == 0 {
                0.8
            } else {
                -0.8
            }
        } else {
            0.0
        };
        let [lo, _] = d_off.process_frame([hi, hi]);
        let [ll, _] = d_lp.process_frame([hi, hi]);
        if i > 600 {
            rms_off += lo * lo;
            rms_lp += ll * ll;
        }
    }
    assert!(
        rms_lp < rms_off * 0.5,
        "LP feedback filter must attenuate Nyquist echo content"
    );

    // Exercise Wah, Phaser, Flanger, Spring & Chorale reverb
    for (m1, m2, rev, col) in [
        (
            Mod1Type::Wah,
            Mod2Type::Phaser,
            ReverbModel::Spring,
            ReverbColor::Bright,
        ),
        (
            Mod1Type::RingMod,
            Mod2Type::Flanger,
            ReverbModel::Hall,
            ReverbColor::Dark,
        ),
        (
            Mod1Type::Tremolo,
            Mod2Type::Vibe,
            ReverbModel::Cathedral,
            ReverbColor::Chorale,
        ),
    ] {
        let mut fx = LayerFxChain::new(48_000.0);
        fx.mod1_type = m1;
        fx.mod2_type = m2;
        fx.amp_cabinet = AmpCabinet::Twin;
        fx.comp_enabled = true;
        fx.comp_amount = 0.6;
        fx.comp_fast = true;
        fx.reverb_model = rev;
        fx.reverb_color = col;
        fx.reverb_mix = 0.4;
        for i in 0..512 {
            let x = ((i as f32) * 0.05).sin() * 0.4;
            let [l, r] = fx.process_frame_with_bpm([x, x], 130.0);
            assert!(l.is_finite() && r.is_finite());
        }
    }
}

#[test]
fn test_apply_morph_drives_only_assigned_params() {
    let mut fx = LayerFxChain::new(48_000.0);
    fx.reverb_mix = 0.3;
    fx.delay_mix = 0.2;
    fx.morph.reverb_amount = MorphParameter::new(0.2);
    fx.morph.reverb_amount.delta_wheel = 0.5;
    fx.morph.eq_mid_freq = MorphParameter::new(0.0);
    fx.morph.eq_mid_freq.delta_aftertouch = 1.0;
    let ctrl = MorphControllers {
        mod_wheel: 1.0,
        control_pedal: 0.0,
        aftertouch: 64.0 / 127.0,
    };
    fx.apply_morph(&ctrl);
    assert!((fx.reverb_mix - 0.7).abs() < 1e-5);
    assert!((fx.eq_mid_freq_hz - EQ_FREQ_HZ[64]).abs() < 1e-3);
    assert!(
        (fx.delay_mix - 0.2).abs() < 1e-6,
        "un-morphed param untouched"
    );
}

#[test]
fn test_eq_mid_freq_morph_moves_peak() {
    let mut fx = eq_only(0.0, 12.0, 200.0, 0.0);
    fx.morph.eq_mid_freq = MorphParameter::new(0.0);
    fx.morph.eq_mid_freq.delta_pedal = 1.0;
    fx.apply_morph(&MorphControllers::default());
    let low = tone_gain_db(&mut fx, 200.0);
    fx.apply_morph(&MorphControllers {
        control_pedal: 1.0,
        ..MorphControllers::default()
    });
    let moved = tone_gain_db(&mut fx, 200.0);
    assert!(low > 8.0 && moved < 2.0, "low={low} moved={moved}");
}

#[test]
fn test_lp24_hp24_slope_and_passband() {
    // LP24 @ 1 kHz: passband ~flat, 2 kHz (1 oct) < -18 dB, 4 kHz (2 oct) < -36 dB
    let mut fx = filter_chain(AmpCabinet::LpFilter, 1000.0, 0.0);
    assert!(tone_gain_db(&mut fx, 100.0).abs() < 1.0);
    let mut fx = filter_chain(AmpCabinet::LpFilter, 1000.0, 0.0);
    assert!(tone_gain_db(&mut fx, 2000.0) < -18.0);
    let mut fx = filter_chain(AmpCabinet::LpFilter, 1000.0, 0.0);
    assert!(tone_gain_db(&mut fx, 4000.0) < -36.0);
    // HP24 mirror image
    let mut fx = filter_chain(AmpCabinet::HpFilter, 1000.0, 0.0);
    assert!(tone_gain_db(&mut fx, 10_000.0).abs() < 1.0);
    let mut fx = filter_chain(AmpCabinet::HpFilter, 1000.0, 0.0);
    assert!(tone_gain_db(&mut fx, 250.0) < -36.0);
    // Resonance raises the level near cutoff
    let mut flat = filter_chain(AmpCabinet::LpFilter, 1000.0, 0.0);
    let mut res = filter_chain(AmpCabinet::LpFilter, 1000.0, 10.0);
    assert!(tone_gain_db(&mut res, 1000.0) > tone_gain_db(&mut flat, 1000.0) + 3.0);
}

#[test]
fn test_filter_mode_ignores_mid_gain_peak() {
    let mut a = filter_chain(AmpCabinet::LpFilter, 8000.0, 0.0);
    a.eq_mid_db = 12.0;
    let mut b = filter_chain(AmpCabinet::LpFilter, 8000.0, 0.0);
    let ga = tone_gain_db(&mut a, 1000.0);
    let gb = tone_gain_db(&mut b, 1000.0);
    assert!(
        (ga - gb).abs() < 0.1,
        "mid knob is resonance here, not a peak"
    );
}

#[test]
fn test_cabinets_follow_manual_treble_direction() {
    let twin = treble_tilt_db(AmpCabinet::Twin, false);
    let twin_v = treble_tilt_db(AmpCabinet::Twin, true);
    let jc = treble_tilt_db(AmpCabinet::Jc, false);
    let jc_v = treble_tilt_db(AmpCabinet::Jc, true);
    let small = treble_tilt_db(AmpCabinet::Small, false);
    let small_v = treble_tilt_db(AmpCabinet::Small, true);
    // Twin and JC boost the treble; variations boost less (Twin) or reduce it (JC)
    assert!(twin > 3.0 && jc > 3.0, "twin {twin} jc {jc}");
    assert!(twin_v < twin - 2.0, "twin var {twin_v} vs {twin}");
    assert!(jc_v < jc - 6.0 && jc_v < 0.0, "jc var {jc_v} vs {jc}");
    // Small is a small speaker (rolls off the treble); variation is smaller still
    assert!(
        small < 0.0 && small_v < small - 1.0,
        "small {small} var {small_v}"
    );
}

#[test]
fn test_cabinet_variation_switch_redesigns_filters() {
    let mut fx = cab_chain(AmpCabinet::Twin, false);
    let a = tone_gain_db(&mut fx, 6000.0);
    fx.amp_variation = true;
    let b = tone_gain_db(&mut fx, 6000.0);
    assert!(b < a - 2.0, "variation must lower the treble: {a} -> {b}");
}
