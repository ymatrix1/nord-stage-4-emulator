//! Float arithmetic of the SHARC ALU and multiplier: 40-bit extended precision, the two rounding
//! modes and the flush/overflow rules of the programming reference (rev 2.4, 3-37..3-39 and the
//! "ALU Floating-Point Computations" pages of chapter 11).
//!
//! A register float is a 40-bit value held in a `u64`: bit 39 sign, bits 38..31 biased exponent,
//! bits 30..0 mantissa. The top 32 bits are an IEEE single; the low 8 bits are the extension that
//! `RND32 = 0` (the reset state) keeps between operations. All arithmetic is exact integer
//! arithmetic with a single rounding at the end, so results are deterministic on every platform.
//!
//! Provenance: the rounding, FIX/TRUNC and extension behaviours are VERIFIED against the CCES
//! ADSP-21489 simulator by `float_measured.rs` (912 rows, 0 mismatches); the rest is ASSUMED, see
//! `docs/research/sharc-float.md`.

/// MODE1 bit 13: ALU saturation.
pub const MODE1_ALUSAT: u32 = 1 << 13;
/// MODE1 bit 15: round toward zero instead of to nearest.
pub const MODE1_TRUNC: u32 = 1 << 15;
/// MODE1 bit 16: round float results to a 32-bit instead of a 40-bit boundary.
pub const MODE1_RND32: u32 = 1 << 16;
/// A 40-bit all-ones result: what a NaN input produces ("returns an all 1s result").
pub const ALL_ONES: u64 = 0xFF_FFFF_FFFF;

/// Float-related MODE1 bits.
///
/// VERIFIED: bit positions TRUNC 15, RND32 16, ALUSAT 13 = programming reference rev 2.4 MODE1
/// table (A-5), checked by `mode_bits_follow_the_mode1_register` and by the firmware's
/// `BIT SET/CLR MODE1 0x2000` (ALUSAT) toggles in the DSP image listings (research note 5.34).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mode {
    pub trunc: bool,
    pub r32: bool,
    pub sat: bool,
}

impl Mode {
    pub fn from_mode1(m1: u32) -> Self {
        Mode {
            trunc: m1 & MODE1_TRUNC != 0,
            r32: m1 & MODE1_RND32 != 0,
            sat: m1 & MODE1_ALUSAT != 0,
        }
    }
}

/// Classification of a decoded float operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// Zero, or a denormal (flushed to zero on input).
    Zero,
    Norm,
    Inf,
    Nan,
}

/// A decoded operand: sign, class, unbiased exponent and the 1.31 mantissa (hidden bit at 31).
#[derive(Clone, Copy, Debug)]
pub struct Un {
    pub neg: bool,
    pub class: Class,
    pub exp: i32,
    pub man: u64,
}

/// Decodes a 40-bit float. With `r32` the eight extension bits are first cleared.
pub fn unpack(v: u64, r32: bool) -> Un {
    let v = if r32 { v & !0xFF } else { v } & ALL_ONES;
    let (neg, e, frac) = (v >> 39 == 1, ((v >> 31) & 0xFF) as i32, v & 0x7FFF_FFFF);
    let class = match (e, frac) {
        (0, _) => Class::Zero,
        (255, 0) => Class::Inf,
        (255, _) => Class::Nan,
        _ => Class::Norm,
    };
    let man = if class == Class::Norm {
        (1 << 31) | frac
    } else {
        0
    };
    Un {
        neg,
        class,
        exp: e - 127,
        man,
    }
}

/// Encodes a decoded operand again (zero, normal or infinity; NaN becomes all ones).
pub fn repack(u: Un) -> u64 {
    let sign = u64::from(u.neg) << 39;
    match u.class {
        Class::Zero => sign,
        Class::Norm => sign | (((u.exp + 127) as u64) << 31) | (u.man & 0x7FFF_FFFF),
        Class::Inf => sign | (0xFF << 31),
        Class::Nan => ALL_ONES,
    }
}

