//! Hardware sample-interpolation kernels: 12 taps x 256 phases, windowed sinc.
//!
//! The synth DSP image of OS v1.44 to v1.66 holds six interpolation kernels
//! (`docs/research/dsp-image-hunt.md` 5.15, `docs/specs/synth-engine.md` section 7):
//!
//! ```text
//! value[tap][phase] = sinc(fc * d) * window(d),   d = tap - 5 - phase / 256,   |d| < 6
//! window(d)         = 0.4231 + 0.4991 cos(pi d / 6) + 0.0778 cos(2 pi d / 6)
//! ```
//!
//! The tap sum of every phase is `1 / fc` (the kernels are not normalised to unity gain).
//! [`SincKernel::interpolate`] multiplies by `fc` so the DC gain is 1.
//!
//! Not wired into the voices yet; the `SAMPLES` oscillator still uses 4-point cubic interpolation.

use std::f64::consts::PI;

/// Number of taps per phase. VERIFIED: layout of the firmware table (`tools/ns4_synth_kernels.py`).
pub const TAPS: usize = 12;
/// Sub-sample phases per kernel. VERIFIED: layout of the firmware table.
pub const PHASES: usize = 256;
/// Tap whose distance to the output position is `0` at phase `0`. VERIFIED: the table peaks at tap 5.
pub const CENTER_TAP: usize = 5;
/// Window half-width in samples. VERIFIED: the table is zero-padded outside `|d| < 6`.
const HALF_WIDTH: f64 = 6.0;

/// Cutoffs `fc` of the six kernels (argument scale of `sinc`, `1.0` = Nyquist).
///
/// VERIFIED: `SY1SY2` image offset `0x76A8`, OS v1.44 to v1.66, recovered from the per-phase tap
/// sum (`1 / fc` = 0.9091, 0.9524, 1.0000, 1.0417, 1.0811, 1.1173) by `tools/ns4_synth_kernels.py`.
/// How the hardware picks a kernel from the playback ratio is not decoded.
pub const CUTOFFS: [f32; 6] = [1.10, 1.05, 1.00, 0.96, 0.925, 0.895];

/// Cosine-sum window coefficients, sum `1.0` so the window is `1` at `d = 0`.
///
/// EMPIRICAL: least-squares fit to all six firmware kernels, rms 3.6e-5, max 8.8e-5; close to the
/// textbook Blackman window (0.42, 0.5, 0.08), which leaves rms 3e-4.
const WINDOW: [f64; 3] = [0.4231, 0.4991, 0.0778];

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        1.0
    } else {
        (PI * x).sin() / (PI * x)
    }
}

fn window(d: f64) -> f64 {
    if d.abs() >= HALF_WIDTH {
        return 0.0;
    }
    let a = PI * d / HALF_WIDTH;
    WINDOW[0] + WINDOW[1] * a.cos() + WINDOW[2] * (2.0 * a).cos()
}

/// One kernel value for cutoff `fc`, tap `0..TAPS` and phase `0..PHASES`.
pub fn kernel_value(fc: f32, tap: usize, phase: usize) -> f32 {
    let d = tap as f64 - CENTER_TAP as f64 - phase as f64 / PHASES as f64;
    (sinc(f64::from(fc) * d) * window(d)) as f32
}

/// A precomputed polyphase kernel for one cutoff, stored phase-major (`[phase][tap]`).
#[derive(Debug, Clone, PartialEq)]
pub struct SincKernel {
    fc: f32,
    table: Box<[f32]>,
}

impl SincKernel {
    /// Builds the kernel for cutoff `fc` (allocates once; do not call on the audio thread).
    pub fn new(fc: f32) -> Self {
        let mut table = Vec::with_capacity(TAPS * PHASES);
        for phase in 0..PHASES {
            for tap in 0..TAPS {
                table.push(kernel_value(fc, tap, phase));
            }
        }
        Self {
            fc,
            table: table.into_boxed_slice(),
        }
    }

    /// Kernel `index` (`0..6`) of the hardware set, see [`CUTOFFS`].
    pub fn hardware(index: usize) -> Self {
        Self::new(CUTOFFS[index.min(CUTOFFS.len() - 1)])
    }

    /// Cutoff of this kernel.
    pub fn cutoff(&self) -> f32 {
        self.fc
    }

    /// Weights of one phase (`TAPS` values, tap 0 first).
    pub fn weights(&self, phase: usize) -> &[f32] {
        let p = phase.min(PHASES - 1);
        &self.table[p * TAPS..(p + 1) * TAPS]
    }

