//! The DSP's 40-bit extended arithmetic as the Stage B resampler uses it, in portable Rust.
//!
//! Shared by `tests/oracle_stage_b.rs` and `examples/sinc_ref.rs` through `#[path]`. Rules, all
//! VERIFIED against the oracle (`docs/research/oracle-vs-engine.md`): products and sums are
//! rounded to 32 significant bits (1 + 31 fraction bits of the 40-bit register) with ties to even,
//! the sum starts from zero and adds tap 0 first, the gain multiplies the finished sum, and the
//! store to data memory keeps the upper 32 bits, which is a truncation to 24 significant bits.

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
