//! The firmware reverb's number format: the DSP's 40-bit register ([`E40`]) or plain `f32`.
//!
//! [`Fp`] is the arithmetic the generated reverb steps (`fw_rvb_steps.rs`) and the half-rate
//! wrapper (`fw_rvb_wrap.rs`) are written against. Two implementations:
//!
//! * [`E40`]: every product and sum is rounded to 32 significant bits with ties to even, a store
//!   to a data word keeps the upper 32 bits (a truncation to 24 significant bits), and a result
//!   below the smallest normal single flushes to zero. VERIFIED word for word against the
//!   `sharc-fx` oracle (`tests_fw_rvb_golden.rs`, the 0x12417E wrapper end to end on the real
//!   reverb instance); the same rules as `sinc::ext40`, but exact: the integer mantissa product
//!   and sum avoid the double rounding of an `f64` detour.
//! * `f32`: the fast path, ordinary single precision (a register keeps 24 bits, stores do not
//!   truncate). Same data flow, error against the 40-bit path measured in `tests_fw_rvb_golden.rs`.
//!
//! Allocation free, no state.

/// A register-file number: operations, and the conversion to and from a 32-bit data word.
pub trait Fp: Copy + Default + std::fmt::Debug {
    /// A data word read into a register (a zero exponent field flushes to zero).
    fn from_word(w: u32) -> Self;
    /// The data word a store keeps (upper 32 bits of the register).
    fn to_word(self) -> u32;
    /// `a + b` rounded to the register format.
    fn add(self, o: Self) -> Self;
    /// `a - b`.
    fn sub(self, o: Self) -> Self;
    /// `a * b`.
    fn mul(self, o: Self) -> Self;
    /// `(a + b) / 2`, one rounding.
    fn avg(self, o: Self) -> Self;

    /// The register value as an `f64` (exact for both formats).
    fn to_f64(self) -> f64;
    /// A register value from an `f64` that is exact in the format (integers of at most 32 bits
    /// times a power of two, or a value taken from [`Fp::to_f64`]).
    fn from_f64(x: f64) -> Self;

    /// The value after a store to memory and a reload.
    fn store(self) -> Self {
        Self::from_word(self.to_word())
    }
    /// A float32 data word as a number.
    fn from_f32(x: f32) -> Self {
        Self::from_word(x.to_bits())
    }
    /// The float32 a store keeps.
    fn to_f32(self) -> f32 {
        f32::from_bits(self.to_word())
    }
}

/// Smallest normal single: smaller results flush to zero (VERIFIED: oracle, 60 calls); the
/// exponent test in `pack` is the same limit.
fn flush32(w: u32) -> u32 {
    if (w >> 23) & 0xFF == 0 {
        0
    } else {
        w
    }
}

impl Fp for f32 {
    fn from_word(w: u32) -> Self {
        f32::from_bits(flush32(w))
    }
    fn to_word(self) -> u32 {
        flush32(self.to_bits())
    }
    fn add(self, o: Self) -> Self {
        Self::from_word((self + o).to_bits())
    }
    fn sub(self, o: Self) -> Self {
        Self::from_word((self - o).to_bits())
    }
    fn mul(self, o: Self) -> Self {
        Self::from_word((self * o).to_bits())
    }
    fn avg(self, o: Self) -> Self {
        Self::from_word(((self + o) * 0.5).to_bits())
    }
    fn to_f64(self) -> f64 {
        f64::from(self)
    }
    fn from_f64(x: f64) -> Self {
        Self::from_word((x as f32).to_bits())
    }
}

/// A 40-bit register value: at most 32 significant bits, held exactly in an `f64`.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct E40(f64);

const MANT: u64 = (1 << 52) - 1;

/// `(negative, 32-bit mantissa with its top bit set, exponent of the mantissa's lsb)`.
fn unpack(x: f64) -> (bool, u64, i32) {
    let b = x.to_bits();
    let m = ((b & MANT) | (1 << 52)) >> 21;
    (b >> 63 != 0, m, ((b >> 52) & 0x7FF) as i32 - 1023 - 31)
}

/// `(-1)^neg * r * 2^lsb` rounded to 32 significant bits (ties to even); flush below [`TINY`].
fn pack(neg: bool, r: u128, lsb: i32) -> E40 {
    if r == 0 {
        return E40(0.0);
    }
    let n = 128 - r.leading_zeros() as i32;
    let (mut q, mut e) = if n > 32 {
        let sh = (n - 32) as u32;
        let (q, rest, half) = (r >> sh, r & ((1u128 << sh) - 1), 1u128 << (sh - 1));
        let up = rest > half || (rest == half && q & 1 == 1);
        (q + u128::from(up), lsb + sh as i32)
    } else {
        (r << (32 - n), lsb - (32 - n))
    };
    if q >> 32 != 0 {
        q >>= 1;
        e += 1;
    }
    let exp = e + 31;
    if exp < -126 {
        return E40(0.0);
    }
    let bits = (u64::from(neg) << 63) | (((exp + 1023) as u64) << 52) | (((q as u64) << 21) & MANT);
    E40(f64::from_bits(bits))
}

impl E40 {
    /// The value as an `f64` (exact).
    pub fn value(self) -> f64 {
        self.0
    }
}

