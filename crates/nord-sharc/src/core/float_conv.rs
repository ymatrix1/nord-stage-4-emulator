//! Conversions and exponent operations of the float ALU: `FLOAT`, `FIX`/`TRUNC`, `SCALB`, `RND`
//! (programming reference rev 2.4, 11-3x). Same provenance as `float.rs`: FIX/TRUNC, FLOAT and RND
//! VERIFIED by `float_measured.rs`, SCALB ASSUMED.
use super::float::{overflow_value, repack, round_pack, unpack, Class, Mode, Out, Un};

/// `Fn = FLOAT Rx BY scale`: the integer converted with the exponent shifted by `scale`. The
/// rounding boundary is always 40 bits, whatever RND32 says (reference 3-38).
pub fn from_int(x: i32, scale: i32, mode: Mode) -> Out {
    if x == 0 {
        return Out::value(0, false, true);
    }
    round_pack(
        x < 0,
        scale,
        u128::from(x.unsigned_abs()),
        false,
        mode,
        false,
    )
}

/// `Fn = SCALB Fx BY n`: adds `n` to the exponent. Exact, so no rounding.
pub fn scalb(a: u64, n: i32, mode: Mode) -> Out {
    let x = unpack(a, mode.r32);
    match x.class {
        Class::Nan => Out::invalid(),
        Class::Zero | Class::Inf => Out::value(repack(x), false, x.class == Class::Zero),
        Class::Norm => {
            let e = i64::from(x.exp) + i64::from(n);
            if e > 127 {
                Out::value(overflow_value(x.neg, mode, mode.r32), true, false)
            } else if e < -126 {
                Out {
                    au: true,
                    ..Out::value(u64::from(x.neg) << 39, false, true)
                }
            } else {
                Out::value(repack(Un { exp: e as i32, ..x }), false, false)
            }
        }
    }
}

/// `Fn = RND Fx`: round to a 32-bit boundary (24-bit significand).
pub fn rnd(a: u64, mode: Mode) -> Out {
    let x = unpack(a, mode.r32);
    match x.class {
        Class::Nan => Out::invalid(),
        Class::Zero | Class::Inf => Out::value(repack(x), false, x.class == Class::Zero),
        Class::Norm => round_pack(x.neg, x.exp - 31, u128::from(x.man), false, mode, true),
    }
}

