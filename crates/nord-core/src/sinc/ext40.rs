//! The DSP's 40-bit extended arithmetic as the Stage B resampler uses it, in portable Rust.
//!
//! Not the render path (the default [`SincKernel::interpolate`] is plain f32 and faster); this is
//! for tests and the `sinc_ref` comparison example. Rules, all VERIFIED against the oracle with
//! the oracle's own table (every output bit of three renders, `tests/oracle_stage_b.rs`,
//! `docs/research/oracle-vs-engine.md` section 4): products and sums are rounded to 32 significant
//! bits (1 + 31 fraction bits of the 40-bit register) with ties to even, the sum starts from zero
//! and adds tap 0 first, the gain multiplies the finished sum, and the store to data memory keeps
//! the upper 32 bits, which is a truncation to 24 significant bits. The oracle's rules are
//! VERIFIED on the ADI simulator (`docs/research/sharc-float.md`), not on hardware.

use super::SincKernel;

/// Rounds (or truncates) a finite f64 to `bits` significant bits.
pub fn round_sig(x: f64, bits: u32, trunc: bool) -> f64 {
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    let drop = 53 - bits;
    let (b, mask) = (x.to_bits(), (1u64 << drop) - 1);
    if trunc {
        return f64::from_bits(b & !mask);
    }
    let (half, lsb) = (1u64 << (drop - 1), (b >> drop) & 1);
    f64::from_bits((b + half - 1 + lsb) & !mask)
}

/// One output sample `gain * sum(w[t] * x(t))` in 40-bit extended arithmetic, stored as f32.
/// `trunc_ops` truncates instead of rounding to nearest in the multiplies and adds (the oracle
/// does not: reset MODE1), `trunc_store` selects truncation (the DM store) or rounding for the
/// final conversion.
pub fn mac_ext40(
    w: &[f32],
    x: impl Fn(usize) -> f32,
    gain: f32,
    trunc_ops: bool,
    trunc_store: bool,
) -> f32 {
    let r = |v: f64| round_sig(v, 32, trunc_ops);
    let mut acc = 0.0_f64;
    for (t, w) in w.iter().enumerate() {
        acc = r(acc + r(f64::from(*w) * f64::from(x(t))));
    }
    let g = r(acc * f64::from(gain));
    round_sig(g, 24, trunc_store) as f32
}

impl SincKernel {
    /// Like [`SincKernel::interpolate`] (same position convention, floored phase, edge repeat),
    /// but with the DSP's 40-bit accumulation and the hardware's separate `gain` instead of the
    /// built-in `fc` (the firmware leaves the sum at `1 / fc` and the voice gain follows; pass
    /// `gain = cutoff() as f32` for unity DC gain). Slow; for tests and comparisons only.
    pub fn interpolate_ext40(&self, pcm: &[f32], pos: f64, gain: f32) -> f32 {
        let Some(last) = pcm.len().checked_sub(1) else {
            return 0.0;
        };
        let (idx, phase) = Self::split_position(pos);
        mac_ext40(
            self.weights(phase),
            |t| Self::source(pcm, last, idx, t),
            gain,
            false,
            true,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinc::CENTER_TAP;

    #[test]
    fn rounding_helpers() {
        // 1 + 2^-32 is a tie at 32 significant bits and rounds to even (down); truncation also down
        let tie = 1.0 + 2f64.powi(-32);
        assert_eq!(round_sig(tie, 32, false), 1.0);
        // 1 + 1.5 * 2^-31 rounds up to 1 + 2^-30 (the measured simulator row)
        assert_eq!(
            round_sig(1.0 + 1.5 * 2f64.powi(-31), 32, false),
            1.0 + 2f64.powi(-30)
        );
        assert_eq!(
            round_sig(1.0 + 1.5 * 2f64.powi(-31), 32, true),
            1.0 + 2f64.powi(-31)
        );
        assert_eq!(round_sig(0.0, 32, false), 0.0);
        let one = mac_ext40(&[0.5, 0.25], |t| [2.0, 4.0][t], 2.0, false, true);
        assert_eq!(one, 4.0);
    }

    #[test]
    fn interpolate_ext40_agrees_with_the_f32_path_within_an_ulp() {
        let pcm: Vec<f32> = (0..128)
            .map(|n| ((n * 37 + 11) % 64) as f32 / 64.0 - 0.5)
            .collect();
        for k in 0..6 {
            let kern = SincKernel::hardware(k);
            let gain = kern.cutoff();
            for step in 0..60 {
                let pos = 20.0 + f64::from(step) * 0.731;
                let (a, b) = (
                    kern.interpolate(&pcm, pos),
                    kern.interpolate_ext40(&pcm, pos, gain),
                );
                assert!((a - b).abs() < 2e-7, "kernel {k} pos {pos}: {a} vs {b}");
            }
        }
    }

    #[test]
    fn interpolate_ext40_floors_the_phase_and_handles_edges() {
        let kern = SincKernel::new(1.0);
        let pcm: Vec<f32> = (0..64).map(|i| (i as f32 * 0.37).sin()).collect();
        let p76 = 20.0 + 76.0 / 256.0;
        assert_eq!(
            kern.interpolate_ext40(&pcm, 20.3, 1.0),
            kern.interpolate_ext40(&pcm, p76, 1.0)
        );
        assert_eq!(kern.interpolate_ext40(&[], 3.0, 1.0), 0.0);
        let ones = [1.0_f32; 8];
        assert!((kern.interpolate_ext40(&ones, -4.0, 1.0) - 1.0).abs() < 2e-4);
        assert_eq!(CENTER_TAP, 5);
    }
}