/// Result of an operation: the value (40-bit float, or a 32-bit integer in the low bits) and the
/// ASTAT flags it sets. `AC` is always cleared by the float operations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Out {
    pub v: u64,
    pub az: bool,
    pub an: bool,
    pub av: bool,
    pub as_: bool,
    pub ai: bool,
}

impl Out {
    /// A float result with AZ (zero or tiny) and AN derived from the value, plus overflow.
    pub fn value(v: u64, av: bool, tiny: bool) -> Out {
        let class = unpack(v, false).class;
        Out {
            v,
            az: tiny || class == Class::Zero,
            an: v >> 39 == 1 && class != Class::Nan,
            av,
            ..Out::default()
        }
    }

    /// The all-ones NaN result with the invalid flag.
    pub fn invalid() -> Out {
        Out {
            v: ALL_ONES,
            an: true,
            ai: true,
            ..Out::default()
        }
    }
}

/// The value an overflow returns: infinity when rounding to nearest, the largest normal number
/// when truncating.
pub fn overflow_value(neg: bool, mode: Mode, narrow: bool) -> u64 {
    let sign = u64::from(neg) << 39;
    match (mode.trunc, narrow) {
        (false, _) => sign | (0xFF << 31),
        (true, true) => sign | (0xFE << 31) | (0x7F_FFFF << 8),
        (true, false) => sign | (0xFE << 31) | 0x7FFF_FFFF,
    }
}

/// Rounds the exact value `m * 2^e2` (plus a non-zero fraction of the last place when `sticky`)
/// to a 24-bit (`narrow`) or 32-bit significand and encodes it. Overflow gives infinity or the
/// largest normal; a result below the smallest normal gives zero (denormals are flushed).
pub fn round_pack(neg: bool, e2: i32, m: u128, sticky: bool, mode: Mode, narrow: bool) -> Out {
    let sign = u64::from(neg) << 39;
    if m == 0 {
        return Out::value(sign, false, true);
    }
    let width: i32 = if narrow { 24 } else { 32 };
    let msb = 127 - m.leading_zeros() as i32;
    let mut exp = e2 + msb;
    let shift = msb + 1 - width;
    let mut mant = if shift > 0 {
        let sh = shift as u32;
        let (rem, half) = (m & ((1u128 << sh) - 1), 1u128 << (sh - 1));
        let mut q = m >> sh;
        if !mode.trunc && (rem > half || (rem == half && (sticky || q & 1 == 1))) {
            q += 1;
        }
        q
    } else {
        m << (-shift) as u32
    };
    if mant == 1u128 << width {
        mant >>= 1;
        exp += 1;
    }
    if exp > 127 {
        return Out::value(overflow_value(neg, mode, narrow), true, false);
    }
    if exp < -126 {
        return Out::value(sign, false, true);
    }
    let frac = (mant as u64) & ((1u64 << (width - 1)) - 1);
    let frac = if narrow { frac << 8 } else { frac };
    Out::value(sign | (((exp + 127) as u64) << 31) | frac, false, false)
}

