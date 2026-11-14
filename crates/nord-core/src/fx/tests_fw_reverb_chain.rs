//! `FirmwareFx::reverb` in the layer chain: Spring types use the firmware routine, others the FDN.

use super::*;

fn chain(fw: FirmwareFx, model: ReverbModel, variation: bool, mix: f32) -> LayerFxChain {
    let mut fx = LayerFxChain::new(48_000.0);
    fx.reverb_model = model;
    fx.reverb_variation = variation;
    fx.reverb_mix = mix;
    fx.firmware_fx = fw;
    fx
}

fn on() -> FirmwareFx {
    FirmwareFx {
        reverb: true,
        ..FirmwareFx::OFF
    }
}

fn impulse_tail(fx: &mut LayerFxChain, n: usize) -> Vec<[f32; 2]> {
    (0..n)
        .map(|k| {
            let x = f32::from(k == 0);
            fx.process_reverb(x, x, 1.0 / 48_000.0)
        })
        .collect()
}

#[test]
fn spring_runs_the_firmware_routine_four_frames_late_and_mixes_by_the_amount_words() {
    let mut fx = chain(on(), ReverbModel::Spring, false, 0.5);
    let out = impulse_tail(&mut fx, 16);
    assert!(out[..4].iter().all(|f| *f == [0.0; 2]), "latency 4 frames");
    let ui = fw_map::unit_to_ui(0.5);
    assert!(
        (out[4][0] - fw_map::reverb_dry(ui)).abs() < 2e-3,
        "{:?}",
        out[4]
    );
    assert!(fx.fw_reverb.as_ref().is_some_and(|r| r.rvb_type() == 0));
}

#[test]
fn the_variation_flag_selects_firmware_type_1() {
    let mut fx = chain(on(), ReverbModel::Spring, true, 0.5);
    impulse_tail(&mut fx, 8);
    assert_eq!(fx.fw_reverb.as_ref().map(|r| r.rvb_type()), Some(1));
}

#[test]
fn other_models_and_the_default_keep_the_engine_fdn() {
    // Stage without the variation flag is type 6: no captured block, so the engine FDN stays
    let mut fdn = chain(on(), ReverbModel::Stage, false, 0.5);
    let mut base = chain(FirmwareFx::OFF, ReverbModel::Stage, false, 0.5);
    assert_eq!(impulse_tail(&mut fdn, 64), impulse_tail(&mut base, 64));
    assert!(fdn.fw_reverb.is_none() && fdn.fw_reverb_multi.is_none());
    let mut hall_off = chain(FirmwareFx::OFF, ReverbModel::Hall, false, 0.5);
    impulse_tail(&mut hall_off, 8);
    assert!(hall_off.fw_reverb_multi.is_none(), "opt-in");
    let mut spring_off = chain(FirmwareFx::OFF, ReverbModel::Spring, false, 0.5);
    impulse_tail(&mut spring_off, 8);
    assert!(
        spring_off.fw_reverb.is_none(),
        "the firmware reverb is opt-in"
    );
    assert!(!std::hint::black_box(FirmwareFx::DEFAULT).reverb);
}

#[test]
fn zero_amount_bypasses_and_the_tail_is_finite() {
    let mut fx = chain(on(), ReverbModel::Spring, false, 0.0);
    assert_eq!(fx.process_reverb(1.0, -1.0, 1.0 / 48_000.0), [1.0, -1.0]);
    let mut fx = chain(on(), ReverbModel::Spring, false, 1.0);
    let tail = impulse_tail(&mut fx, 20_000);
    assert!(tail
        .iter()
        .flatten()
        .all(|v| v.is_finite() && v.abs() < 4.0));
    assert!(
        tail[8_000..].iter().flatten().any(|v| v.abs() > 1e-6),
        "a tail after the taps"
    );
}

#[test]
fn the_other_models_run_the_multi_tap_routine_four_frames_late() {
    let mut fx = chain(on(), ReverbModel::Hall, false, 0.5);
    let out = impulse_tail(&mut fx, 16);
    assert!(out[..4].iter().all(|f| *f == [0.0; 2]), "latency 4 frames");
    assert_eq!(fx.fw_reverb_multi.as_ref().map(|r| r.rvb_type()), Some(8));
    assert!(fx.fw_reverb.is_none(), "not the Spring routine");
    for (model, var, t) in [
        (ReverbModel::Booth, false, 2),
        (ReverbModel::Room, true, 5),
        (ReverbModel::Stage, true, 7),
        (ReverbModel::Cathedral, true, 11),
    ] {
        assert_eq!(LayerFxChain::fw_multi_type(model, var), Some(t));
    }
    assert_eq!(LayerFxChain::fw_multi_type(ReverbModel::Stage, false), None);
    assert_eq!(LayerFxChain::fw_multi_type(ReverbModel::Spring, true), None);
}

#[test]
fn a_variation_change_retypes_the_instance_without_a_new_allocation() {
    let mut fx = chain(on(), ReverbModel::Hall, false, 0.5);
    impulse_tail(&mut fx, 8);
    let before = fx
        .fw_reverb_multi
        .as_deref()
        .map(|r| r as *const FwReverbMulti);
    fx.reverb_variation = true;
    impulse_tail(&mut fx, 8);
    let after = fx
        .fw_reverb_multi
        .as_deref()
        .map(|r| r as *const FwReverbMulti);
    assert_eq!(before, after);
    assert_eq!(fx.fw_reverb_multi.as_ref().map(|r| r.rvb_type()), Some(9));
}

#[test]
fn the_multi_tap_tail_is_finite_and_follows_the_taps() {
    let mut fx = chain(on(), ReverbModel::Cathedral, true, 1.0);
    let tail = impulse_tail(&mut fx, 30_000);
    assert!(tail
        .iter()
        .flatten()
        .all(|v| v.is_finite() && v.abs() < 4.0));
    assert!(
        tail[3_000..].iter().flatten().any(|v| v.abs() > 1e-6),
        "a tail after the taps"
    );
}
