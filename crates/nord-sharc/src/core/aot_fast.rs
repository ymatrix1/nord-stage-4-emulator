//! Native-width float kernels for the hot-block AOT prototype: `Fx + Fy`, `Fx - Fy`, `Fx * Fy`
//! and `FLOAT Rx BY Ry` on 40-bit registers, equal to `float::add`, `float::mul` and
//! `float_conv::from_int` result for result and flag for flag, but with `u64` arithmetic instead
//! of the `u128` of the reference kernels and a direct path for ordinary (normal, non-zero)
//! operands. Zero, denormal, infinite and NaN operands, and results that overflow or flush,
//! delegate to the reference kernel, so the special-value rules stay in one place.
//!
//! Provenance: ASSUMED equal to the reference kernels by construction; the claim "bit-identical"
//! is VERIFIED by the randomized equivalence tests below (`fast_*_equals_reference_*`: operand
//! classes x RND32 x TRUNC, every `Out` field compared) and by the real-firmware differential in
//! `docs/research/sharc-aot-hot-block.md`. The reference kernels themselves are VERIFIED against
//! the CCES simulator (see `float.rs`).
use super::decode_cache::ComputeOp;
use super::float::{self, overflow_value, Mode, Out};
use super::float_conv;
use super::Core;

const NORM_EXP_MAX: u64 = 255;

/// Exponent field (bits 38..31) of a 40-bit float.
#[inline(always)]
fn exp_field(v: u64) -> u64 {
    (v >> 31) & 0xFF
}

/// Whether the exponent field marks a normal number (not zero/denormal, not infinity/NaN).
#[inline(always)]
fn is_normal(v: u64) -> bool {
    let e = exp_field(v);
    e != 0 && e != NORM_EXP_MAX
}

/// `float::round_pack` for a `u64` significand: rounds `m * 2^e2` (plus a non-zero fraction of
/// the last place when `sticky`) to a 24- or 32-bit significand and encodes it.
#[inline(always)]
fn round_pack64(neg: bool, e2: i32, m: u64, sticky: bool, mode: Mode, narrow: bool) -> Out {
    let sign = u64::from(neg) << 39;
    if m == 0 {
        return Out::value(sign, false, true);
    }
    let width: i32 = if narrow { 24 } else { 32 };
    let msb = 63 - m.leading_zeros() as i32;
    let mut exp = e2.saturating_add(msb);
    let shift = msb + 1 - width;
    let mut mant = if shift > 0 {
        let sh = shift as u32;
        let (rem, half) = (m & ((1u64 << sh) - 1), 1u64 << (sh - 1));
        let mut q = m >> sh;
        if !mode.trunc && (rem > half || (rem == half && (sticky || q & 1 == 1))) {
            q += 1;
        }
        q
    } else {
        m << (-shift) as u32
    };
    if mant == 1u64 << width {
        mant >>= 1;
        exp += 1;
    }
    if exp > 127 {
        return Out::value(overflow_value(neg, mode, narrow), true, false);
    }
    if exp < -126 {
        return Out {
            au: true,
            ..Out::value(sign, false, true)
        };
    }
    let frac = mant & ((1u64 << (width - 1)) - 1);
    let frac = if narrow { frac << 8 } else { frac };
    let v = sign | (((exp + 127) as u64) << 31) | frac;
    // a normal result: biased exponent 1..=254, so AZ is clear and AN is the sign
    Out {
        v,
        an: neg,
        ..Out::default()
    }
}

/// `Fx +/- Fy` when an operand is zero or denormal (flushed to zero): the other operand passes
/// through exactly (RND32 off) and a zero sum is `+0`; anything else (infinity, NaN, RND32)
/// delegates to the reference kernel.
#[cold]
#[inline(never)]
fn add_with_zero(a: u64, b: u64, sub: bool, mode: Mode) -> Out {
    let (fa, fb) = (exp_field(a), exp_field(b));
    if mode.r32 || fa == NORM_EXP_MAX || fb == NORM_EXP_MAX {
        return float::add(a, b, sub, false, mode);
    }
    let nb = (b >> 39 == 1) ^ sub;
    match (fa == 0, fb == 0) {
        (true, true) => Out {
            v: u64::from(a >> 39 == 1 && nb) << 39,
            az: true,
            an: a >> 39 == 1 && nb,
            ..Out::default()
        },
        (true, false) => {
            let v = (b & ((1 << 39) - 1)) | u64::from(nb) << 39;
            Out {
                v,
                an: nb,
                ..Out::default()
            }
        }
        _ => Out {
            v: a,
            an: a >> 39 == 1,
            ..Out::default()
        },
    }
}

