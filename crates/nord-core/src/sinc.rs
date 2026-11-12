//! Hardware sample-interpolation kernels: 12 taps x 256 phases, Kaiser-windowed sinc.
//!
//! The synth DSP image of OS v1.44 to v1.66 holds six interpolation kernels
//! (`docs/research/sy-dsp-map.md` section 2, `docs/research/dsp-image-hunt.md` 5.15,
//! `docs/specs/synth-engine.md` section 7):
//!
//! ```text
//! h[tap][phase] = sinc(fc d) * kaiser(d),   d = tap - 5 - phase / 256,   |d| <= 6
//! kaiser(d)     = I0(8.5 sqrt(1 - (d / 6)^2)) / I0(8.5)
//! ```
//!
//! The tap sum of every phase is `1 / fc` (the kernels are not normalised to unity gain).
//! [`SincKernel::interpolate`] multiplies by `fc` so the DC gain is 1.
//!
//! Not wired into the voices yet (TD-08); the `SAMPLES` oscillator still uses 4-point cubic
//! interpolation. The sampler and piano path of the hardware (SMP image) uses a different
//! 6-tap x 256-phase Q31 kernel (`docs/research/pno-dsp-map.md` section 7); that role is ASSUMED
//! and not implemented here, so this module is the synth oscillator path only.

use std::f64::consts::PI;

/// Number of taps per phase. VERIFIED: layout of the firmware table (`tools/ns4_synth_kernels.py`).
pub const TAPS: usize = 12;
/// Sub-sample phases per kernel. VERIFIED: layout of the firmware table.
pub const PHASES: usize = 256;
/// Tap whose distance to the output position is `0` at phase `0`. VERIFIED: the table peaks at tap 5.
pub const CENTER_TAP: usize = 5;
/// Window half-width in samples. VERIFIED: free fit gives 6.00001; the edge `d = +6` (tap 11,
/// phase 0) is non-zero, so the support is `|d| <= 6`.
const HALF_WIDTH: f64 = 6.0;
/// Kaiser shape parameter. VERIFIED: `SY1SY2` image offset `0x76A8`, free fit 8.50002, golden-section
/// fit 8.5000, model max error 5.9e-8 (one float32 ulp) over all 18,432 values, checked by
/// `tools/ns4_sinc_check.py` and `tools/tests/test_ns4_sinc_check.py`.
const KAISER_BETA: f64 = 8.5;

/// Cutoffs `fc` of the six kernels (argument scale of `sinc`, `1.0` = Nyquist).
///
/// VERIFIED: `SY1SY2` image offset `0x76A8` (DM `0xE3400`, blocks `0xC00` words apart), OS v1.44
/// to v1.66; free fit 1.1000000, 1.0500000, 1.0000000, 0.9600000, 0.9250000, 0.8950000
/// (`tools/ns4_sinc_check.py`), tap sum `1 / fc` = 0.9091, 0.9524, 1.0000, 1.0417, 1.0811, 1.1173.
/// How the hardware picks a kernel from the playback ratio is not in the DSP code (no DSP
/// instruction writes the voice's kernel pointer); see [`SincKernelSet::select`].
pub const CUTOFFS: [f32; 6] = [1.10, 1.05, 1.00, 0.96, 0.925, 0.895];

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

/// All six hardware kernels, built once at construction (allocates then, never afterwards).
#[derive(Debug, Clone, PartialEq)]
pub struct SincKernelSet {
    kernels: Vec<SincKernel>,
}

impl SincKernelSet {
    /// Builds the six kernels of [`CUTOFFS`]. Call at engine construction, not on the audio path.
    pub fn new() -> Self {
        Self {
            kernels: (0..CUTOFFS.len()).map(SincKernel::hardware).collect(),
        }
    }

    /// Kernel `index` (`0..6`, clamped).
    pub fn get(&self, index: usize) -> &SincKernel {
        &self.kernels[index.min(self.kernels.len() - 1)]
    }

    /// Index of the kernel used for a playback ratio (source samples per output sample).
    ///
    /// ASSUMED: the ARM picks the kernel (it writes the voice record's kernel pointer; the DSP
    /// never does) and the rule is not decoded. Chosen so that a higher ratio gets a lower `fc`
    /// (anti-alias): the kernel whose `fc` is nearest to `min(1, 1 / ratio)`, so ratios `<= 1` use
    /// `fc = 1.00` (index 2; the `fc = 1.10` and `1.05` kernels are never chosen). Confirmed if a
    /// run of the ARM sample-start path shows which `0xE3400 + 0xC00 k` it sends for a given pitch
    /// (`docs/research/sy-dsp-map.md` section 6, ARM literal `bank0_CODE@0xC0001BAC`).
    pub fn select(ratio: f32) -> usize {
        let target = if ratio > 1.0 { 1.0 / ratio } else { 1.0 };
        let mut best = 2;
        for (i, fc) in CUTOFFS.iter().enumerate().skip(2) {
            if (fc - target).abs() < (CUTOFFS[best] - target).abs() {
                best = i;
            }
        }
        best
    }

    /// Kernel for a playback ratio, see [`Self::select`]. Allocation-free.
    pub fn for_ratio(&self, ratio: f32) -> &SincKernel {
        self.get(Self::select(ratio))
    }
}

impl Default for SincKernelSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            let want = (sinc(6.0 * f64::from(fc)) / bessel_i0(8.5)) as f32;
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
    fn tap_sum_is_one_over_cutoff_for_every_phase() {
        // VERIFIED in firmware to 1e-4 (0.90899..0.90907 for kernel 0, 1/fc = 0.90909).
        for (i, fc) in CUTOFFS.iter().enumerate() {
            let kernel = SincKernel::hardware(i);
            for phase in 0..PHASES {
                let sum: f32 = kernel.weights(phase).iter().sum();
                assert!(
                    (sum * fc - 1.0).abs() < 2e-4,
                    "kernel {i} phase {phase}: sum*fc = {}",
                    sum * fc
                );
            }
        }
    }

    #[test]
    fn fc_one_is_an_exact_delta_at_phase_zero() {
        let kernel = SincKernel::new(1.0);
        for (tap, w) in kernel.weights(0).iter().enumerate() {
            let want = if tap == CENTER_TAP { 1.0 } else { 0.0 };
            assert!((w - want).abs() < 1e-6, "tap {tap}");
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
                (kernel.interpolate(&pcm, pos) - 1.0).abs() < 2e-4,
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

    #[test]
    fn kernel_set_holds_all_six_cutoffs_and_selects_by_ratio() {
        let set = SincKernelSet::default();
        for (i, fc) in CUTOFFS.iter().enumerate() {
            assert_eq!(set.get(i).cutoff(), *fc);
        }
        assert_eq!(set.get(99).cutoff(), CUTOFFS[5]);
        assert_eq!(SincKernelSet::select(0.5), 2);
        assert_eq!(SincKernelSet::select(1.0), 2);
        assert_eq!(SincKernelSet::select(1.04), 3);
        assert_eq!(SincKernelSet::select(1.08), 4);
        assert_eq!(SincKernelSet::select(1.12), 5);
        assert_eq!(SincKernelSet::select(8.0), 5);
        assert_eq!(set.for_ratio(1.04).cutoff(), 0.96);
    }
}
