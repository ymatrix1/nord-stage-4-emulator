//! `FirmwareFx::reverb_mix`: the dry path follows the `kRvbAmt` equal-power word.

use super::*;

fn first_frame(fw: FirmwareFx, mix: f32) -> [f32; 2] {
    let mut fx = LayerFxChain::new(48_000.0);
    fx.reverb_model = ReverbModel::Room;
    fx.reverb_mix = mix;
    fx.firmware_fx = fw;
    fx.process_reverb(1.0, -1.0, 1.0 / 48_000.0)
}

fn on() -> FirmwareFx {
    FirmwareFx {
        reverb_mix: true,
        ..FirmwareFx::OFF
    }
}

#[test]
fn firmware_mix_replaces_the_engine_dry_law() {
    let ui = fw_map::unit_to_ui(0.5);
    let [l, r] = first_frame(on(), 0.5);
    assert!((l - fw_map::reverb_dry(ui)).abs() < 1e-6 && (r + l).abs() < 1e-6);
    let [l_engine, _] = first_frame(FirmwareFx::OFF, 0.5);
    assert!((l_engine - 0.89).abs() < 1e-6, "engine law 1 - 0.22 mix");
    assert!(l != l_engine);
}

#[test]
fn full_mix_removes_the_dry_signal_and_zero_mix_is_untouched() {
    assert!(first_frame(on(), 1.0)[0].abs() < 1e-6);
    assert_eq!(first_frame(on(), 0.0), [1.0, -1.0]);
}
