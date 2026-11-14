//! `FirmwareFx::mod2` in the layer chain: the firmware stages replace the engine Mod 2 for
//! `kFx2Type` 0 to 7, 10, 11 (`PHAS`, `FLANG`, `CHOR`, `VIBE`, `SPIN`); `ENS` (8, 9) and the types
//! above 11 stay on the engine (the boot words complete `PHAS 1` and `SPIN`; the `ENS` output is
//! right only, `docs/research/fx-mod-types.md` s8).

use super::*;

fn chain(on: bool, ty: u8) -> LayerFxChain {
    let mut fx = LayerFxChain::new(48_000.0);
    fx.firmware_fx = FirmwareFx {
        mod2: on,
        ..FirmwareFx::OFF
    };
    fx.mod2_type = Mod2Type::Chorus;
    fx.mod2_type_raw = Some(ty);
    fx.mod2_rate_raw = Some(80);
    fx.mod2_amount = 0.8;
    fx
}

fn run(fx: &mut LayerFxChain, n: usize) -> Vec<[f32; 2]> {
    (0..n)
        .map(|i| {
            let x = (i as f32 * 0.07).sin() + 0.2;
            fx.process_mod2(x, 0.5 * x, 1.0 / 48_000.0)
        })
        .collect()
}

#[test]
fn the_firmware_stages_have_a_four_frame_delay_and_stay_bounded() {
    for ty in [0, 1, 2, 3, 4, 5, 6, 7, 10, 11] {
        let mut fx = chain(true, ty);
        let y = run(&mut fx, 20_000);
        assert!(y[..4].iter().all(|f| *f == [0.0, 0.0]), "type {ty}");
        assert!(y
            .iter()
            .all(|f| f.iter().all(|x| x.is_finite() && x.abs() < 8.0)));
        assert!(
            y[5_000..].iter().any(|f| f[0].abs() > 0.1),
            "type {ty} is not silent"
        );
    }
}

#[test]
fn the_engine_path_is_used_when_the_switch_is_off_or_the_type_is_not_a_firmware_one() {
    // The engine chorus has no 4-frame silence: its first frames already carry the dry signal.
    let first_frames_live =
        |fx: &mut LayerFxChain| run(fx, 3).iter().any(|f| f[0] != 0.0 || f[1] != 0.0);
    assert!(first_frames_live(&mut chain(false, 4)));
    for ty in [8, 9, 12] {
        assert!(
            first_frames_live(&mut chain(true, ty)),
            "type {ty} stays on the engine"
        );
    }
    let mut no_raw = chain(true, 4);
    no_raw.mod2_rate_raw = None;
    assert!(first_frames_live(&mut no_raw));
}

#[test]
fn changing_the_type_rebuilds_the_stage() {
    let mut fx = chain(true, 4);
    run(&mut fx, 16);
    fx.mod2_type_raw = Some(5);
    let y = run(&mut fx, 20_000);
    assert!(y[5_000..].iter().any(|f| f[0].abs() > 0.1));
}

#[test]
fn changing_between_the_law_stages_and_the_others_rebuilds() {
    let mut fx = chain(true, 2);
    run(&mut fx, 16);
    for ty in [3, 6, 4, 0, 2] {
        fx.mod2_type_raw = Some(ty);
        let y = run(&mut fx, 20_000);
        assert!(
            y[..4].iter().all(|f| *f == [0.0, 0.0]),
            "type {ty} restarts"
        );
        assert!(y[5_000..].iter().any(|f| f[0].abs() > 0.05), "type {ty}");
    }
}
