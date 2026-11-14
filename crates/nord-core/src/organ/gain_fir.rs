//! Linear-phase FIR from a measured magnitude response (frequency sampling with a Hann window),
//! shared by the Farfisa key stage and the B3 Bass stage.
//!
//! The processors `0x122B85` and `0x122296` are linear and time invariant in the ring and the key
//! weights, with a gain that depends on the line frequency only (research s8, s11), so a measured
//! magnitude table realises them as a FIR (magnitude only, no phase: EMPIRICAL).

use std::f32::consts::{PI, TAU};

/// Fills `fir` (odd length, symmetric) so that its response follows `gain(hz)` at `sample_rate`.
pub(super) fn design(fir: &mut [f32], sample_rate: f32, gain: impl Fn(f32) -> f32) {
    let taps = fir.len();
    let c = (taps - 1) / 2;
    let amp: Vec<f32> = (0..=taps / 2)
        .map(|m| gain(m as f32 * sample_rate / taps as f32))
        .collect();
    for (n, tap) in fir.iter_mut().enumerate() {
        let t = n as f32 - c as f32;
        let mut acc = amp[0];
        for (m, a) in amp.iter().enumerate().skip(1).take(c) {
            acc += 2.0 * a * (TAU * m as f32 * t / taps as f32).cos();
        }
        let win = 0.5 + 0.5 * (PI * t / (c as f32 + 1.0)).cos();
        *tap = acc / taps as f32 * win;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_follows_the_requested_magnitude() {
        let fs = 48_000.0;
        let mut fir = vec![0.0f32; 511];
        design(&mut fir, fs, |hz| if hz < 2000.0 { 1.0 } else { 0.25 });
        let at = |hz: f32| {
            let (mut re, mut im) = (0.0f32, 0.0f32);
            for (n, h) in fir.iter().enumerate() {
                let a = TAU * hz / fs * n as f32;
                re += h * a.cos();
                im -= h * a.sin();
            }
            re.hypot(im)
        };
        assert!((at(500.0) - 1.0).abs() < 0.05);
        assert!((at(6000.0) - 0.25).abs() < 0.05);
    }

    #[test]
    fn taps_are_symmetric() {
        let mut fir = vec![0.0f32; 255];
        design(&mut fir, 48_000.0, |hz| 1.0 / (1.0 + hz / 1000.0));
        for i in 0..127 {
            assert!((fir[i] - fir[254 - i]).abs() < 1e-6);
        }
    }
}
