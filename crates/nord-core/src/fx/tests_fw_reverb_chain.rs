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
    let mut fdn = chain(on(), ReverbModel::Hall, false, 0.5);
    let mut base = chain(FirmwareFx::OFF, ReverbModel::Hall, false, 0.5);
    assert_eq!(impulse_tail(&mut fdn, 64), impulse_tail(&mut base, 64));
    assert!(fdn.fw_reverb.is_none());
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