/// `Fx + Fy` (`sub` = false) or `Fx - Fy` on 40-bit floats, like `float::add(a, b, sub, false, m)`.
#[inline(always)]
pub(super) fn fast_add(a: u64, b: u64, sub: bool, mode: Mode) -> Out {
    let (a, b) = if mode.r32 {
        (a & !0xFF, b & !0xFF)
    } else {
        (a, b)
    };
    if !is_normal(a) || !is_normal(b) {
        return add_with_zero(a, b, sub, mode);
    }
    let (ea, eb) = (exp_field(a) as i32 - 127, exp_field(b) as i32 - 127);
    let (ma, mb) = (
        (1u64 << 31) | (a & 0x7FFF_FFFF),
        (1u64 << 31) | (b & 0x7FFF_FFFF),
    );
    let (na, nb) = (a >> 39 == 1, (b >> 39 == 1) ^ sub);
    let a_big = (ea, ma) >= (eb, mb);
    let ((e_big, m_big, n_big), (e_small, m_small, n_small)) = if a_big {
        ((ea, ma, na), (eb, mb, nb))
    } else {
        ((eb, mb, nb), (ea, ma, na))
    };
    let d = (e_big - e_small) as u32;
    let (big, full) = (m_big << 30, m_small << 30);
    let (small, sticky) = if d >= 62 {
        (0, true)
    } else {
        (full >> d, full & ((1u64 << d) - 1) != 0)
    };
    let m = if n_big == n_small {
        big + small
    } else {
        big - small - u64::from(sticky)
    };
    if m == 0 && !sticky {
        return Out::value(0, false, true);
    }
    round_pack64(n_big, e_big - 61, m, sticky, mode, mode.r32)
}

/// `Fx * Fy` when an operand is zero or denormal: a zero of the product's sign with MN set when it
/// is negative, unless the other operand is infinity or NaN (reference kernel).
#[cold]
#[inline(never)]
fn mul_with_zero(a: u64, b: u64, mode: Mode) -> Out {
    let (fa, fb) = (exp_field(a), exp_field(b));
    if fa == NORM_EXP_MAX || fb == NORM_EXP_MAX {
        return float::mul(a, b, mode);
    }
    let neg = (a >> 39) != (b >> 39);
    Out {
        v: u64::from(neg) << 39,
        mn: neg,
        ..Out::default()
    }
}

/// `Fx * Fy` on 40-bit floats, like `float::mul(a, b, m)`.
#[inline(always)]
pub(super) fn fast_mul(a: u64, b: u64, mode: Mode) -> Out {
    let (a, b) = if mode.r32 {
        (a & !0xFF, b & !0xFF)
    } else {
        (a, b)
    };
    if !is_normal(a) || !is_normal(b) {
        return mul_with_zero(a, b, mode);
    }
    let (ea, eb) = (exp_field(a) as i32 - 127, exp_field(b) as i32 - 127);
    let (ma, mb) = (
        (1u64 << 31) | (a & 0x7FFF_FFFF),
        (1u64 << 31) | (b & 0x7FFF_FFFF),
    );
    let neg = (a >> 39) != (b >> 39);
    let o = round_pack64(neg, ea + eb - 62, ma * mb, false, mode, mode.r32);
    Out {
        mn: neg,
        mv: o.av,
        mu: o.az,
        ..o
    }
}

