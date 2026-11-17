//! Exact-guarded host-float kernels for the hot-block AOT prototype: `Fx + Fy`, `Fx - Fy`,
//! `Fx * Fy` and `FLOAT Rx BY Ry` computed with the host's `f64` when, and only when, the 40-bit
//! result is provably exact, so no rounding step exists to differ. Any other operand class (zero,
//! denormal, infinity, NaN, a non-zero float extension, TRUNC or RND32 mode, an exponent gap that
//! would not fit 53 bits, a result with a non-zero rounding tail, an out-of-range exponent)
//! returns `None` and the caller uses the bit-exact kernels of `aot_fast.rs`.
//!
//! Same idea as the guarded Cranelift path of the JIT (`jit.rs`, `emit_exact_float_op`), applied
//! to the AOT lowering. Provenance: VERIFIED bit-identical to `aot_fast` (and so to the reference
//! kernels) wherever a kernel returns `Some`, by the randomized `guarded_*_equal_fast_kernels`
//! tests below (every `Out` field compared); whether it is *faster* is measured, not assumed, in
//! `docs/research/sharc-aot-native-float.md`.
use super::float::{Mode, Out};

/// Which float kernels a generated block uses (see [`AotTier`](super::AotTier)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kern {
    /// The bit-exact `u64` kernels of `aot_fast.rs` only.
    Exact,
    /// Host `f64` when provably exact, else [`Kern::Exact`].
    Guarded,
    /// NOT bit-exact: host `f32` arithmetic with the 8-bit float extension dropped and
    /// denormals flushed to zero (see the `approx_*` kernels).
    Approx,
}

/// Normal exponent field (1..=254) of a 40-bit float: neither zero/denormal nor infinity/NaN.
#[inline(always)]
fn normal(v: u64) -> bool {
    let e = (v >> 31) & 0xFF;
    e != 0 && e != 255
}

/// The 40-bit float `v` (normal, zero float extension) as `f32`, widened to `f64`.
#[inline(always)]
fn to_f64(v: u64) -> f64 {
    f64::from(f32::from_bits((v >> 8) as u32))
}

/// Packs an exactly representable non-zero `f64` as a 40-bit float: `None` when the exponent
/// leaves the normal range or the 52-bit fraction has bits below the 31 a 40-bit float keeps.
#[inline(always)]
fn pack(x: f64) -> Option<u64> {
    let b = x.to_bits();
    let e = ((b >> 52) & 0x7FF) as i64 - 1023;
    if b & ((1 << 21) - 1) != 0 || !(-126..=127).contains(&e) {
        return None;
    }
    Some((b >> 63) << 39 | ((e + 127) as u64) << 31 | (b >> 21) & 0x7FFF_FFFF)
}

/// `Fx +/- Fy` (like `float::add`) when exact in `f64`.
#[inline(always)]
pub(super) fn guarded_add(a: u64, b: u64, sub: bool, mode: Mode) -> Option<Out> {
    if mode.trunc || mode.r32 || !normal(a) || !normal(b) || (a | b) & 0xFF != 0 {
        return None;
    }
    let gap = ((a >> 31) & 0xFF).abs_diff((b >> 31) & 0xFF);
    if gap > 28 {
        return None;
    }
    let s = if sub {
        to_f64(a) - to_f64(b)
    } else {
        to_f64(a) + to_f64(b)
    };
    if s == 0.0 {
        return None;
    }
    let v = pack(s)?;
    Some(Out {
        v,
        an: v >> 39 == 1,
        ..Out::default()
    })
}

/// `Fx * Fy` (like `float::mul`) when exact in `f64`.
#[inline(always)]
pub(super) fn guarded_mul(a: u64, b: u64, mode: Mode) -> Option<Out> {
    if mode.trunc || mode.r32 || !normal(a) || !normal(b) || (a | b) & 0xFF != 0 {
        return None;
    }
    let v = pack(to_f64(a) * to_f64(b))?;
    let neg = v >> 39 == 1;
    Some(Out {
        v,
        an: neg,
        mn: neg,
        ..Out::default()
    })
}

/// `FLOAT Rx BY scale` (like `float_conv::from_int`): a 32-bit integer times a power of two is
/// always exact; `None` for zero and for exponents outside the normal range.
#[inline(always)]
pub(super) fn guarded_from_int(x: i32, scale: i32) -> Option<Out> {
    if x == 0 {
        return None;
    }
    let m = f64::from(x.unsigned_abs());
    let e = i64::from(((m.to_bits() >> 52) & 0x7FF) as i32 - 1023) + i64::from(scale);
    if !(-126..=127).contains(&e) {
        return None;
    }
    let frac = (m.to_bits() >> 21) & 0x7FFF_FFFF;
    let neg = x < 0;
    Some(Out {
        v: u64::from(neg) << 39 | ((e + 127) as u64) << 31 | frac,
        an: neg,
        ..Out::default()
    })
}

