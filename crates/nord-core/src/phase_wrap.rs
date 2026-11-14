//! Bit-exact fast phase wrap for the audio path.
//!
//! `x % m` on `f32` is a call to the C library `fmodf` (tens of nanoseconds). Oscillator and LFO
//! phases advance by less than one period per sample, so `x` is nearly always in `[0, 2m)`, where
//! the remainder is `x` or the exact difference `x - m`. [`wrap_pos`] handles those two cases
//! inline and defers to `%` for everything else, so it returns exactly the bits of `x % m`.

/// `x % m` for `m > 0`, bit-identical to the `%` operator.
///
/// * `0 <= x < m`: the remainder is `x` itself (also for `-0.0` and `+0.0`).
/// * `m <= x < 2m`: the quotient is 1 and `x - m` is exact (Sterbenz lemma: `m / 2 <= x <= 2m`).
/// * anything else (negative, `>= 2m`, NaN, infinite): `x % m`.
#[inline(always)]
pub fn wrap_pos(x: f32, m: f32) -> f32 {
    if (0.0..m).contains(&x) {
        x
    } else if (m..2.0 * m).contains(&x) {
        x - m
    } else {
        x % m
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    #[test]
    fn matches_the_remainder_operator_bit_for_bit() {
        let mut state = 0x9E37_79B9u32;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        let specials = [
            0.0f32,
            -0.0,
            1.0,
            -1.0,
            2.0,
            TAU,
            -TAU,
            2.0 * TAU,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MIN_POSITIVE,
            1.0e-30,
            3.0e9,
        ];
        for m in [1.0f32, TAU, 0.75] {
            for x in specials {
                assert_eq!(wrap_pos(x, m).to_bits(), (x % m).to_bits(), "{x} % {m}");
            }
            for _ in 0..200_000 {
                // spread over [-1, 3m) with exact boundaries hit by the bit patterns of m and 2m
                let u = next() as f32 / u32::MAX as f32;
                let x = (u * 4.0 - 1.0) * m;
                assert_eq!(wrap_pos(x, m).to_bits(), (x % m).to_bits(), "{x} % {m}");
            }
            for x in [m, 2.0 * m, f32::from_bits(m.to_bits() - 1)] {
                assert_eq!(wrap_pos(x, m).to_bits(), (x % m).to_bits(), "{x} % {m}");
                let up = f32::from_bits(x.to_bits() + 1);
                assert_eq!(wrap_pos(up, m).to_bits(), (up % m).to_bits(), "{up} % {m}");
            }
        }
    }
}