/// `Fn = FLOAT Rx BY scale` (fixed to float), like `float_conv::from_int(x, scale, m)`.
#[inline(always)]
pub(super) fn fast_from_int(x: i32, scale: i32, mode: Mode) -> Out {
    if x == 0 {
        return float_conv::from_int(x, scale, mode);
    }
    round_pack64(
        x < 0,
        scale,
        u64::from(x.unsigned_abs()),
        false,
        mode,
        false,
    )
}

/// ASTAT bit 10: the last ALU operation was floating-point (set by every float ALU op).
const ASTAT_AF: u32 = 1 << 10;

// Used only by generated block modules (see `aot_hot.rs`) and the tests below.
#[allow(dead_code)]
impl Core {
    /// `Fn = Fx + Fy` / `Fx - Fy` through [`fast_add`], effects identical to `float_alu`.
    #[inline(always)]
    pub(super) fn aot_fadd(&mut self, n: u8, x: u8, y: u8, sub: bool) {
        let mode = Mode::from_mode1(self.mode1);
        let o = fast_add(
            self.f40(usize::from(x)),
            self.f40(usize::from(y)),
            sub,
            mode,
        );
        self.float_result(usize::from(n), o);
        self.sys.astat_extra[0] |= ASTAT_AF;
    }

    /// `Fn = FLOAT Rx BY Ry` through [`fast_from_int`].
    #[inline(always)]
    pub(super) fn aot_float(&mut self, n: u8, x: u8, y: u8) {
        let mode = Mode::from_mode1(self.mode1);
        let (ix, iy) = (self.r[usize::from(x)] as i32, self.r[usize::from(y)] as i32);
        let o = fast_from_int(ix, iy, mode);
        self.float_result(usize::from(n), o);
        self.sys.astat_extra[0] |= ASTAT_AF;
    }

    /// `Fn = Fx * Fy` through [`fast_mul`], effects identical to `float_mul`.
    #[inline(always)]
    pub(super) fn aot_fmul(&mut self, n: u8, x: u8, y: u8) {
        let mode = Mode::from_mode1(self.mode1);
        let o = fast_mul(self.f40(usize::from(x)), self.f40(usize::from(y)), mode);
        self.mul_result(usize::from(n), o);
    }

