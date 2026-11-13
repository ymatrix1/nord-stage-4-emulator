//! SMP 6-tap, 256-phase Q31 interpolation kernel (the hardware piano sampler path).
//!
//! Spec: `docs/specs/piano-engine.md` ("SMP sample interpolation"); fit and evidence:
//! `docs/research/smp-kernel-fit.md`, `tools/ns4_smp_kernel_fit.py`.
//!
//! The SMP DSP image holds this table at `DM(0xE3400)`: 6 rows (taps) of 256 phases, Q31 words
//! with the low byte zero. The voice loop (`0x1C07D8`) reads it with stride 256 after
//! `I9 = 0xE3400 + (frac >> 24)`, six `MRF += R0*R1 (SSF)` steps over the 6 newest history samples.
//!
//! The table is not stored here: it is rebuilt once from its closed form (a constant 6 KB).
//!
//! ```text
//! d        = (2 - tap) + phase / 256                    tap 0..5, phase 0..255
//! u(d)     = sinc(d) * (0.54 + 0.46 cos(pi d / 3))      Hamming window, half width 3
//! h[t][p]  = u + (1 - sum_t u) / 6                      additive DC correction per phase
//! word     = min(round(h * 2^23), 2^23 - 1) << 8
//! ```
//!
//! Tap `t` weights the sample `n - 2 + t` when the read position is `n + phase / 256`.

use std::f64::consts::PI;
use std::sync::OnceLock;

/// Decoded-integer units per float PCM unit (`y / 8192`, the CNSP decoders' scale).
pub const Y_PER_UNIT: f32 = 8192.0;
/// Number of kernel taps (history samples read per output sample).
pub const TAPS: usize = 6;
/// Number of fractional phases (`frac >> 24` of the 32-bit position fraction).
pub const PHASES: usize = 256;
/// Significant bits of a kernel word (`word = q23 << 8`).
pub const Q_BITS: u32 = 23;

/// VERIFIED: Hamming `a` = 0.54 (0.46 for the cosine), half width 3, cutoff 1, all 1,536 words of
/// the v1.66 SMP table, checked by `python tools/ns4_smp_kernel_fit.py smp.bin` (0 mismatches)
/// and by `test_closed_form_golden_words` (words and checksums), max err 0 LSB.
const HAMMING_A: f64 = 0.54;
/// VERIFIED: see [`HAMMING_A`].
const HALF_WIDTH: f64 = 3.0;

/// Sample interpolation used by the piano sampler voices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SampleInterp {
    /// Catmull-Rom cubic over 4 points (the pre-SMP-research default, kept for comparison).
    Cubic,
    /// Hardware parity: the 6-tap, 256-phase SMP kernel with the phase truncated to 1/256.
    #[default]
    SmpQ31,
}

/// The SMP kernel in integer (hardware) and `f32` (render) form.
#[derive(Debug, Clone)]
pub struct SmpKernel {
    /// `[tap][phase]` Q31 words exactly as stored in the SMP image.
    words: [[i32; PHASES]; TAPS],
    /// `[phase][tap]` weights as `f32` (`word / 2^31`; exact, 23 significant bits).
    weights: [[f32; TAPS]; PHASES],
}

fn sinc(d: f64) -> f64 {
    if d == 0.0 {
        1.0
    } else {
        (PI * d).sin() / (PI * d)
    }
}

impl SmpKernel {
    /// Builds the table from its closed form (allocation-free; call once at construction).
    pub fn build() -> Self {
        let mut words = [[0i32; PHASES]; TAPS];
        let mut weights = [[0f32; TAPS]; PHASES];
        for p in 0..PHASES {
            let mut u = [0f64; TAPS];
            for (t, v) in u.iter_mut().enumerate() {
                let d = (2.0 - t as f64) + p as f64 / PHASES as f64;
                *v = sinc(d) * (HAMMING_A + (1.0 - HAMMING_A) * (PI * d / HALF_WIDTH).cos());
            }
            let fix = (1.0 - u.iter().sum::<f64>()) / TAPS as f64;
            for (t, &ut) in u.iter().enumerate() {
                let q23 = (((ut + fix) * f64::from(1u32 << Q_BITS)).round() as i64)
                    .min((1i64 << Q_BITS) - 1);
                words[t][p] = (q23 << 8) as i32;
                weights[p][t] = (q23 as f32) / (1u32 << Q_BITS) as f32;
            }
        }
        Self { words, weights }
    }