/// `Rn = FIX|TRUNC Fx BY scale`. `toward_zero` selects `TRUNC`; `FIX` rounds to nearest even, or
/// toward minus infinity when MODE1 TRUNC is set (reference 11-4x). `v` is the 32-bit result.
pub fn to_int(a: u64, scale: i32, toward_zero: bool, mode: Mode) -> Out {
    let x = unpack(a, mode.r32);
    let max = |neg: bool| if neg { 0x8000_0000 } else { 0x7FFF_FFFF };
    // Out of range: saturated when ALUSAT is set, otherwise all ones with the invalid flag.
    let overflow = |neg: bool| {
        let v = if mode.sat { max(neg) } else { 0xFFFF_FFFF };
        Out {
            v,
            // VERIFIED (simulator): the unsaturated all-ones result sets AV and AI but not AN
            an: mode.sat && v >> 31 == 1,
            av: true,
            ai: !mode.sat,
            ..Out::default()
        }
    };
    match x.class {
        Class::Nan => {
            return Out {
                v: 0xFFFF_FFFF,
                ai: true, // VERIFIED: AI only, no AN
                ..Out::default()
            };
        }
        Class::Inf => return overflow(x.neg),
        Class::Zero => {
            return Out {
                az: true,
                ..Out::default()
            }
        }
        Class::Norm => {}
    }
    let e = i64::from(x.exp) + i64::from(scale);
    if e >= 31 && !(e == 31 && x.neg && x.man == 1 << 31) {
        return overflow(x.neg);
    }
    let shift = (31 - e).min(100) as u32;
    let man = u128::from(x.man);
    let (mut q, rem, half) = (
        man >> shift,
        man & ((1u128 << shift) - 1),
        (1u128 << shift) >> 1,
    );
    let up = if toward_zero {
        false
    } else if mode.trunc {
        x.neg && rem != 0
    } else {
        rem > half || (rem == half && q & 1 == 1)
    };
    q += u128::from(up);
    if q > u128::from(max(x.neg)) {
        return overflow(x.neg);
    }
    let v = if x.neg {
        (q as u32).wrapping_neg()
    } else {
        q as u32
    };
    Out {
        v: u64::from(v),
        az: v == 0,
        an: v >> 31 == 1,
        av: false, // VERIFIED: the exact -2^31 result is not an overflow
        // VERIFIED (simulator): a nonzero input that converts to zero latches STKY AUS
        au: v == 0,
        ..Out::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(x: f32) -> u64 {
        u64::from(x.to_bits()) << 8
    }
    const NEAR: Mode = Mode {
        trunc: false,
        r32: false,
        sat: false,
    };
    const TRUNC: Mode = Mode {
        trunc: true,
        r32: false,
        sat: false,
    };

    #[test]
    fn integer_conversions() {
        assert_eq!(from_int(3, 0, NEAR).v, f(3.0));
        assert_eq!(from_int(-5, 2, NEAR).v, f(-20.0));
        let r = from_int(0, 0, NEAR);
        assert!(r.az && r.v == 0);
        // 2^24 + 1 needs 25 bits: the 40-bit format keeps it exactly
        assert_eq!(from_int((1 << 24) + 1, 0, NEAR).v, f(16_777_216.0) | 0x80);
        assert_eq!(to_int(f(2.5), 0, false, NEAR).v, 2, "ties to even");
        assert_eq!(to_int(f(3.5), 0, false, NEAR).v, 4);
        assert_eq!(
            to_int(f(-2.7), 0, true, NEAR).v as i32,
            -2,
            "TRUNC rounds toward zero"
        );
        assert_eq!(
            to_int(f(-0.3), 0, false, TRUNC).v as i32,
            -1,
            "FIX with TRUNC floors"
        );
        assert_eq!(to_int(f(-0.3), 0, false, NEAR).v, 0);
        assert_eq!(to_int(f(0.5), 31, false, NEAR).v, 1 << 30, "scaled by 31");
    }

    #[test]
    fn fix_overflow_depends_on_alusat() {
        let sat = Mode { sat: true, ..NEAR };
        assert_eq!(to_int(f(3e9), 0, false, sat).v, 0x7FFF_FFFF);
        assert_eq!(to_int(f(-3e9), 0, false, sat).v, 0x8000_0000);
        let r = to_int(f(3e9), 0, false, NEAR);
        assert!(r.ai && r.av && r.v == 0xFFFF_FFFF);
        let r = to_int(f(f32::INFINITY), 0, false, sat);
        assert!(r.av && !r.ai && r.v == 0x7FFF_FFFF);
        assert_eq!(to_int(f(-2147483648.0), 0, false, NEAR).v, 0x8000_0000);
        assert!(to_int(f(f32::NAN), 0, false, sat).ai);
    }

    #[test]
    fn scalb_and_rnd_edges() {
        assert_eq!(scalb(f(1.5), 3, NEAR).v, f(12.0));
        assert!(scalb(f(1.5), 200, NEAR).av);
        assert!(scalb(f(1.5), -200, NEAR).az);
        assert_eq!(scalb(f(0.0), 5, NEAR).v, 0);
        let r = rnd(f(1.0) | 0xC0, NEAR);
        assert_eq!(
            r.v,
            f(f32::from_bits(0x3F80_0001)),
            "above the half rounds up"
        );
        assert_eq!(rnd(f(1.0) | 0xC0, TRUNC).v, f(1.0));
    }
}
