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
//! Position and phase convention (VERIFIED against the Stage B routine run on the SHARC oracle,
//! `docs/research/oracle-vs-engine.md` sections 3 and 4): the hardware position word holds the
//! frame index in bits 31..25 and the phase index in bits 24..17; the 17 bits below are ignored
//! (floor, no interpolation between phases). [`SincKernel::interpolate`] takes the position of the
//! centre tap as `f64` (`index + CENTER_TAP + phase / 256`) and floors the fraction to one of the
//! 256 phases the same way. [`SincKernel::interpolate_ext40`] adds the DSP's 40-bit accumulation
//! (tests and the `sinc_ref` example only).
//!
//! The synth `SAMPLES` oscillator uses this module (TD-08, `synth/sample_osc.rs`). The sampler and
//! piano path of the hardware (SMP image) uses a different 6-tap x 256-phase Q31 kernel
//! (`docs/research/pno-dsp-map.md` section 7, `piano/q31_kernel.rs`), so this module is the synth
//! oscillator path only.

pub mod ext40;
mod model;

pub use model::kernel_value;

/// Number of taps per phase. VERIFIED: layout of the firmware table (`tools/ns4_synth_kernels.py`).
pub const TAPS: usize = 12;
/// Sub-sample phases per kernel. VERIFIED: layout of the firmware table.
pub const PHASES: usize = 256;
/// Tap whose distance to the output position is `0` at phase `0`. VERIFIED: the table peaks at tap 5.
pub const CENTER_TAP: usize = 5;
/// Cutoffs `fc` of the six kernels (argument scale of `sinc`, `1.0` = Nyquist).
///
/// VERIFIED: `SY1SY2` image offset `0x76A8` (DM `0xE3400`, blocks `0xC00` words apart), OS v1.44
/// to v1.66; free fit 1.1000000, 1.0500000, 1.0000000, 0.9600000, 0.9250000, 0.8950000
/// (`tools/ns4_sinc_check.py`), tap sum `1 / fc` = 0.9091, 0.9524, 1.0000, 1.0417, 1.0811, 1.1173.
/// How the hardware picks a kernel from the playback ratio is not in the DSP code (no DSP
/// instruction writes the voice's kernel pointer); see [`SincKernelSet::select`].
///
/// Stored as `f64` on purpose: the table matches the decimal cutoffs evaluated in f64 (max error
/// 5.95e-8 against all 18,432 table values), while widening an `f32` such as `1.1f32`
/// (= 1.10000002384) gives 1.19e-7 (VERIFIED, `docs/research/oracle-vs-engine.md` section 4,
/// `tests/oracle_stage_b.rs`).
pub const CUTOFFS: [f64; 6] = [1.10, 1.05, 1.00, 0.96, 0.925, 0.895];

/// A precomputed polyphase kernel for one cutoff, stored phase-major (`[phase][tap]`).
#[derive(Debug, Clone, PartialEq)]
pub struct SincKernel {
    fc: f64,
    table: Box<[f32]>,
}

impl SincKernel {
    /// Builds the kernel for cutoff `fc` (allocates once; do not call on the audio thread).
    pub fn new(fc: f64) -> Self {
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
        self.fc as f32
    }

    /// Cutoff of this kernel at full precision.
    pub fn cutoff_f64(&self) -> f64 {
        self.fc
    }

    /// Weights of one phase (`TAPS` values, tap 0 first).
    pub fn weights(&self, phase: usize) -> &[f32] {
        let p = phase.min(PHASES - 1);
        &self.table[p * TAPS..(p + 1) * TAPS]
    }

    /// Splits a centre-tap position into the index of tap 0 and the phase.
    ///
    /// VERIFIED: the hardware floors to the 8 phase bits (position bits 24..17) and ignores the
    /// bits below, `docs/research/oracle-vs-engine.md` section 3; rounding to nearest instead costs
    /// 3.4e-3 abs error (SNR 41 to 45 dB) on a 0.3-sample start. Negative positions clamp to 0.
    fn split_position(pos: f64) -> (usize, usize) {
        let pos = pos.max(0.0);
        let base = pos.floor();
        let phase = (((pos - base) * PHASES as f64).floor() as usize).min(PHASES - 1);
        (base as usize, phase)
    }

    /// Source sample for tap `tap` of the window at `idx`; indices outside `pcm` repeat the edge.
    fn source(pcm: &[f32], last: usize, idx: usize, tap: usize) -> f32 {
        pcm[(idx + tap).saturating_sub(CENTER_TAP).min(last)]
    }

