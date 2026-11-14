//! Tests of the Vox note low-pass and the harmonic content of one note (`vox_bank.rs`).

use super::lane_mix::FIRMWARE_FRAME_RATE_HZ;
use super::vox_bank::NOTES;
use super::vox_bank::{cycles_per_frame, VoxBank};
use std::f64::consts::PI;

fn frame_rate_bank() -> VoxBank {
    VoxBank::new(FIRMWARE_FRAME_RATE_HZ as f32)
}

#[test]
fn low_pass_coefficients_are_bilinear_with_unit_dc_gain() {
    let bank = frame_rate_bank();
    for lp in &bank.low_pass {
        assert!((2.0 * lp.c + lp.keep - 1.0).abs() < 1e-6);
        assert!(lp.c > 0.004 && lp.c < 0.31, "{}", lp.c);
    }
    // image extremes: 0.00450535 (k = 0) and 0.30378 (k = 83), within the 4-digit ratio
    assert!((bank.low_pass[0].c / 0.004_505_352 - 1.0).abs() < 1e-4);
    assert!((bank.low_pass[83].c / 0.303_781_36 - 1.0).abs() < 1e-4);
    // notes of one group share a coefficient
    assert_eq!(bank.low_pass[4].c, bank.low_pass[7].c);
}

#[test]
fn low_pass_response_matches_the_bilinear_formula() {
    // |H(f)| of c (1 + z^-1) / (1 - (1 - 2c) z^-1) against the analytic bilinear response
    // 1 / sqrt(1 + (tan(pi f) / tan(pi fc))^2), at the firmware frame rate
    let bank = frame_rate_bank();
    let k = 36;
    let c = f64::from(bank.low_pass[k].c);
    let fc = (c / (1.0 - c)).atan() / PI;
    for f in [0.0005f64, 0.003, 0.0063, 0.02, 0.1] {
        let w = 2.0 * PI * f;
        let (re_n, im_n) = (c * (1.0 + w.cos()), -c * w.sin());
        let (re_d, im_d) = (1.0 - (1.0 - 2.0 * c) * w.cos(), (1.0 - 2.0 * c) * w.sin());
        let mag = (re_n.hypot(im_n)) / (re_d.hypot(im_d));
        let want = 1.0 / (1.0 + ((PI * f).tan() / (PI * fc).tan()).powi(2)).sqrt();
        assert!((mag - want).abs() < 2e-3, "f {f}: {mag} vs {want}");
    }
}

#[test]
fn engine_rate_keeps_the_cutoff_in_hz() {
    let at_fw = frame_rate_bank();
    let at48 = VoxBank::new(48_000.0);
    for k in [0usize, 36, 83] {
        let hz = |bank: &VoxBank, fs: f64| {
            let c = f64::from(bank.low_pass[k].c);
            (c / (1.0 - c)).atan() / PI * fs
        };
        let a = hz(&at_fw, FIRMWARE_FRAME_RATE_HZ);
        let b = hz(&at48, 48_000.0);
        assert!((a / b - 1.0).abs() < 1e-4, "{k}: {a} {b}");
    }
}

#[test]
fn square_like_wave_has_odd_harmonics_and_an_asymmetric_second() {
    // single note 36 (MIDI 60): harmonic 3 about -16 dB, 2 about -35 dB (post-stage table)
    let mut bank = frame_rate_bank();
    let mut out = [0.0f32; NOTES];
    let n = 1 << 15;
    let mut sig = Vec::with_capacity(n);
    for i in 0..(n + 4096) {
        bank.step(0.0, 1.0, &mut out);
        if i >= 4096 {
            sig.push(f64::from(out[36]));
        }
    }
    let f0 = cycles_per_frame(36);
    let level = |h: f64| {
        let (mut re, mut im) = (0.0, 0.0);
        for (i, v) in sig.iter().enumerate() {
            let w = 2.0 * PI * f0 * h * i as f64;
            // Hann window
            let win = 0.5 - 0.5 * (2.0 * PI * i as f64 / n as f64).cos();
            re += v * win * w.cos();
            im += v * win * w.sin();
        }
        re.hypot(im)
    };
    let fund = level(1.0);
    let db = |h| 20.0 * (level(h) / fund).log10();
    assert!((db(3.0) + 16.4).abs() < 2.5, "3rd {}", db(3.0));
    assert!((db(5.0) + 24.9).abs() < 3.0, "5th {}", db(5.0));
    assert!(db(2.0) < -28.0 && db(2.0) > -45.0, "2nd {}", db(2.0));
}