    /// [`Core::aot_op`] with the native-width float kernels for add, subtract, `FLOAT` and
    /// multiply; every other compute field runs the interpreter's implementation.
    #[inline(always)]
    pub(super) fn aot_op_native(&mut self, op: ComputeOp) {
        match op {
            ComputeOp::FloatAlu { op: 0x81, n, x, y } => self.aot_fadd(n, x, y, false),
            ComputeOp::FloatAlu { op: 0x82, n, x, y } => self.aot_fadd(n, x, y, true),
            ComputeOp::FloatAlu { op: 0xDA, n, x, y } => self.aot_float(n, x, y),
            ComputeOp::FloatMul { n, x, y } => self.aot_fmul(n, x, y),
            other => self.aot_op(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic xorshift stream.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }

    /// A 40-bit float drawn from an operand class: ordinary, near-1, huge/tiny exponents, exact
    /// powers of two, specials (zero, denormal, infinity, NaN) and values with a set extension.
    fn operand(r: &mut Rng) -> u64 {
        let v = r.next();
        let (sign, frac, ext) = ((v >> 63) << 39, (v >> 8) & 0x7FFF_FFFF, v & 0xFF);
        let exp = match (v >> 40) % 16 {
            0 => 0,
            1 => 255,
            2 => 1,
            3 => 254,
            4 | 5 => 127 + r.next() % 5 - 2,
            6 => 100 + r.next() % 55,
            _ => 1 + r.next() % 254,
        };
        let frac = if (v >> 44).is_multiple_of(8) { 0 } else { frac };
        sign | exp << 31 | frac | ext
    }

    fn modes() -> [Mode; 4] {
        [(false, false), (true, false), (false, true), (true, true)].map(|(trunc, r32)| Mode {
            trunc,
            r32,
            sat: false,
        })
    }

    #[test]
    fn fast_add_and_sub_equal_reference_on_random_and_special_operands() {
        let mut r = Rng(0x1234_5678_9ABC_DEF1);
        for mode in modes() {
            for _ in 0..400_000 {
                let (a, b) = (operand(&mut r), operand(&mut r));
                for sub in [false, true] {
                    assert_eq!(
                        fast_add(a, b, sub, mode),
                        float::add(a, b, sub, false, mode),
                        "a={a:#x} b={b:#x} sub={sub} {mode:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn fast_add_equals_reference_for_cancellation_and_exponent_gaps() {
        let mut r = Rng(0xDEAD_BEEF_0BAD_F00D);
        for mode in modes() {
            for gap in 0..70u64 {
                for _ in 0..4_000 {
                    let a = operand(&mut r) & !(0xFFu64 << 31) | (150 + gap.min(100)) << 31;
                    let b = operand(&mut r) & !(0xFFu64 << 31) | 150 << 31;
                    // near-equal mantissas make the subtraction cancel
                    let b = if gap < 3 {
                        b & !0x3FF | (a & 0x3FF) ^ (r.next() & 3)
                    } else {
                        b
                    };
                    for sub in [false, true] {
                        assert_eq!(
                            fast_add(a, b, sub, mode),
                            float::add(a, b, sub, false, mode),
                            "a={a:#x} b={b:#x} sub={sub} {mode:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn fast_mul_equals_reference_including_overflow_and_flush() {
        let mut r = Rng(0x0F0F_1234_ABCD_5555);
        for mode in modes() {
            for _ in 0..600_000 {
                let (a, b) = (operand(&mut r), operand(&mut r));
                assert_eq!(
                    fast_mul(a, b, mode),
                    float::mul(a, b, mode),
                    "a={a:#x} b={b:#x} {mode:?}"
                );
            }
        }
    }

    #[test]
    fn fast_from_int_equals_reference_for_all_scales_used() {
        let mut r = Rng(0x7777_0101_2222_9999);
        for mode in modes() {
            for _ in 0..300_000 {
                let v = r.next();
                let x = match v % 4 {
                    0 => (v >> 8) as i32,
                    1 => ((v >> 8) as i32) >> ((v >> 40) % 32),
                    2 => i32::MIN + (v >> 60) as i32,
                    _ => (v >> 8) as i32 % 5,
                };
                for scale in [
                    -31,
                    0,
                    1,
                    -127,
                    120,
                    -200,
                    200,
                    i32::MAX,
                    i32::MIN,
                    (v >> 50) as i32 % 300 - 150,
                ] {
                    assert_eq!(
                        fast_from_int(x, scale, mode),
                        float_conv::from_int(x, scale, mode),
                        "x={x} scale={scale} {mode:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn native_ops_have_the_same_state_effects_as_the_interpreter_ops() {
        let ops = [
            ComputeOp::FloatAlu {
                op: 0x81,
                n: 3,
                x: 1,
                y: 2,
            },
            ComputeOp::FloatAlu {
                op: 0x82,
                n: 4,
                x: 2,
                y: 1,
            },
            ComputeOp::FloatAlu {
                op: 0xDA,
                n: 5,
                x: 6,
                y: 7,
            },
            ComputeOp::FloatMul { n: 8, x: 1, y: 2 },
            ComputeOp::FloatMul { n: 9, x: 3, y: 4 },
        ];
        let mut r = Rng(0xABCD_EF01_2345_6789);
        for case in 0..3000 {
            let mut slow = Core::new(&[]);
            slow.mode1 = [0, 1 << 15, 1 << 16, 1 << 21][case % 4];
            for k in 0..16 {
                slow.r[k] = (operand(&mut r) >> 8) as u32;
                slow.s[k] = (operand(&mut r) >> 8) as u32;
            }
            slow.r[7] = ((r.next() % 70) as i32 - 35) as u32;
            let mut fast = Core::new(&[]);
            fast.mode1 = slow.mode1;
            fast.r = slow.r;
            fast.s = slow.s;
            for op in ops {
                slow.aot_op(op);
                fast.aot_op_native(op);
                assert_eq!(slow.state_mismatch(&fast), None, "case {case} {op:?}");
            }
        }
    }
}
