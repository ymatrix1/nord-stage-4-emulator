//! `FirmwareFx::mod1` in the layer chain: the A-PAN / TREM firmware stages replace the engine LFO
//! models only for `kFx1Type` 0..=3 in free running mode.

use super::*;

fn chain(on: bool, ty: u8) -> LayerFxChain {
    let mut fx = LayerFxChain::new(48_000.0);
    fx.firmware_fx = FirmwareFx {
        mod1: on,
        ..FirmwareFx::OFF
    };
    fx.mod1_type = match ty {
        0 | 1 => Mod1Type::AutoPan,
        2 | 3 => Mod1Type::Tremolo,
        4 | 5 => Mod1Type::RingMod,
        6 | 7 => Mod1Type::AutoWah,
        8 | 9 => Mod1Type::Wah,
        _ => Mod1Type::Pump,
    };
    fx.mod1_type_raw = Some(ty);
    fx.mod1_rate_raw = Some(100);
    fx.mod1_amount = 1.0;
    fx.mod1_rate_hz = 3.0;
    fx
}

fn run(fx: &mut LayerFxChain, n: usize) -> Vec<[f32; 2]> {
    (0..n)
        .map(|_| fx.process_mod1(1.0, 1.0, 1.0 / 48_000.0))
        .collect()
}

#[test]
fn the_firmware_auto_pan_pans_with_opposite_channel_gains_and_a_four_frame_delay() {
    let mut fx = chain(true, 0);
    let y = run(&mut fx, 48_000);
    assert!(y[..4].iter().all(|f| *f == [0.0, 0.0]));
    let tail = &y[4_000..];
    assert!(tail.iter().all(|f| (f[0] + f[1] - 1.0).abs() < 1.0e-3));
    assert!(tail.iter().any(|f| f[0] > 0.9) && tail.iter().any(|f| f[0] < 0.1));
}

#[test]
fn the_engine_path_is_used_when_the_switch_is_off_or_the_type_is_not_a_firmware_one() {
    let mut off = chain(false, 0);
    assert_ne!(
        run(&mut off, 8)[0],
        [0.0, 0.0],
        "engine path has no latency"
    );
    for ty in [4, 8, 10] {
        let mut fx = chain(false, ty);
        assert_ne!(
            run(&mut fx, 16)[8],
            [0.0, 0.0],
            "type {ty} stays on the engine with the switch off"
        );
    }
}

#[test]
fn the_rest_of_the_mod1_types_run_the_firmware_stages_with_a_two_frame_delay() {
    for ty in 4..=11 {
        let mut fx = chain(true, ty);
        let y = run(&mut fx, 6_000);
        assert!(y[..2].iter().all(|f| *f == [0.0, 0.0]), "type {ty}");
        assert!(y[2..].iter().any(|f| f[0] != 0.0), "type {ty} is silent");
        assert!(
            y.iter()
                .all(|f| f.iter().all(|x| x.is_finite() && x.abs() < 16.0)),
            "type {ty}"
        );
    }
}

#[test]
fn the_rest_stages_fall_back_in_sync_and_the_ped_wah_follows_the_pedal() {
    let mut sync = chain(true, 5);
    sync.mod1_sync_beats = Some(1.0);
    assert_ne!(
        run(&mut sync, 16)[8],
        [0.0, 0.0],
        "Master Clock stays on the engine"
    );
    let at = |pos: f32| {
        let mut fx = chain(true, 9);
        fx.mod1_ped = true;
        fx.mod1_ped_pos = pos;
        run(&mut fx, 4_000)
    };
    assert_ne!(at(0.1), at(0.9), "the pedal position moves the wah");
}

#[test]
fn master_clock_sync_and_missing_raw_values_keep_the_engine_path() {
    let mut sync = chain(true, 2);
    sync.mod1_sync_beats = Some(1.0);
    assert_ne!(run(&mut sync, 8)[0], [0.0, 0.0]);
    let mut no_raw = chain(true, 2);
    no_raw.mod1_rate_raw = None;
    assert_ne!(run(&mut no_raw, 8)[0], [0.0, 0.0]);
}

#[test]
fn the_firmware_tremolo_peaks_at_unity_and_is_mono() {
    let mut fx = chain(true, 2);
    let y = run(&mut fx, 48_000);
    let tail = &y[4_000..];
    let hi = tail.iter().map(|f| f[0]).fold(f32::MIN, f32::max);
    let lo = tail.iter().map(|f| f[0]).fold(f32::MAX, f32::min);
    assert!(hi > 0.95 && hi < 1.01 && lo < 0.1, "{lo} {hi}");
    assert!(tail.iter().all(|f| f[0] == f[1]));
}

#[test]
fn changing_the_type_rebuilds_the_stage() {
    let mut fx = chain(true, 0);
    run(&mut fx, 16);
    fx.mod1_type = Mod1Type::Tremolo;
    fx.mod1_type_raw = Some(3);
    let y = run(&mut fx, 4_000);
    assert!(y[3_000..].iter().all(|f| f[0] == f[1]), "tremolo is mono");
}