impl Fp for E40 {
    fn from_word(w: u32) -> Self {
        E40(f64::from(f32::from_bits(flush32(w))))
    }
    fn to_word(self) -> u32 {
        if self.0 == 0.0 {
            return 0;
        }
        // keep 23 fraction bits of the 52: truncation toward zero, then exact to f32
        let t = f64::from_bits(self.0.to_bits() & !((1u64 << 29) - 1));
        flush32((t as f32).to_bits())
    }
    fn add(self, o: Self) -> Self {
        if self.0 == 0.0 {
            return o;
        }
        if o.0 == 0.0 {
            return self;
        }
        let (a, b) = (unpack(self.0), unpack(o.0));
        // order by magnitude: x is the larger
        let a_big = (a.2, a.1) >= (b.2, b.1);
        let (x, y) = if a_big { (a, b) } else { (b, a) };
        let d = x.2 - y.2;
        if d > 70 {
            return if a_big { self } else { o };
        }
        let (mx, my) = ((u128::from(x.1)) << d, u128::from(y.1));
        let r = if x.0 == y.0 { mx + my } else { mx - my };
        pack(x.0, r, y.2)
    }
    fn sub(self, o: Self) -> Self {
        self.add(E40(-o.0))
    }
    fn mul(self, o: Self) -> Self {
        if self.0 == 0.0 || o.0 == 0.0 {
            return E40(0.0);
        }
        let (a, b) = (unpack(self.0), unpack(o.0));
        pack(a.0 != b.0, u128::from(a.1 * b.1), a.2 + b.2)
    }
    fn avg(self, o: Self) -> Self {
        let s = self.add(o);
        E40(s.0 * 0.5)
    }
    fn to_f64(self) -> f64 {
        self.0
    }
    fn from_f64(x: f64) -> Self {
        E40(x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(x: f64) -> E40 {
        E40(x)
    }

    #[test]
    fn exact_small_integers_and_signs() {
        assert_eq!(e(3.0).add(e(4.0)).value(), 7.0);
        assert_eq!(e(3.0).sub(e(5.0)).value(), -2.0);
        assert_eq!(e(-3.0).mul(e(4.0)).value(), -12.0);
        assert_eq!(e(1.0).avg(e(2.0)).value(), 1.5);
        assert_eq!(e(2.0).sub(e(2.0)).value(), 0.0);
    }

    #[test]
    fn products_round_to_32_bits_ties_to_even() {
        // (1 + 2^-31) * (1 + 2^-31) = 1 + 2^-30 + 2^-62: rounds to 1 + 2^-30
        let a = e(1.0 + 2f64.powi(-31));
        assert_eq!(a.mul(a).value(), 1.0 + 2f64.powi(-30));
        // 1 + 2^-32 is a tie between 1 and 1 + 2^-31: even is 1
        let t = e(1.0).add(e(2f64.powi(-32)));
        assert_eq!(t.value(), 1.0);
        // a tie that rounds up to the even neighbour: (1 + 2^-31) + 2^-32
        let u = e(1.0 + 2f64.powi(-31)).add(e(2f64.powi(-32)));
        assert_eq!(u.value(), 1.0 + 2f64.powi(-30));
    }

    #[test]
    fn a_much_smaller_operand_does_not_change_the_larger() {
        assert_eq!(e(1.0).add(e(1e-30)).value(), 1.0);
        assert_eq!(e(-1.0).add(e(1e-30)).value(), -1.0);
    }

    #[test]
    fn the_add_and_mul_match_exact_rational_rounding_on_random_values() {
        // reference: f64 with an exactly rounded 32-bit result via u128 big arithmetic
        let mut s = 0x1234_5678_9abc_def0_u64;
        let mut next = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        for _ in 0..20_000 {
            let (a, b) = (next(), next());
            let fa = f32::from_bits(((a >> 32) as u32 & 0xBFFF_FFFF) | 0x2000_0000);
            let fb = f32::from_bits((b as u32 & 0xBFFF_FFFF) | 0x2000_0000);
            let (x, y) = (E40(f64::from(fa)), E40(f64::from(fb)));
            let p = x.mul(y).value();
            let exact = f64::from(fa) * f64::from(fb); // 48 bits: exact in f64
            let r = crate::sinc::ext40::round_sig(exact, 32, false);
            assert_eq!(p, r, "mul {fa} {fb}");
        }
    }

    #[test]
    fn store_truncates_to_24_bits_and_flushes_the_tiny() {
        let x = e(1.0 + 2f64.powi(-23) + 2f64.powi(-30));
        assert_eq!(x.to_word(), (1.0f32 + 2f32.powi(-23)).to_bits());
        assert_eq!(e(-0.0).to_word(), 0);
        assert_eq!(
            E40::from_word(0x0000_0001).value(),
            0.0,
            "denormal word flushes"
        );
        let small = e(2f64.powi(-100)).mul(e(2f64.powi(-100)));
        assert_eq!(small.value(), 0.0, "below the smallest normal");
    }

    #[test]
    fn f32_implementation_flushes_and_averages() {
        assert_eq!(<f32 as Fp>::add(1.0, 2.0), 3.0);
        assert_eq!(<f32 as Fp>::avg(1.0, 2.0), 1.5);
        assert_eq!(<f32 as Fp>::mul(1e-30, 1e-30), 0.0);
        assert_eq!(<f32 as Fp>::from_word(0x0000_0010), 0.0);
        assert_eq!(1.5f32.store(), 1.5);
        assert_eq!(E40::from_f32(0.25).to_f32(), 0.25);
    }
}
