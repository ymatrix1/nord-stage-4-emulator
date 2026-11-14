//! Short (16-bit) floats of the SHARC `FPACK` / `FUNPACK` shifter operations, as used by the packed
//! delay memory of the firmware delay routine (`fw_delay`): sign 15, exponent 14..11, fraction
//! 10..0, two lanes per 32-bit word.
//!
//! VERIFIED: ported from `crates/nord-sharc/src/core/shifter.rs` (census diff on 600+ shifter
//! cases of the ADI simulator) through `tools/fx_pe.py`, which the delay model uses against the
//! `sharc-fx` oracle.

/// `FPACK`: an IEEE single to the 16-bit short float (sign 15, exponent 14..11, fraction 10..0).
/// Source exponent above 135 gives the largest magnitude; 121 ..= 135 keeps the MSB and three LSBs
/// of the exponent and rounds the fraction; 110 ..= 120 is a denormal; below 110 zero. VERIFIED:
/// `crates/nord-sharc/src/core/shifter.rs::fpack` (census diff on 600+ shifter cases).
pub fn fpack(x: u32) -> u32 {
    let sign = (x >> 31) << 15;
    let (e, frac) = ((x >> 23) & 0xFF, x & 0x7F_FFFF);
    let round = |value: u32, drop: u32| {
        let (kept, rest, half) = (value >> drop, value & ((1 << drop) - 1), 1 << (drop - 1));
        kept + u32::from(rest > half || (rest == half && kept & 1 == 1))
    };
    let magnitude = match e {
        136.. => return sign | 0x7FFF,
        121.. => (((((e >> 7) & 1) << 3) | (e & 7)) << 11) + round(frac, 12),
        110.. => round(0x80_0000 | frac, 133 - e),
        _ => 0,
    };
    sign | magnitude.min(0x7FFF)
}

/// `FUNPACK`: the 16-bit short float in the low half of `x` to an IEEE single.
pub fn funpack(x: u32) -> u32 {
    let (sign, exp, frac) = (((x >> 15) & 1) << 31, (x >> 11) & 0xF, x & 0x7FF);
    if exp != 0 {
        let msb = exp >> 3;
        let e8 = (msb << 7) | (((1 - msb) * 0xF) << 3) | (exp & 7);
        return sign | (e8 << 23) | (frac << 12);
    }
    if frac == 0 {
        return sign;
    }
    let lead = frac.leading_zeros() - 21;
    sign | ((120 - lead) << 23) | (((frac << (lead + 1)) & 0x7FF) << 12)
}

/// The two lanes of a packed word as floats (low = left, high = right).
#[inline]
pub fn lanes(word: u32) -> [f32; 2] {
    [word & 0xFFFF, word >> 16].map(|h| f32::from_bits(funpack(h)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_floats_pack_and_unpack_like_the_dsp() {
        let one = 1.0f32.to_bits();
        assert_eq!(fpack(one), 0x3800, "programming reference example");
        assert_eq!(funpack(0x3800), one);
        assert_eq!(fpack(1.0e10f32.to_bits()), 0x7FFF);
        assert_eq!(fpack((-1.0e10f32).to_bits()), 0xFFFF);
        assert_eq!(fpack(1.0e-9f32.to_bits()), 0);
        assert_eq!(
            funpack(1),
            110 << 23,
            "the smallest short float is a denormal"
        );
        assert_eq!(fpack(110 << 23), 1);
        for v in [0.02f32, 0.5, 1.0, -1.7, 100.0, -0.0623] {
            let back = f32::from_bits(funpack(fpack(v.to_bits())));
            assert!((back - v).abs() <= v.abs() / 2048.0, "{v} -> {back}");
        }
    }

    #[test]
    fn a_packed_word_holds_the_left_lane_low_and_the_right_lane_high() {
        let word = fpack(0.5f32.to_bits()) | (fpack((-2.0f32).to_bits()) << 16);
        assert_eq!(lanes(word), [0.5, -2.0]);
    }
}