/// `Fx + Fy`, `Fx - Fy` and, with `halve`, `(Fx + Fy)/2` (the exponent is decremented before
/// rounding). `invalid` covers NaN inputs and infinities that cannot be added.
pub fn add(a: u64, b: u64, sub: bool, halve: bool, mode: Mode) -> Out {
    let (x, y) = (unpack(a, mode.r32), unpack(b, mode.r32));
    let y_neg = y.neg ^ sub;
    let down = i32::from(halve);
    match (x.class, y.class) {
        (Class::Nan, _) | (_, Class::Nan) => return Out::invalid(),
        (Class::Inf, Class::Inf) if x.neg != y_neg => return Out::invalid(),
        (Class::Inf, _) => return Out::value(repack(x), false, false),
        (_, Class::Inf) => return Out::value(repack(Un { neg: y_neg, ..y }), false, false),
        (Class::Zero, Class::Zero) => {
            return Out::value(u64::from(x.neg && y_neg) << 39, false, true);
        }
        (Class::Zero, _) => {
            let (neg, e2) = (y_neg, y.exp - 31 - down);
            return round_pack(neg, e2, u128::from(y.man), false, mode, mode.r32);
        }
        (_, Class::Zero) => {
            return round_pack(
                x.neg,
                x.exp - 31 - down,
                u128::from(x.man),
                false,
                mode,
                mode.r32,
            );
        }
        _ => {}
    }
    let y = Un { neg: y_neg, ..y };
    let (big, small) = if (x.exp, x.man) >= (y.exp, y.man) {
        (x, y)
    } else {
        (y, x)
    };
    let d = (big.exp - small.exp) as u32;
    let (mb, full) = (u128::from(big.man) << 64, u128::from(small.man) << 64);
    let (ms, sticky) = if d >= 128 {
        (0, true)
    } else {
        (full >> d, full & ((1u128 << d) - 1) != 0)
    };
    let m = if big.neg == small.neg {
        mb + ms
    } else {
        mb - ms - u128::from(sticky)
    };
    if m == 0 && !sticky {
        return Out::value(0, false, true);
    }
    round_pack(big.neg, big.exp - 95 - down, m, sticky, mode, mode.r32)
}

/// `Fx * Fy`.
pub fn mul(a: u64, b: u64, mode: Mode) -> Out {
    let (x, y) = (unpack(a, mode.r32), unpack(b, mode.r32));
    let neg = x.neg ^ y.neg;
    match (x.class, y.class) {
        (Class::Nan, _) | (_, Class::Nan) => Out::invalid(),
        (Class::Inf, Class::Zero) | (Class::Zero, Class::Inf) => Out::invalid(),
        (Class::Inf, _) | (_, Class::Inf) => {
            Out::value(repack(Un { neg, ..x_inf() }), false, false)
        }
        (Class::Zero, _) | (_, Class::Zero) => Out::value(u64::from(neg) << 39, false, true),
        _ => round_pack(
            neg,
            x.exp + y.exp - 62,
            u128::from(x.man) * u128::from(y.man),
            false,
            mode,
            mode.r32,
        ),
    }
}

