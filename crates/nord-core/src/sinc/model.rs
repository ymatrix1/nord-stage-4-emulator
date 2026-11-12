//! Closed-form model of the firmware kernel table: `h = sinc(fc d) * kaiser(d)`.
//!
//! See the parent module for the formula, the cutoffs and their provenance.

use super::{CENTER_TAP, PHASES};
use std::f64::consts::PI;

/// Window half-width in samples. VERIFIED: free fit gives 6.00001; the edge `d = +6` (tap 11,
/// phase 0) is non-zero, so the support is `|d| <= 6`.
const HALF_WIDTH: f64 = 6.0;
/// Kaiser shape parameter. VERIFIED: `SY1SY2` image offset `0x76A8`, free fit 8.50002, golden-section
/// fit 8.5000, model max error 5.9e-8 (one float32 ulp) over all 18,432 values, checked by
/// `tools/ns4_sinc_check.py` and `tools/tests/test_ns4_sinc_check.py`.
const KAISER_BETA: f64 = 8.5;

/// Modified Bessel function of the first kind, order 0 (power series, converged to f64 precision).
fn bessel_i0(x: f64) -> f64 {
    let q = x * x / 4.0;
    let (mut term, mut total, mut k) = (1.0_f64, 1.0_f64, 1.0_f64);
    while term > total * 1e-17 {
        term *= q / (k * k);
        total += term;
        k += 1.0;
    }
    total
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        1.0
    } else {
        (PI * x).sin() / (PI * x)
    }
}

/// Kaiser window, `1` at `d = 0` and `1 / I0(beta)` at `|d| = 6`; zero beyond.
fn window(d: f64) -> f64 {
    if d.abs() > HALF_WIDTH {
        return 0.0;
    }
    let r = d / HALF_WIDTH;
    bessel_i0(KAISER_BETA * (1.0 - r * r).max(0.0).sqrt()) / bessel_i0(KAISER_BETA)
}

/// One kernel value for cutoff `fc`, tap `0..TAPS` and phase `0..PHASES`.
///
/// `fc` is `f64` so the decimal cutoffs of [`super::CUTOFFS`] are evaluated exactly as the firmware table.
pub fn kernel_value(fc: f64, tap: usize, phase: usize) -> f32 {
    let d = tap as f64 - CENTER_TAP as f64 - phase as f64 / PHASES as f64;
    (sinc(fc * d) * window(d)) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinc::{CUTOFFS, TAPS};

    /// Values read from the firmware table (v1.66 `SY1SY2` image): (kernel, tap, phase, value),
    /// six decimals as printed by `tools/ns4_synth_kernels.py`.
    const FIRMWARE_GOLDEN: [(usize, usize, usize, f32); 9] = [
        (0, 5, 64, 0.874082),
        (0, 5, 128, 0.555968),
        (0, 6, 64, 0.189363),
        (0, 3, 200, -0.007800),
        (2, 5, 64, 0.894096),
        (2, 7, 100, -0.139042),
        (5, 5, 128, 0.682441),
        (5, 4, 30, 0.000102),
        (5, 9, 250, 0.031804),
    ];

    #[test]
    fn kernel_peaks_at_tap_five_and_is_zero_at_integer_offsets() {
        assert!((kernel_value(1.0, 5, 0) - 1.0).abs() < 1e-6);
        for tap in (0..TAPS).filter(|t| *t != 5) {
            assert!(kernel_value(1.0, tap, 0).abs() < 1e-6, "tap {tap}");
        }
    }

    #[test]
    fn model_matches_values_read_from_the_firmware_table() {
        // 6-decimal printout: tolerance 2e-6 (the 3-term cosine fit needed 2e-4).
        for (k, tap, phase, expected) in FIRMWARE_GOLDEN {
            let got = kernel_value(CUTOFFS[k], tap, phase);
            assert!(
                (got - expected).abs() < 2e-6,
                "kernel {k} tap {tap} phase {phase}: got {got}, firmware {expected}"
            );
        }
    }

    #[test]
    fn bessel_i0_matches_known_values() {
        assert!((bessel_i0(0.0) - 1.0).abs() < 1e-15);
        // Abramowitz and Stegun table 9.8: I0(1) = 1.2660658777520082, I0(5) = 27.239871823604442.
        assert!((bessel_i0(1.0) - 1.266_065_877_752_008_2).abs() < 1e-14);
        assert!((bessel_i0(5.0) / 27.239_871_823_604_442 - 1.0).abs() < 1e-14);
    }

    #[test]
    fn window_is_one_at_center_and_keeps_the_nonzero_edge() {
        assert!((window(0.0) - 1.0).abs() < 1e-15);
        let edge = 1.0 / bessel_i0(KAISER_BETA);
        assert!((window(6.0) - edge).abs() < 1e-15 && edge > 1e-4);
        assert_eq!(window(6.0001), 0.0);
        assert_eq!(window(-6.5), 0.0);
        assert!((window(2.5) - window(-2.5)).abs() < 1e-15);
    }

    #[test]
    fn edge_value_tap_11_phase_0_is_sinc_over_i0() {
        // VERIFIED element [11][0]: sinc(6 fc) / I0(8.5), 6.7e-5 for kernel 0 (sy-dsp-map.md 2).
        for fc in CUTOFFS {
            let want = (sinc(6.0 * fc) / bessel_i0(8.5)) as f32;
            assert!((kernel_value(fc, 11, 0) - want).abs() < 1e-9);
        }
        assert!((kernel_value(CUTOFFS[0], 11, 0) - 6.7e-5).abs() < 1e-6);
    }

    #[test]
    fn kernels_are_mirror_symmetric() {
        // VERIFIED in firmware: h[t][p] = h[11 - t][256 - p], bit exact; the model agrees to 1 ulp.
        for fc in CUTOFFS {
            for tap in 0..TAPS {
                for phase in 1..PHASES {
                    let a = kernel_value(fc, tap, phase);
                    let b = kernel_value(fc, TAPS - 1 - tap, PHASES - phase);
                    assert!((a - b).abs() < 1e-7, "fc {fc} tap {tap} phase {phase}");
                }
            }
        }
    }

    #[test]
    fn decimal_cutoffs_are_kept_in_f64() {
        // 1.1f32 widened is 1.10000002384; the table matches the decimal 1.1
        assert_ne!(f64::from(1.1_f32), CUTOFFS[0]);
        assert_eq!(CUTOFFS[0], 1.1_f64);
        let differing = (0..PHASES)
            .flat_map(|p| (0..TAPS).map(move |t| (p, t)))
            .filter(|&(p, t)| {
                kernel_value(f64::from(1.1_f32), t, p) != kernel_value(CUTOFFS[0], t, p)
            })
            .count();
        assert!(differing > 100, "{differing}");
    }
}