/// Flushes a denormal `f32` to zero (the SHARC has no denormals); NaN and infinity pass.
#[inline(always)]
fn ftz(f: f32) -> f32 {
    if f.abs() < f32::MIN_POSITIVE {
        0.0
    } else {
        f
    }
}

/// The 40-bit float `v` as an `f32` with the extension byte dropped and denormals flushed.
#[inline(always)]
fn approx_in(v: u64) -> f32 {
    ftz(f32::from_bits((v >> 8) as u32))
}

/// An `f32` result as a 40-bit float (zero extension) and its flags: AZ for zero or flushed, AN
/// for the sign, AV for overflow to infinity from finite operands, AU for a flush.
#[inline(always)]
fn approx_out(raw: f32, finite_in: bool) -> Out {
    let f = ftz(raw);
    let neg = f.to_bits() >> 31 == 1 && !f.is_nan();
    let tiny = f == 0.0 && raw != 0.0;
    Out {
        v: u64::from(f.to_bits()) << 8,
        az: f == 0.0,
        an: neg,
        av: f.is_infinite() && finite_in,
        au: tiny,
        ..Out::default()
    }
}

/// `Fx +/- Fy` in `f32`: rounds to 24 bits (RN-even) and drops the extension.
#[inline(always)]
pub(super) fn approx_add(a: u64, b: u64, sub: bool) -> Out {
    let (x, y) = (approx_in(a), approx_in(b));
    approx_out(
        if sub { x - y } else { x + y },
        x.is_finite() && y.is_finite(),
    )
}

/// `Fx * Fy` in `f32`; the multiplier flags MN, MV and MU follow the result.
#[inline(always)]
pub(super) fn approx_mul(a: u64, b: u64) -> Out {
    let (x, y) = (approx_in(a), approx_in(b));
    let o = approx_out(x * y, x.is_finite() && y.is_finite());
    let under = o.az && x != 0.0 && y != 0.0;
    Out {
        mn: o.an,
        mv: o.av,
        mu: under,
        au: under,
        ..o
    }
}

/// `FLOAT Rx BY scale` in `f32`: `x * 2^scale` rounded to 24 bits.
#[inline(always)]
pub(super) fn approx_from_int(x: i32, scale: i32) -> Out {
    let p = f64::from_bits((i64::from(scale.clamp(-1000, 1000)) + 1023).cast_unsigned() << 52);
    approx_out((f64::from(x) * p) as f32, true)
}

