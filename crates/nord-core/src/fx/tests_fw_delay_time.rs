//! `FirmwareFx::delay_time` wiring: the first echo lands at the verified sample, the engine time
//! stays the default, tempo sync and a missing raw value fall back to the engine.

use super::*;

fn chain(fw: FirmwareFx) -> LayerFxChain {
    let mut fx = LayerFxChain::new(48_000.0);
    fx.reverb_model = ReverbModel::Off;
    fx.firmware_fx = fw;
    fx
}

/// Frame index of the loudest left output after a unit impulse (wet only, no feedback).
fn echo_index(fx: &mut LayerFxChain, frames: usize) -> usize {
    fx.delay_enabled = true;
    fx.delay_feedback = 0.0;
    fx.delay_mix = 1.0;
    fx.snap_delay_time(120.0);
    let out: Vec<f32> = (0..frames)
        .map(|i| fx.process_frame([f32::from(i == 0), 0.0])[0])
        .collect();
    (1..frames)
        .max_by(|&a, &b| out[a].abs().total_cmp(&out[b].abs()))
        .unwrap()
}

fn delay_time_only() -> FirmwareFx {
    FirmwareFx {
        delay_time: true,
        ..FirmwareFx::OFF
    }
}

#[test]
fn first_echo_is_c15_plus_two_samples() {
    // kDlyRateClkDiv 16383: c[0x15] = 868, echo 870; 1024: c[0x15] = 46498, echo 46500.
    for (raw, want) in [(16_383_u16, 870_usize), (1024, 46_500)] {
        let mut fx = chain(delay_time_only());
        fx.delay_rate_raw = Some(raw);
        assert_eq!(echo_index(&mut fx, want + 200), want, "raw {raw}");
    }
}

#[test]
fn switch_off_keeps_the_engine_time() {
    let mut fx = chain(FirmwareFx::OFF);
    fx.delay_rate_raw = Some(16_383);
    fx.delay_time_ms = 375.0;
    assert_eq!(echo_index(&mut fx, 20_000), 18_000);
}

#[test]
fn missing_raw_value_and_tempo_sync_fall_back_to_the_engine() {
    let mut a = chain(delay_time_only());
    a.delay_time_ms = 100.0;
    assert_eq!(echo_index(&mut a, 6000), 4800, "no raw value");
    let mut b = chain(FirmwareFx::ALL);
    b.delay_rate_raw = Some(16_383);
    b.delay_clock_sync = true;
    b.delay_sync_beats = Some(0.5);
    // 120 bpm, half a beat = 12000 samples (the engine's sync path, not the firmware raw value;
    // the slewed read position lands within a few samples)
    let echo = echo_index(&mut b, 30_000);
    assert!(echo.abs_diff(12_000) <= 8, "sync echo at {echo}");
}

#[test]
fn raw_value_is_clamped_to_the_14_bit_range() {
    let mut fx = chain(delay_time_only());
    fx.delay_rate_raw = Some(u16::MAX);
    assert_eq!(echo_index(&mut fx, 1200), 870);
}