fn x_inf() -> Un {
    Un {
        neg: false,
        class: Class::Inf,
        exp: 128,
        man: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::super::float_conv::rnd;
    use super::*;

    fn f(x: f32) -> u64 {
        u64::from(x.to_bits()) << 8
    }
    const NEAR: Mode = Mode {
        trunc: false,
        r32: false,
        sat: false,
    };
    const R32: Mode = Mode {
        trunc: false,
        r32: true,
        sat: false,
    };
    const TRUNC: Mode = Mode {
        trunc: true,
        r32: false,
        sat: false,
    };

    #[test]
    fn mode_bits_follow_the_mode1_register() {
        assert_eq!(Mode::from_mode1(0), NEAR);
        assert_eq!(Mode::from_mode1(1 << 15), TRUNC);
        assert_eq!(Mode::from_mode1(1 << 16), R32);
        assert!(Mode::from_mode1(1 << 13).sat);
        assert_eq!(Mode::from_mode1(0x4F8), NEAR, "other bits do not matter");
    }

    #[test]
    fn simple_arithmetic_matches_ieee_single() {
        for (a, b) in [
            (1.5f32, 2.25),
            (-3.0, 0.1),
            (1e30, 1e-30),
            (7.0, -7.0),
            (0.3, 0.7),
        ] {
            assert_eq!(add(f(a), f(b), false, false, R32).v, f(a + b), "{a}+{b}");
            assert_eq!(add(f(a), f(b), true, false, R32).v, f(a - b), "{a}-{b}");
            assert_eq!(mul(f(a), f(b), R32).v, f(a * b), "{a}*{b}");
        }
    }

    #[test]
    fn extended_precision_keeps_eight_more_bits_than_single() {
        // 1 + 2^-24 is not representable in single precision but is in the 40-bit format.
        let tiny = f(f32::from_bits(0x3380_0000)); // 2^-24
        let s40 = add(f(1.0), tiny, false, false, NEAR);
        assert_eq!(s40.v, f(1.0) | 0x80, "extension holds the 2^-24 bit");
        // with RND32 the same sum rounds to even, which is 1.0
        assert_eq!(add(f(1.0), tiny, false, false, R32).v, f(1.0));
        // RND to a 32-bit boundary drops the extension (tie to even: 1.0)
        assert_eq!(rnd(s40.v, NEAR).v, f(1.0));
    }

    #[test]
    fn rounding_to_nearest_even_and_truncation_differ_in_the_last_place() {
        // 1 + 3 * 2^-25 sits above the halfway point of a 24-bit significand.
        let b = f(3.0 * f32::from_bits(0x3300_0000));
        let trunc_r32 = Mode {
            trunc: true,
            r32: true,
            sat: false,
        };
        assert_eq!(
            add(f(1.0), b, false, false, R32).v,
            f(f32::from_bits(0x3F80_0001))
        );
        assert_eq!(add(f(1.0), b, false, false, trunc_r32).v, f(1.0));
    }

    #[test]
    fn overflow_and_tiny_results() {
        let big = f(f32::MAX);
        let r = mul(big, f(2.0), NEAR);
        assert!(r.av && !r.az);
        assert_eq!(r.v, 0x7F8 << 28, "round to nearest overflows to infinity");
        let r = mul(big, f(2.0), TRUNC);
        assert!(r.av);
        assert_eq!(
            r.v,
            (0xFE << 31) | 0x7FFF_FFFF,
            "truncation returns the largest normal"
        );
        // a denormal result is flushed to zero and flags AZ
        let r = mul(f(f32::MIN_POSITIVE), f(0.25), NEAR);
        assert!(r.az && !r.av);
        assert_eq!(r.v, 0);
        // a denormal input is zero
        let den = f(f32::from_bits(0x0000_1000));
        assert_eq!(add(den, f(1.0), false, false, NEAR).v, f(1.0));
        assert_eq!(mul(den, f(1e10), NEAR).v, 0);
    }

    #[test]
    fn nan_and_infinity_rules() {
        let inf = f(f32::INFINITY);
        assert_eq!(add(inf, f(1.0), false, false, NEAR).v, inf);
        let r = add(inf, f(f32::NEG_INFINITY), false, false, NEAR);
        assert!(r.ai && r.v == ALL_ONES, "inf - inf is invalid");
        let r = add(inf, inf, true, false, NEAR);
        assert!(r.ai, "inf - inf via subtract is invalid");
        let r = mul(inf, f(0.0), NEAR);
        assert!(r.ai);
        let nan = f(f32::NAN);
        assert!(add(nan, f(1.0), false, false, NEAR).ai);
        assert_eq!(mul(f(-2.0), inf, NEAR).v, f(f32::NEG_INFINITY));
    }

    #[test]
    fn exact_cancellation_and_zero_signs() {
        let r = add(f(2.5), f(2.5), true, false, NEAR);
        assert!(r.az && !r.an);
        assert_eq!(r.v, 0);
        assert_eq!(add(f(-0.0), f(-0.0), false, false, NEAR).v, f(-0.0));
        assert_eq!(add(f(-0.0), f(0.0), false, false, NEAR).v, 0);
        // averaging halves the exponent before rounding
        assert_eq!(add(f(3.0), f(5.0), false, true, NEAR).v, f(4.0));
    }

    #[test]
    fn far_apart_operands_keep_the_larger_one() {
        assert_eq!(add(f(1.0), f(1e-30), false, false, NEAR).v, f(1.0));
        assert_eq!(add(f(1.0), f(1e-30), true, false, NEAR).v, f(1.0));
        // truncation of 1 - tiny steps down to the largest 40-bit value below 1.0
        assert_eq!(add(f(1.0), f(1e-30), true, false, TRUNC).v, f(1.0) - 1);
    }
}