#[cfg(test)]
mod tests {
    use super::super::aot_fast::{fast_add, fast_from_int, fast_mul};
    use super::*;

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }

    /// A 40-bit float with a short significand (so sums and products are often exact), a modest
    /// exponent, and sometimes a float extension, zero or infinity.
    fn operand(r: &mut Rng) -> u64 {
        let v = r.next();
        let bits = 1 + v % 24;
        let frac = ((v >> 8) & 0x7F_FFFF) >> (23 - bits.min(23)) << (23 - bits.min(23));
        let exp = 127 + (v >> 40) % 40 - 20;
        let mut f = (v >> 63) << 39 | exp << 31 | frac << 8;
        match (v >> 33) % 16 {
            0 => f |= (v >> 20) & 0xFF,
            1 => f &= 1 << 39,
            2 => f |= 255 << 31,
            _ => {}
        }
        f
    }

    #[test]
    fn guarded_kernels_equal_fast_kernels_and_usually_apply() {
        let mut r = Rng(0x1234_5678_9ABC_DEF1);
        let modes = [
            Mode::from_mode1(0),
            Mode::from_mode1(1 << 15),
            Mode::from_mode1(1 << 16),
        ];
        let (mut add_hits, mut mul_hits) = (0, 0);
        for case in 0..60_000 {
            let (a, b) = (operand(&mut r), operand(&mut r));
            let mode = modes[case % 3];
            for sub in [false, true] {
                if let Some(o) = guarded_add(a, b, sub, mode) {
                    add_hits += 1;
                    assert_eq!(o, fast_add(a, b, sub, mode), "add {a:#x} {b:#x} sub={sub}");
                }
            }
            if let Some(o) = guarded_mul(a, b, mode) {
                mul_hits += 1;
                assert_eq!(o, fast_mul(a, b, mode), "mul {a:#x} {b:#x}");
            }
        }
        assert!(add_hits > 2000 && mul_hits > 2000, "{add_hits} {mul_hits}");
    }

    #[test]
    fn guarded_from_int_equals_fast_from_int() {
        let mut r = Rng(0xCAFE_F00D_1234_5678);
        let mode = Mode::from_mode1(0);
        let mut hits = 0;
        for i in 0..50_000 {
            let v = r.next();
            let x = match i % 4 {
                0 => (v >> 8) as i32,
                1 => ((v >> 8) as i32) >> ((v >> 40) % 32),
                2 => i32::MIN + (v >> 60) as i32,
                _ => (v >> 8) as i32 % 5,
            };
            let scale = [
                -31,
                0,
                1,
                -127,
                120,
                200,
                i32::MAX,
                i32::MIN,
                (v >> 50) as i32 % 300,
            ][i % 9];
            if let Some(o) = guarded_from_int(x, scale) {
                hits += 1;
                assert_eq!(o, fast_from_int(x, scale, mode), "float {x} by {scale}");
            }
        }
        assert!(hits > 10_000, "{hits}");
    }

    #[test]
    fn approx_kernels_stay_within_one_f32_ulp_of_the_exact_ones() {
        let mut r = Rng(0x0DDC_0FFE_E123_4567);
        let mode = Mode::from_mode1(0);
        let (mut checked, mut worst) = (0, 0.0f64);
        for _ in 0..60_000 {
            let (a, b) = (operand(&mut r), operand(&mut r));
            let finite = |v: u64| normal(v) && (v & 0xFF) == 0;
            if !(finite(a) && finite(b)) {
                continue;
            }
            for (approx, exact) in [
                (approx_add(a, b, false), fast_add(a, b, false, mode)),
                (approx_add(a, b, true), fast_add(a, b, true, mode)),
                (approx_mul(a, b), fast_mul(a, b, mode)),
            ] {
                let (av, ev) = (f32::from_bits((approx.v >> 8) as u32), exact_f64(exact.v));
                if ev == 0.0 || !normal(exact.v) {
                    continue;
                }
                let rel = ((f64::from(av) - ev) / ev).abs();
                worst = worst.max(rel);
                checked += 1;
                assert!(rel <= 2f64.powi(-23), "{a:#x} {b:#x}: {av} vs {ev}");
            }
        }
        assert!(checked > 50_000, "{checked}");
        assert!(
            worst > 0.0,
            "the kernels differ only by rounding, so some error must show"
        );
    }

    /// A normal 40-bit float as `f64` (extension kept).
    fn exact_f64(v: u64) -> f64 {
        let m = 1.0 + f64::from((v & 0x7FFF_FFFF) as u32) / f64::from(1u32 << 31);
        let e = ((v >> 31) & 0xFF) as i32 - 127;
        (if v >> 39 == 1 { -1.0 } else { 1.0 }) * m * 2f64.powi(e)
    }

    #[test]
    fn approx_flushes_denormals_and_flags_overflow() {
        let tiny = 1u64 << 31 | 1 << 8; // exponent 1: the smallest normal
        let o = approx_mul(tiny, tiny);
        assert_eq!((o.v, o.az, o.mu), (0, true, true));
        let big = (254u64 << 31) | 1 << 8;
        let o = approx_mul(big, big);
        assert!(o.av && o.v >> 8 == 0x7F80_0000, "{o:?}");
        assert_eq!(approx_from_int(3, 2).v >> 8, 12.0f32.to_bits() as u64);
        assert_eq!(approx_from_int(-1, 0).v >> 8, (-1.0f32).to_bits() as u64);
    }

    #[test]
    fn guard_rejects_what_it_cannot_prove() {
        let one = 127u64 << 31; // 1.0
        let m = Mode::from_mode1(0);
        assert!(guarded_add(one, one, false, m).is_some());
        assert!(guarded_add(one, one, true, m).is_none(), "zero sum");
        assert!(
            guarded_add(one, one | 1, false, m).is_none(),
            "extension byte set"
        );
        assert!(guarded_add(one, 255 << 31, false, m).is_none(), "infinity");
        assert!(guarded_add(one, 0, false, m).is_none(), "zero operand");
        assert!(
            guarded_add(one, 90u64 << 31, false, m).is_none(),
            "gap > 28"
        );
        assert!(
            guarded_mul(one, one, Mode::from_mode1(1 << 16)).is_none(),
            "RND32"
        );
        assert!(
            guarded_mul(one, one, Mode::from_mode1(1 << 15)).is_none(),
            "TRUNC"
        );
        // 1 + 2^-23 squared needs 47 bits: not representable, falls back
        assert!(guarded_mul(one | 1 << 8, one | 1 << 8, m).is_none());
    }
}
