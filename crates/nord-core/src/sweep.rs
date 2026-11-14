//! Output-health statistics for the factory program sweep (`crates/nord-tools/src/bin/program_sweep.rs`,
//! `tools/program_sweep.py`, `docs/research/program-coverage.md`).
//!
//! Pure functions over rendered stereo buffers; they allocate nothing on the audio path and are
//! only used offline. Thresholds are tool conventions, not hardware facts (ASSUMED).

use serde::Serialize;

/// Absolute sample level at or above which a sample counts as clipped (ASSUMED: digital full scale).
pub const CLIP_LEVEL: f32 = 1.0;

/// Peak below which a render counts as silent, -80 dBFS (ASSUMED: far below any audible note).
pub const SILENCE_PEAK: f32 = 1.0e-4;

/// Health statistics of one stereo render.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct RenderStats {
    /// Frames per channel.
    pub frames: usize,
    /// Samples that are NaN.
    pub nan: usize,
    /// Samples that are infinite.
    pub inf: usize,
    /// Peak of the finite samples.
    pub peak: f32,
    /// RMS over the finite samples of both channels.
    pub rms: f32,
    /// Finite samples with `|x| >= CLIP_LEVEL`.
    pub clipped: usize,
    /// Peak below [`SILENCE_PEAK`].
    pub silent: bool,
}

impl RenderStats {
    /// True when every sample is finite.
    pub fn finite(&self) -> bool {
        self.nan == 0 && self.inf == 0
    }
}

/// Measures a stereo render; non-finite samples are counted and excluded from peak and RMS.
pub fn analyze(left: &[f32], right: &[f32]) -> RenderStats {
    let (mut nan, mut inf, mut clipped, mut n) = (0usize, 0usize, 0usize, 0usize);
    let (mut peak, mut sum) = (0.0f32, 0.0f64);
    for &x in left.iter().chain(right.iter()) {
        if x.is_nan() {
            nan += 1;
        } else if x.is_infinite() {
            inf += 1;
        } else {
            let a = x.abs();
            peak = peak.max(a);
            clipped += usize::from(a >= CLIP_LEVEL);
            sum += f64::from(x) * f64::from(x);
            n += 1;
        }
    }
    let rms = if n == 0 {
        0.0
    } else {
        (sum / n as f64).sqrt() as f32
    };
    RenderStats {
        frames: left.len(),
        nan,
        inf,
        peak,
        rms,
        clipped,
        silent: peak < SILENCE_PEAK,
    }
}

/// Level difference `20 log10(b / a)` of two RMS values in dB; `None` when either is zero.
pub fn rms_delta_db(a: f32, b: f32) -> Option<f32> {
    (a > 0.0 && b > 0.0).then(|| 20.0 * (b / a).log10())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_nan_inf_and_clipping_and_excludes_them_from_levels() {
        let l = [0.5, f32::NAN, 1.0, -0.5];
        let r = [f32::INFINITY, 0.0, 1.5, 0.5];
        let s = analyze(&l, &r);
        assert_eq!((s.nan, s.inf, s.clipped, s.frames), (1, 1, 2, 4));
        assert!(!s.finite());
        assert_eq!(s.peak, 1.5);
        assert!(!s.silent);
    }

    #[test]
    fn silence_and_rms_of_a_constant() {
        let s = analyze(&[0.0; 8], &[0.0; 8]);
        assert!(s.silent && s.finite() && s.rms == 0.0);
        let s = analyze(&[0.5; 4], &[0.5; 4]);
        assert!((s.rms - 0.5).abs() < 1e-6 && !s.silent);
        let s = analyze(&[], &[]);
        assert!(s.silent && s.rms == 0.0);
    }

    #[test]
    fn rms_delta_is_in_decibels() {
        assert!((rms_delta_db(0.1, 0.2).unwrap() - 6.0206).abs() < 1e-3);
        assert_eq!(rms_delta_db(0.0, 0.2), None);
    }
}
