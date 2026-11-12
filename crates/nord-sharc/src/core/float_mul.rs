//! The floating-point multiplier `Fn = Fx * Fy`: result, special values and the multiplier flags
//! MN, MV, MU, MI.
//!
//! VERIFIED against the CCES 2.12.1 ADSP-21489 simulator by `float_flags.rs` (every pair of 16
//! special values and 256 random pairs, in four MODE1 settings): the multiplier leaves the ALU
//! flags AZ/AV/AN/AS/AI untouched; MN is the sign of the result (also for a zero or a flushed
//! underflow, not for an invalid result); MV marks an overflow, MU a normal-by-normal product that
//! flushes to zero (a denormal or zero operand gives a plain zero), MI an invalid operation
//! (NaN operand, infinity times zero or denormal). An invalid result is 0x7FFFFFFF, except
//! infinity times zero whose result takes the sign of the product (0xFFFFFFFF when negative).
use super::float::{repack, round_pack, unpack, Class, Mode, Out, Un};

/// 40-bit invalid-operation result of the multiplier: sign cleared (the extension byte is
/// ASSUMED all ones, it is not visible through memory).
const INVALID: u64 = 0x7F_FFFF_FFFF;

fn invalid(neg: bool) -> Out {
    Out {
        v: if neg { INVALID | 1 << 39 } else { INVALID },
        mi: true,
        ..Out::default()
    }
}

/// `Fx * Fy`.
pub fn mul(a: u64, b: u64, mode: Mode) -> Out {
    let (x, y) = (unpack(a, mode.r32), unpack(b, mode.r32));
    let neg = x.neg ^ y.neg;
    match (x.class, y.class) {
        (Class::Nan, _) | (_, Class::Nan) => invalid(false),
        (Class::Inf, Class::Zero) | (Class::Zero, Class::Inf) => invalid(neg),
        (Class::Inf, _) | (_, Class::Inf) => Out {
            v: repack(Un { neg, ..x_inf() }),
            mn: neg,
            ..Out::default()
        },
        (Class::Zero, _) | (_, Class::Zero) => Out {
            v: u64::from(neg) << 39,
            mn: neg,
            ..Out::default()
        },
        _ => {
            let o = round_pack(
                neg,
                x.exp + y.exp - 62,
                u128::from(x.man) * u128::from(y.man),
                false,
                mode,
                mode.r32,
            );
            Out {
                mn: neg,
                mv: o.av,
                mu: o.az,
                ..o
            }
        }
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
    use super::*;
    const M: Mode = Mode {
        trunc: false,
        r32: false,
        sat: false,
    };
    fn f(x: f32) -> u64 {
        u64::from(x.to_bits()) << 8
    }

    #[test]
    fn multiplier_flags_and_invalid_results() {
        let o = mul(f(f32::INFINITY), f(-0.0), M);
        assert!(o.mi && !o.mn && o.v >> 8 == 0xFFFF_FFFF, "inf * -0 is -NaN");
        let o = mul(f(f32::NAN), f(-2.0), M);
        assert!(o.mi && o.v >> 8 == 0x7FFF_FFFF);
        let o = mul(f(3.0), f(f32::MAX), M);
        assert!(o.mv && !o.mu && !o.mn && o.v >> 8 == 0x7F80_0000);
        let o = mul(f(f32::MIN_POSITIVE), f(-f32::MIN_POSITIVE), M);
        assert!(o.mu && o.mn && o.v == 1 << 39);
        let o = mul(f(0.0), f(-1.0), M);
        assert!(o.mn && !o.mu && o.v == 1 << 39);
        assert!(
            !o.az && !o.an && !o.ai,
            "the ALU flags are not the multiplier's"
        );
    }
}
