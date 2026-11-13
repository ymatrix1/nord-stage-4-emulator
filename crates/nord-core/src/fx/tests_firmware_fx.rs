//! The opt-in `FirmwareFx` switch on `LayerFxChain`: default unchanged, delay mix law, firmware
//! compressor stability, silence and latency.

use super::*;

fn chain(fw: FirmwareFx) -> LayerFxChain {
    let mut fx = LayerFxChain::new(48_000.0);
    fx.reverb_model = ReverbModel::Off;
    fx.firmware_fx = fw;
    fx
}

fn render(fx: &mut LayerFxChain, n: usize, f: impl Fn(usize) -> f32) -> Vec<[f32; 2]> {
    (0..n)
        .map(|i| {
            let x = f(i);
            fx.process_frame([x, -x])
        })
        .collect()
}

fn tone(i: usize) -> f32 {
    (i as f32 * 0.043).sin() * 0.5
}

#[test]
fn default_chain_is_bit_identical_after_the_switches_toggle_back_off() {
    let mut a = chain(FirmwareFx::OFF);
    a.delay_enabled = true;
    a.comp_enabled = true;
    a.comp_amount = 0.6;
    a.delay_time_ms = 20.0;
    let mut b = a.clone();
    b.firmware_fx = FirmwareFx::ALL;
    b.firmware_fx = FirmwareFx::OFF;
    assert_eq!(render(&mut a, 2000, tone), render(&mut b, 2000, tone));
    // switching the compressor off mid-stream returns to the legacy path (stage dropped)
    let mut c = chain(FirmwareFx::ALL);
    let _ = render(&mut c, 300, tone);
    c.firmware_fx = FirmwareFx::OFF;
    let _ = render(&mut c, 1, tone);
    assert!(c.fw_comp.is_none());
}

#[test]
fn delay_mix_law_fades_dry_out_and_the_send_in() {
    let run = |mix: f32| {
        let mut fx = chain(FirmwareFx {
            delay_laws: true,
            ..FirmwareFx::OFF
        });
        fx.delay_enabled = true;
        fx.delay_time_ms = 20.0;
        fx.delay_feedback = 0.0;
        fx.delay_mix = mix;
        let out = render(&mut fx, 4000, |i| if i == 0 { 1.0 } else { 0.0 });
        (out[0][0], out[960][0])
    };
    let (dry0, echo0) = run(0.0);
    assert!(
        (dry0 - 1.0).abs() < 1e-6 && echo0.abs() < 1e-6,
        "mix 0: dry only"
    );
    let (dry1, echo1) = run(1.0);
    assert!(dry1.abs() < 1e-3, "mix 127: dry gone, got {dry1}");
    assert!(
        (echo1 - 1.0).abs() < 1e-3,
        "mix 127: full send, got {echo1}"
    );
    let (dry_half, echo_half) = run(0.5);
    assert!(
        dry_half > 0.99 && echo_half > 0.9,
        "both audible at mid mix"
    );
}

#[test]
fn firmware_compressor_delays_four_frames_and_is_unity_when_disabled() {
    let mut fx = chain(FirmwareFx {
        compressor: true,
        ..FirmwareFx::OFF
    });
    let out = render(&mut fx, 16, |i| if i == 0 { 0.25 } else { 0.0 });
    assert!(
        out[..4].iter().all(|f| f[0] == 0.0),
        "first block is silent"
    );
    assert!(
        (out[4][0] - 0.25).abs() < 1e-5,
        "unity gain, got {}",
        out[4][0]
    );
    assert!((out[4][1] + 0.25).abs() < 1e-5);
}

#[test]
fn firmware_compressor_is_silent_for_silence_and_stable_for_loud_and_quiet_input() {
    for amount in [0.0, 0.3, 1.0] {
        let mut fx = chain(FirmwareFx::ALL);
        fx.comp_enabled = true;
        fx.comp_amount = amount;
        let quiet = render(&mut fx, 3000, |_| 0.0);
        assert!(quiet.iter().all(|f| f[0] == 0.0 && f[1] == 0.0));
        let out = render(&mut fx, 60_000, |i| {
            let env = if (i / 6000) % 2 == 0 { 1.5 } else { 0.001 };
            env * (i as f32 * 0.31).sin()
        });
        for (i, f) in out.iter().enumerate() {
            assert!(f[0].is_finite() && f[1].is_finite(), "frame {i}");
            assert!(f[0].abs() < 100.0, "gain bounded by the makeup, frame {i}");
            assert!(f[0] == 0.0 || f[0].abs() > 1e-30, "no denormal at {i}");
        }
    }
}

#[test]
fn firmware_compressor_amount_changes_the_level_of_a_loud_tone() {
    let rms = |amount: f32| {
        let mut fx = chain(FirmwareFx {
            compressor: true,
            ..FirmwareFx::OFF
        });
        fx.comp_enabled = true;
        fx.comp_amount = amount;
        let out = render(&mut fx, 48_000, |i| 0.5 * (i as f32 * 0.09).sin());
        let tail = &out[24_000..];
        (tail.iter().map(|f| f[0] * f[0]).sum::<f32>() / tail.len() as f32).sqrt()
    };
    let (off, mid, full) = (rms(0.0), rms(0.5), rms(1.0));
    assert!((off - 0.3535).abs() < 0.01, "amount 0 is unity: {off}");
    assert!(mid != off && full != mid, "amount acts: {off} {mid} {full}");
}