    /// Interpolates `pcm` at the fractional index `pos` with unity DC gain.
    ///
    /// ASSUMED: the fractional position is rounded to the nearest of the 256 phases (the hardware
    /// rule is not known). Indices outside `pcm` repeat the edge sample. Allocation-free.
    pub fn interpolate(&self, pcm: &[f32], pos: f64) -> f32 {
        let Some(last) = pcm.len().checked_sub(1) else {
            return 0.0;
        };
        let pos = pos.max(0.0);
        let base = pos.floor();
        let mut idx = base as usize;
        let mut phase = ((pos - base) * PHASES as f64).round() as usize;
        if phase >= PHASES {
            phase = 0;
            idx += 1;
        }
        let mut acc = 0.0_f32;
        for (tap, w) in self.weights(phase).iter().enumerate() {
            let k = (idx + tap).saturating_sub(CENTER_TAP).min(last);
            acc += w * pcm[k];
        }
        acc * self.fc
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Values read from the firmware table (v1.66 `SY1SY2` image): (kernel, tap, phase, value).
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
        for (k, tap, phase, expected) in FIRMWARE_GOLDEN {
            let got = kernel_value(CUTOFFS[k], tap, phase);
            assert!(
                (got - expected).abs() < 2e-4,
                "kernel {k} tap {tap} phase {phase}: got {got}, firmware {expected}"
            );
        }
    }

    #[test]
    fn tap_sum_is_one_over_cutoff_for_every_phase() {
        for (i, fc) in CUTOFFS.iter().enumerate() {
            let kernel = SincKernel::hardware(i);
            for phase in 0..PHASES {
                let sum: f32 = kernel.weights(phase).iter().sum();
                assert!(
                    (sum * fc - 1.0).abs() < 2e-3,
                    "kernel {i} phase {phase}: sum*fc = {}",
                    sum * fc
                );
            }
        }
    }

    #[test]
    fn integer_positions_return_the_sample() {
        let pcm: Vec<f32> = (0..64).map(|i| (i as f32 * 0.37).sin()).collect();
        let kernel = SincKernel::new(1.0);
        for i in 10..50 {
            assert!((kernel.interpolate(&pcm, i as f64) - pcm[i]).abs() < 1e-5);
        }
    }

    #[test]
    fn interpolates_a_sine_between_samples() {
        let f = 0.05_f64;
        let pcm: Vec<f32> = (0..200)
            .map(|n| (2.0 * PI * f * n as f64).sin() as f32)
            .collect();
        let kernel = SincKernel::new(1.0);
        for step in 0..200 {
            let pos = 30.0 + step as f64 * 0.431;
            let want = (2.0 * PI * f * pos).sin() as f32;
            let got = kernel.interpolate(&pcm, pos);
            assert!((got - want).abs() < 1e-3, "pos {pos}: {got} vs {want}");
        }
    }

    /// 4-point Catmull-Rom, the interpolation the `SAMPLES` oscillator uses today.
    fn catmull_rom(pcm: &[f32], pos: f64) -> f32 {
        let i = pos.floor() as usize;
        let t = (pos - i as f64) as f32;
        let (p0, p1, p2, p3) = (pcm[i - 1], pcm[i], pcm[i + 1], pcm[i + 2]);
        0.5 * ((2.0 * p1)
            + (-p0 + p2) * t
            + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t
            + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t * t * t)
    }

    #[test]
    fn sinc_kernel_is_more_accurate_than_cubic_near_high_frequencies() {
        let f = 0.3_f64;
        let pcm: Vec<f32> = (0..200)
            .map(|n| (2.0 * PI * f * n as f64).sin() as f32)
            .collect();
        let kernel = SincKernel::new(1.0);
        let (mut e_sinc, mut e_cubic) = (0.0_f32, 0.0_f32);
        for step in 0..100 {
            let pos = 30.0 + step as f64 * 0.37;
            let want = (2.0 * PI * f * pos).sin() as f32;
            e_sinc = e_sinc.max((kernel.interpolate(&pcm, pos) - want).abs());
            e_cubic = e_cubic.max((catmull_rom(&pcm, pos) - want).abs());
        }
        assert!(e_sinc < 0.01, "sinc error {e_sinc}");
        assert!(e_sinc * 5.0 < e_cubic, "sinc {e_sinc} vs cubic {e_cubic}");
    }

    #[test]
    fn edges_empty_input_and_phase_wrap_do_not_panic() {
        let kernel = SincKernel::hardware(2);
        assert_eq!(kernel.interpolate(&[], 3.0), 0.0);
        let pcm = [1.0_f32; 16];
        for pos in [-5.0, 0.0, 0.4999, 14.999, 15.0, 40.0] {
            assert!(
                (kernel.interpolate(&pcm, pos) - 1.0).abs() < 2e-3,
                "pos {pos}"
            );
        }
        // a fraction that rounds up to phase 256 moves to the next integer index at phase 0
        let ramp: Vec<f32> = (0..32).map(|i| i as f32).collect();
        let wrapped = kernel.interpolate(&ramp, 10.0 + 0.9999);
        assert!((wrapped - 11.0).abs() < 1e-2, "{wrapped}");
    }

    #[test]
    fn hardware_index_is_clamped_and_cutoffs_are_exposed() {
        assert_eq!(SincKernel::hardware(99).cutoff(), CUTOFFS[5]);
        assert_eq!(SincKernel::hardware(0).cutoff(), 1.10);
        assert_eq!(SincKernel::new(1.0).weights(PHASES + 7).len(), TAPS);
    }
}