    /// Interpolates `pcm` at the fractional index `pos` (position of the centre tap) with unity
    /// DC gain.
    ///
    /// VERIFIED: the fractional position is floored to one of the 256 phases like the hardware
    /// (see the module documentation). Indices outside `pcm` repeat the edge sample (ASSUMED; the
    /// hardware reads a circular ring). Plain sequential f32 accumulation, within 1 ulp of the
    /// hardware (`tests/oracle_stage_b.rs`). Allocation-free.
    pub fn interpolate(&self, pcm: &[f32], pos: f64) -> f32 {
        let Some(last) = pcm.len().checked_sub(1) else {
            return 0.0;
        };
        let (idx, phase) = Self::split_position(pos);
        let mut acc = 0.0_f32;
        for (tap, w) in self.weights(phase).iter().enumerate() {
            acc += w * Self::source(pcm, last, idx, tap);
        }
        acc * self.fc as f32
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
        let target = f64::from(if ratio > 1.0 { 1.0 / ratio } else { 1.0 });
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
    use std::f64::consts::PI;

    #[test]
    fn tap_sum_is_one_over_cutoff_for_every_phase() {
        // VERIFIED in firmware to 1e-4 (0.90899..0.90907 for kernel 0, 1/fc = 0.90909).
        for (i, fc) in CUTOFFS.iter().enumerate() {
            let kernel = SincKernel::hardware(i);
            for phase in 0..PHASES {
                let sum: f32 = kernel.weights(phase).iter().sum();
                assert!(
                    (f64::from(sum) * fc - 1.0).abs() < 2e-4,
                    "kernel {i} phase {phase}: sum*fc = {}",
                    f64::from(sum) * fc
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
    fn split_position_floors_the_phase() {
        // VERIFIED rule (oracle-vs-engine.md 3): phase = floor(fraction * 256), no rounding up.
        assert_eq!(SincKernel::split_position(10.0), (10, 0));
        assert_eq!(SincKernel::split_position(10.0 + 76.0 / 256.0), (10, 76));
        assert_eq!(SincKernel::split_position(10.3), (10, 76)); // 76.8 -> 76, was 77 when rounded
        assert_eq!(SincKernel::split_position(10.0 + 255.9 / 256.0), (10, 255));
        assert_eq!(SincKernel::split_position(10.999_999_999_9), (10, 255));
        assert_eq!(SincKernel::split_position(-3.0), (0, 0));
    }

    #[test]
    fn interpolate_uses_the_floored_phase_weights() {
        // a fraction of 76.8 phases must give exactly the phase 76 answer, not phase 77
        let kernel = SincKernel::new(1.0);
        let pcm: Vec<f32> = (0..64)
            .map(|i| ((i * 37 + 11) % 64) as f32 / 64.0 - 0.5)
            .collect();
        let at_phase = |p: usize| kernel.interpolate(&pcm, 20.0 + p as f64 / PHASES as f64);
        assert_eq!(kernel.interpolate(&pcm, 20.3), at_phase(76));
        assert_ne!(kernel.interpolate(&pcm, 20.3), at_phase(77));
        assert_eq!(kernel.interpolate(&pcm, 20.0 + 76.99 / 256.0), at_phase(76));
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
            // floored phase: up to one phase (1/256 sample) early, slope 2 pi f = 0.31 -> 1.2e-3
            assert!((got - want).abs() < 1.5e-3, "pos {pos}: {got} vs {want}");
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
        // a fraction just below 1 floors to phase 255 of the same index (never wraps to 256)
        let ramp: Vec<f32> = (0..32).map(|i| i as f32).collect();
        let near_one = kernel.interpolate(&ramp, 10.0 + 0.999_999_999_9);
        assert!((near_one - 11.0).abs() < 1e-2, "{near_one}");
    }

    #[test]
    fn hardware_index_is_clamped_and_cutoffs_are_exposed() {
        assert_eq!(SincKernel::hardware(99).cutoff_f64(), CUTOFFS[5]);
        assert_eq!(SincKernel::hardware(0).cutoff(), 1.10);
        assert_eq!(SincKernel::hardware(0).cutoff_f64(), 1.10_f64);
        assert_eq!(SincKernel::new(1.0).weights(PHASES + 7).len(), TAPS);
    }

    #[test]
    fn kernel_set_holds_all_six_cutoffs_and_selects_by_ratio() {
        let set = SincKernelSet::default();
        for (i, fc) in CUTOFFS.iter().enumerate() {
            assert_eq!(set.get(i).cutoff_f64(), *fc);
        }
        assert_eq!(set.get(99).cutoff_f64(), CUTOFFS[5]);
        assert_eq!(SincKernelSet::select(0.5), 2);
        assert_eq!(SincKernelSet::select(1.0), 2);
        assert_eq!(SincKernelSet::select(1.04), 3);
        assert_eq!(SincKernelSet::select(1.08), 4);
        assert_eq!(SincKernelSet::select(1.12), 5);
        assert_eq!(SincKernelSet::select(8.0), 5);
        assert_eq!(set.for_ratio(1.04).cutoff_f64(), 0.96);
    }
}
