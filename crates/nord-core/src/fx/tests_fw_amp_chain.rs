//! `FirmwareFx::amp` on `LayerFxChain`: default unchanged, the firmware drive stage replaces the
//! engine drive and cabinet for the Twin / JC / Small families only, 4 frame latency.

use super::*;

fn chain(cabinet: AmpCabinet, variation: bool, fw: FirmwareFx) -> LayerFxChain {
    let mut fx = LayerFxChain::new(48_000.0);
    fx.reverb_model = ReverbModel::Off;
    fx.amp_cabinet = cabinet;
    fx.amp_variation = variation;
    fx.drive_amount = 0.11;
    fx.firmware_fx = fw;
    fx
}

fn render(fx: &mut LayerFxChain, n: usize) -> Vec<[f32; 2]> {
    (0..n)
        .map(|i| {
            let x = (i as f32 * 0.043).sin() * 0.3;
            fx.process_frame([x, -x])
        })
        .collect()
}

const AMP_ONLY: FirmwareFx = FirmwareFx {
    amp: true,
    ..FirmwareFx::OFF
};

#[test]
fn off_is_unchanged_and_on_replaces_the_twin_path() {
    let mut off_a = chain(AmpCabinet::Twin, false, FirmwareFx::OFF);
    let mut off_b = chain(AmpCabinet::Twin, false, FirmwareFx::OFF);
    assert_eq!(render(&mut off_a, 1500), render(&mut off_b, 1500));
    let mut on = chain(AmpCabinet::Twin, false, AMP_ONLY);
    let (legacy, fw) = (render(&mut off_a, 1500), render(&mut on, 1500));
    assert_ne!(legacy, fw);
    assert!(fw.iter().flatten().all(|v| v.is_finite()));
    assert!(
        fw[..4].iter().all(|f| *f == [0.0, 0.0]),
        "latency is one 4 frame block"
    );
    assert!(on.fw_amp.is_some());
}

#[test]
fn amps_without_a_drive_stage_keep_the_engine_path() {
    for cabinet in [
        AmpCabinet::Off,
        AmpCabinet::ToRotary,
        AmpCabinet::LpFilter,
        AmpCabinet::HpFilter,
    ] {
        let mut a = chain(cabinet, false, FirmwareFx::OFF);
        let mut b = chain(cabinet, false, AMP_ONLY);
        assert_eq!(render(&mut a, 600), render(&mut b, 600), "{cabinet:?}");
        assert!(b.fw_amp.is_none());
    }
}

#[test]
fn switching_the_type_or_the_switch_rebuilds_or_drops_the_stage() {
    let mut fx = chain(AmpCabinet::Twin, false, AMP_ONLY);
    let _ = render(&mut fx, 100);
    assert_eq!(fx.fw_amp.map(|s| s.kind()), Some(1));
    fx.amp_variation = true;
    let _ = render(&mut fx, 100);
    assert_eq!(fx.fw_amp.map(|s| s.kind()), Some(2));
    fx.firmware_fx = FirmwareFx::OFF;
    let _ = render(&mut fx, 4);
    assert!(fx.fw_amp.is_none());
}

#[test]
fn the_default_chain_runs_the_firmware_amp() {
    let mut fx = LayerFxChain::new(48_000.0);
    fx.reverb_model = ReverbModel::Off;
    fx.amp_cabinet = AmpCabinet::Twin;
    assert_eq!(fx.firmware_fx, FirmwareFx::DEFAULT);
    let _ = render(&mut fx, 50);
    assert_eq!(fx.fw_amp.map(|s| s.kind()), Some(1));
    fx.firmware_fx.amp = false;
    let _ = render(&mut fx, 50);
    assert!(
        fx.fw_amp.is_none(),
        "--amp-path engine restores the engine path"
    );
}