    /// Process-wide shared table, built on first use (never inside the audio callback).
    pub fn shared() -> &'static SmpKernel {
        static KERNEL: OnceLock<SmpKernel> = OnceLock::new();
        KERNEL.get_or_init(SmpKernel::build)
    }

    /// Q31 word of the SMP image at `[tap][phase]` (`DM(0xE3400 + 256 tap + phase)`).
    pub fn word(&self, tap: usize, phase: usize) -> i32 {
        self.words[tap][phase]
    }

    /// The 6 `f32` weights of one phase, oldest sample first.
    #[inline]
    pub fn weights(&self, phase: usize) -> &[f32; TAPS] {
        &self.weights[phase]
    }

    /// Phase index the hardware uses for a fractional position: `floor(frac * 256)` (truncation,
    /// `frac >> 24` of the fixed-point fraction). ASSUMED: the fraction register is unsigned Q32.
    #[inline]
    pub fn phase_of(frac: f64) -> usize {
        ((frac * PHASES as f64) as usize).min(PHASES - 1)
    }

    /// `sum_t taps[t] * weights[phase][t]` in `f32`.
    #[inline]
    pub fn mac_f32(&self, taps: &[f32; TAPS], phase: usize) -> f32 {
        taps.iter()
            .zip(&self.weights[phase])
            .fold(0.0f32, |acc, (x, w)| acc + x * w)
    }

    /// Integer model of the six `MRF += R0*R1 (SSF)` steps and `R2 = MRF (SSFR)`: fractional
    /// multiply (`a * w * 2`), result is the upper 32 bits, rounded to nearest.
    /// ASSUMED: ties round up (the rounding mode of MODE1 is not read from the image).
    pub fn mac_q31(&self, taps: &[i32; TAPS], phase: usize) -> i32 {
        let acc: i128 = taps
            .iter()
            .enumerate()
            .map(|(t, &x)| i128::from(x) * i128::from(self.words[t][phase]))
            .sum();
        ((acc + (1 << 30)) >> 31) as i32
    }

    /// Interpolated value of `pcm` at fractional index `pos`, or `None` outside the stream
    /// (`pos < 0` or `pos >= len`). Samples beyond either end repeat the edge sample (ASSUMED:
    /// the hardware ring buffer is fed by the unpacker and the end handling is not in the loop;
    /// the oracle voice starts from a zero history, `docs/research/smp-voice.md` section 6).
    #[inline]
    pub fn interpolate(&self, pcm: &[f32], pos: f64) -> Option<f32> {
        let (n, phase) = Self::locate(pcm.len(), pos)?;
        let last = pcm.len() - 1;
        let mut taps = [0f32; TAPS];
        for (t, v) in taps.iter_mut().enumerate() {
            *v = pcm[(n + t).saturating_sub(2).min(last)];
        }
        Some(self.mac_f32(&taps, phase))
    }

    /// Like [`Self::interpolate`] but with the DSP's integer arithmetic for integer-valued PCM
    /// (`pcm = y / 8192`, as the CNSP decoders produce): the taps are `y`, the sum is
    /// [`Self::mac_q31`] (rounded to a whole `y`), so the result is a multiple of `1/8192`.
    /// VERIFIED: equals the oracle's `R2 = MRF (SSFR)` of the SMP voice loop in 1,200 of 1,200
    /// frames (`smp_voice_ref`, `docs/research/smp-voice.md`); the f32 path differs from it by up
    /// to half a `y` unit (0.5/8192, SNR about 66 dB on a piano stroke).
    #[inline]
    pub fn interpolate_hw(&self, pcm: &[f32], pos: f64) -> Option<f32> {
        let (n, phase) = Self::locate(pcm.len(), pos)?;
        let last = pcm.len() - 1;
        let mut taps = [0i32; TAPS];
        for (t, v) in taps.iter_mut().enumerate() {
            *v = (pcm[(n + t).saturating_sub(2).min(last)] * Y_PER_UNIT).round() as i32;
        }
        Some(self.mac_q31(&taps, phase) as f32 / Y_PER_UNIT)
    }

    /// Integer index and kernel phase of read position `pos`, `None` outside the stream.
    #[inline]
    fn locate(len: usize, pos: f64) -> Option<(usize, usize)> {
        if len == 0 || pos < 0.0 {
            return None;
        }
        let n = pos as usize;
        (n < len).then(|| (n, Self::phase_of(pos - n as f64)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kernel() -> &'static SmpKernel {
        SmpKernel::shared()
    }

    /// Golden Q31 words of the SMP image (VERIFIED, same values as `tools/tests`).
    const GOLDEN: [(usize, usize, i32); 10] = [
        (2, 0, 0x7FFF_FF00),
        (2, 128, 0x4C6E_1C00),
        (2, 64, 0x7162_2E00),
        (1, 128, -246_675_456),
        (0, 128, 0x0245_DC00),
        (0, 1, 0x0013_2300),
        (5, 255, 0x0013_2300),
        (3, 100, 0x3936_E200),
        (4, 200, -240_518_144),
        (1, 1, -6_457_088),
    ];

    #[test]
    fn test_closed_form_golden_words() {
        let k = kernel();
        for (t, p, w) in GOLDEN {
            assert_eq!(k.word(t, p), w, "tap {t} phase {p}");
        }
        // Checksums of the whole table (computed from the image with the Python tool).
        let flat: Vec<i64> = (0..TAPS)
            .flat_map(|t| (0..PHASES).map(move |p| (t, p)))
            .map(|(t, p)| i64::from(k.word(t, p)))
            .collect();
        assert_eq!(flat.iter().sum::<i64>(), 549_755_807_488);
        let weighted: u64 = flat.iter().enumerate().fold(0u64, |a, (i, &w)| {
            a.wrapping_add((w as u64).wrapping_mul(i as u64 + 1))
        });
        assert_eq!(weighted, 422_212_460_209_920);
        assert!(flat.iter().all(|w| w & 0xFF == 0));
    }

    #[test]
    fn test_mirror_symmetry_is_exact() {
        let k = kernel();
        for t in 0..TAPS {
            for p in 1..PHASES {
                assert_eq!(k.word(t, p), k.word(TAPS - 1 - t, PHASES - p), "{t} {p}");
            }
        }
        // Phase 0: tap 5 is the d = -3 zero crossing.
        assert_eq!(k.word(5, 0), 0);
    }

    #[test]
    fn test_partition_of_unity() {
        let k = kernel();
        for p in 0..PHASES {
            let q: i64 = (0..TAPS).map(|t| i64::from(k.word(t, p)) >> 8).sum();
            // Rounding of 6 taps; phase 0 loses one LSB to the saturated peak.
            assert!((q - (1 << Q_BITS)).abs() <= 3, "phase {p}: {q}");
            let s: f32 = k.weights(p).iter().sum();
            assert!((s - 1.0).abs() < 4e-7, "phase {p}: {s}");
        }
    }

    #[test]
    fn test_integer_positions_are_pass_through() {
        let k = kernel();
        let pcm: Vec<f32> = (0..16).map(|i| (i as f32 * 0.37).sin()).collect();
        for n in 2..13 {
            let v = k.interpolate(&pcm, n as f64).unwrap();
            assert!((v - pcm[n]).abs() < 2e-7);
        }
    }

    #[test]
    fn test_mac_q31_matches_float_and_constant_signal() {
        let k = kernel();
        let taps = [1000, -2000, 3000, 4000, -500, 250];
        for p in [0usize, 1, 77, 128, 255] {
            let f = k.mac_f32(&taps.map(|v| v as f32), p);
            let q = k.mac_q31(&taps, p);
            assert!((f - q as f32).abs() < 0.51, "phase {p}: {f} vs {q}");
        }
        let dc = [8192; TAPS];
        for p in 0..PHASES {
            assert!((k.mac_q31(&dc, p) - 8192).abs() <= 1, "phase {p}");
        }
    }

    #[test]
    fn test_phase_truncation_and_bounds() {
        assert_eq!(SmpKernel::phase_of(0.0), 0);
        assert_eq!(SmpKernel::phase_of(0.5), 128);
        assert_eq!(SmpKernel::phase_of(0.999_999), 255);
        let k = kernel();
        assert!(k.interpolate(&[], 0.0).is_none());
        assert!(k.interpolate(&[1.0, 2.0], -0.1).is_none());
        assert!(k.interpolate(&[1.0, 2.0], 2.0).is_none());
        // Edge samples repeat: a 2-sample ramp stays finite and between the samples at the end.
        let v = k.interpolate(&[1.0, 2.0], 1.5).unwrap();
        assert!(v.is_finite());
    }

    /// Worst error of interpolating `sin(2 pi f n)` at fractional positions against the ideal.
    fn sine_error(f: f64, interp: impl Fn(&[f32], f64) -> Option<f32>) -> f64 {
        let pcm: Vec<f32> = (0..256)
            .map(|n| (2.0 * PI * f * n as f64).sin() as f32)
            .collect();
        let mut worst = 0.0f64;
        for i in 0..4000 {
            let pos = 20.0 + i as f64 * 0.0537;
            let got = f64::from(interp(&pcm, pos).unwrap());
            worst = worst.max((got - (2.0 * PI * f * pos).sin()).abs());
        }
        worst
    }

    fn cubic(pcm: &[f32], pos: f64) -> Option<f32> {
        super::super::PianoLayer::sample_pcm_at(pcm, pos)
    }

    #[test]
    fn test_sine_tone_error_against_ideal_and_cubic() {
        let k = kernel();
        let smp = |p: &[f32], x: f64| k.interpolate(p, x);
        // Phase truncation to 1/256 limits the error to about 2 pi f / 256.
        for f in [0.01, 0.05, 0.1] {
            let bound = 2.0 * PI * f / 256.0 + 6e-3 * f.max(0.05) * 4.0;
            let e = sine_error(f, smp);
            assert!(e < bound, "f {f}: {e} vs {bound}");
        }
        // Both stay close at low frequency; at 0.2 cycles/sample the kernel's flat passband
        // (response 1.0 up to 0.2) beats the Catmull-Rom droop.
        let (e_smp, e_cub) = (sine_error(0.2, smp), sine_error(0.2, cubic));
        assert!(
            e_smp < e_cub,
            "kernel {e_smp} should beat cubic {e_cub} at 0.2"
        );
        assert!(e_smp < 0.02, "{e_smp}");
    }

    #[test]
    fn test_interpolate_hw_is_the_integer_mac_and_within_half_a_unit_of_f32() {
        let k = kernel();
        let y: Vec<i32> = (0..64).map(|i| ((i * 7919) % 4001) - 2000).collect();
        let pcm: Vec<f32> = y.iter().map(|&v| v as f32 / Y_PER_UNIT).collect();
        for pos in [3.0, 3.5, 10.123, 40.99, 62.25] {
            let hw = k.interpolate_hw(&pcm, pos).unwrap();
            let n = pos as usize;
            let taps: [i32; 6] = std::array::from_fn(|t| y[(n + t).saturating_sub(2).min(63)]);
            let want = k.mac_q31(&taps, SmpKernel::phase_of(pos - n as f64));
            assert_eq!(hw, want as f32 / Y_PER_UNIT, "pos {pos}");
            let f = k.interpolate(&pcm, pos).unwrap();
            assert!((hw - f).abs() <= 0.5001 / Y_PER_UNIT, "pos {pos}: {hw} {f}");
        }
        assert!(k.interpolate_hw(&pcm, -1.0).is_none());
        assert!(k.interpolate_hw(&pcm, 64.0).is_none());
        assert!(k.interpolate_hw(&[], 0.0).is_none());
    }
}
