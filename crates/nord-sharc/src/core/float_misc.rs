//! Float ALU operations that do not round: sign and select operations, compare, exponent and
//! mantissa extraction, and the `RECIPS` / `RSQRTS` seeds (programming reference rev 2.4, 11-30
//! to 11-44). Flag semantics are read from the reference's per-operation tables (the PDF text
//! extraction scrambles the column order of several tables, so the AZ/AN/AV/AS/AI assignment is
//! ASSUMED where the table is not as clear as the one of `Fn = Fx + Fy`).
use super::float::{unpack, Class, Mode, Out, Un, ALL_ONES};
use super::float_seeds::{recip_seed, rsqrt_seed};

fn out(v: u64, an: bool) -> Out {
    Out {
        v,
        az: v & !(1 << 39) == 0,
        an,
        ..Out::default()
    }
}

fn enc(u: Un) -> u64 {
    super::float::repack(u)
}

/// Ordering key: sign-magnitude value as a signed integer, zeros equal.
fn key(u: Un) -> i128 {
    let mag: i128 = match u.class {
        Class::Zero => 0,
        Class::Norm => ((i128::from(u.exp) + 200) << 40) | i128::from(u.man),
        _ => 1 << 100,
    };
    if u.neg {
        -mag
    } else {
        mag
    }
}

/// `PASS`, `-`, `ABS`: `kind` 0, 1, 2. Denormals flush to zero; NaN gives all ones and AI.
pub fn sign_op(a: u64, kind: u8, mode: Mode) -> Out {
    let x = unpack(a, mode.r32);
    if x.class == Class::Nan {
        return Out::invalid();
    }
    let neg = match kind {
        0 => x.neg,
        1 => !x.neg,
        _ => false,
    };
    let mut o = out(enc(Un { neg, ..x }), neg);
    o.as_ = kind == 2 && x.neg;
    o
}

/// `COPYSIGN`, `MIN`, `MAX`, `CLIP` (`kind` 0..3).
pub fn select_op(a: u64, b: u64, kind: u8, mode: Mode) -> Out {
    let (x, y) = (unpack(a, mode.r32), unpack(b, mode.r32));
    if x.class == Class::Nan || y.class == Class::Nan {
        return Out::invalid();
    }
    let r = match kind {
        0 => Un { neg: y.neg, ..x },
        1 | 2 => {
            let pick_x = if key(x) == key(y) {
                // MIN of +0 and -0 is -0, MAX is +0
                (kind == 1) == x.neg
            } else {
                (key(x) < key(y)) == (kind == 1)
            };
            if pick_x {
                x
            } else {
                y
            }
        }
        _ => {
            if key(Un { neg: false, ..x }) < key(Un { neg: false, ..y }) {
                x
            } else {
                Un { neg: x.neg, ..y }
            }
        }
    };
    out(enc(r), r.neg)
}

/// `COMP(Fx, Fy)`: only AZ (equal), AN (less) and AI (NaN) are set; the value is unused.
pub fn compare(a: u64, b: u64, mode: Mode) -> Out {
    let (x, y) = (unpack(a, mode.r32), unpack(b, mode.r32));
    if x.class == Class::Nan || y.class == Class::Nan {
        return Out {
            ai: true,
            ..Out::default()
        };
    }
    Out {
        az: key(x) == key(y),
        an: key(x) < key(y),
        ..Out::default()
    }
}

/// `Rn = MANT Fx`: the mantissa with its hidden bit as an unsigned 1.31 integer.
pub fn mant(a: u64, mode: Mode) -> Out {
    let x = unpack(a, mode.r32);
    match x.class {
        // VERIFIED (simulator): MANT of an infinity is 0x80000000, of a NaN all ones
        Class::Nan | Class::Inf => Out {
            v: if x.class == Class::Inf {
                0x8000_0000
            } else {
                0xFFFF_FFFF
            },
            an: false,
            av: x.class == Class::Inf,
            as_: x.neg && x.class == Class::Inf,
            ai: x.class == Class::Nan,
            ..Out::default()
        },
        _ => Out {
            v: x.man,
            az: x.man == 0,
            as_: x.neg,
            ..Out::default()
        },
    }
}

/// `Rn = LOGB Fx`: the unbiased exponent. Infinity and zero return the float infinities, or the
/// largest integers when ALUSAT is set.
pub fn logb(a: u64, mode: Mode) -> Out {
    let x = unpack(a, mode.r32);
    let special = |v: u64| Out {
        v,
        an: v >> 31 == 1,
        av: true,
        ..Out::default()
    };
    match x.class {
        Class::Nan => Out {
            v: 0xFFFF_FFFF,
            ai: true,
            ..Out::default()
        },
        Class::Inf => special(if mode.sat { 0x7FFF_FFFF } else { 0x7F80_0000 }),
        Class::Zero => special(if mode.sat { 0x8000_0000 } else { 0xFF80_0000 }),
        Class::Norm => {
            let v = u64::from(x.exp as u32);
            Out {
                v,
                az: x.exp == 0,
                an: x.exp < 0,
                ..Out::default()
            }
        }
    }
}

/// NaN input of `RECIPS`/`RSQRTS`: all ones with AI only (VERIFIED: ASTAT 0x...20 in
/// `docs/research/sharc-seeds/seeds_specials_flags.csv`; the other ops' `invalid()` also sets AN).
fn nan_seed() -> Out {
    Out {
        v: ALL_ONES,
        ai: true,
        ..Out::default()
    }
}

/// `Fn = RECIPS Fx`. Exponent `-e - 1`, sign of the input; zero gives infinity with AV; an
/// exponent above +125 (also infinity) gives zero with AZ.
pub fn recips(a: u64, mode: Mode) -> Out {
    let x = unpack(a, mode.r32);
    let sign = u64::from(x.neg) << 39;
    let base = Out {
        an: x.neg,
        ..Out::default()
    };
    match x.class {
        Class::Nan => nan_seed(),
        Class::Zero => Out {
            v: sign | (0xFF << 31),
            av: true,
            ..base
        },
        Class::Inf => Out {
            v: sign,
            az: true,
            ..base
        },
        Class::Norm if x.exp > 125 => Out {
            v: sign,
            az: true,
            ..base
        },
        Class::Norm => {
            let idx = ((x.man >> 24) & 0x7F) as u32;
            let biased = (-x.exp - 1 + 127) as u64;
            Out {
                v: sign | (biased << 31) | (u64::from(recip_seed(idx)) << 23),
                ..base
            }
        }
    }
}

/// `Fn = RSQRTS Fx`. Exponent `-floor(e/2) - 1`; zero gives infinity with AV, +infinity gives
/// zero, NaN or a negative non-zero input gives all ones with AI.
pub fn rsqrts(a: u64, mode: Mode) -> Out {
    let x = unpack(a, mode.r32);
    let sign = u64::from(x.neg) << 39;
    match x.class {
        Class::Nan => nan_seed(),
        Class::Zero => Out {
            v: sign | (0xFF << 31),
            an: x.neg,
            av: true,
            ..Out::default()
        },
        _ if x.neg => Out {
            v: ALL_ONES,
            ai: true,
            ..Out::default()
        },
        Class::Inf => Out {
            az: true,
            ..Out::default()
        },
        Class::Norm => {
            let (k, odd) = (x.exp.div_euclid(2), x.exp.rem_euclid(2) == 1);
            let idx = ((x.man >> 25) & 0x3F) as u32;
            let biased = (-k - 1 + 127) as u64;
            Out {
                v: (biased << 31) | (u64::from(rsqrt_seed(odd, idx)) << 23),
                ..Out::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(x: f32) -> u64 {
        u64::from(x.to_bits()) << 8
    }
    fn val(v: u64) -> f64 {
        f64::from(f32::from_bits((v >> 8) as u32))
    }
    const M: Mode = Mode {
        trunc: false,
        r32: false,
        sat: false,
    };

    #[test]
    fn sign_operations_and_flags() {
        let o = sign_op(f(-2.5), 2, M);
        assert_eq!((o.v, o.as_, o.an), (f(2.5), true, false));
        let o = sign_op(f(0.0), 1, M);
        assert_eq!((o.v, o.az, o.an), (1 << 39, true, true), "-(+0) is -0");
        assert!(sign_op(f(f32::NAN), 0, M).ai);
        // a denormal passes as zero
        assert_eq!(sign_op(f(f32::from_bits(5)), 0, M).v, 0);
    }

    #[test]
    fn select_operations() {
        assert_eq!(select_op(f(1.0), f(-3.0), 0, M).v, f(-1.0));
        assert_eq!(select_op(f(1.0), f(-3.0), 1, M).v, f(-3.0));
        assert_eq!(select_op(f(1.0), f(-3.0), 2, M).v, f(1.0));
        assert_eq!(
            select_op(f(0.0), f(-0.0), 1, M).v,
            1 << 39,
            "MIN(+0,-0) is -0"
        );
        assert_eq!(select_op(f(-0.0), f(0.0), 2, M).v, 0, "MAX(+0,-0) is +0");
        assert_eq!(
            select_op(f(5.0), f(-2.0), 3, M).v,
            f(2.0),
            "CLIP 5 by -2 gives +2"
        );
        assert_eq!(select_op(f(1.0), f(-2.0), 3, M).v, f(1.0));
    }

    #[test]
    fn compare_flags() {
        let o = compare(f(1.0), f(2.0), M);
        assert!(o.an && !o.az);
        assert!(compare(f(2.0), f(2.0), M).az);
        assert!(compare(f(0.0), f(-0.0), M).az);
        assert!(compare(f(f32::NAN), f(1.0), M).ai);
    }

    #[test]
    fn mantissa_and_exponent_extraction() {
        assert_eq!(mant(f(-6.0), M).v, 0xC000_0000);
        assert!(mant(f(-6.0), M).as_);
        assert_eq!(mant(f(f32::INFINITY), M).v, 0x8000_0000);
        assert_eq!(logb(f(8.0), M).v, 3);
        assert_eq!(logb(f(0.25), M).v as u32 as i32, -2);
        assert_eq!(logb(f(0.0), M).v, 0xFF80_0000);
        let sat = Mode { sat: true, ..M };
        assert_eq!(logb(f(0.0), sat).v, 0x8000_0000);
        assert_eq!(logb(f(f32::INFINITY), sat).v, 0x7FFF_FFFF);
    }

    #[test]
    fn recips_seed_is_within_the_documented_accuracy() {
        let mut worst = 0.0f64;
        for e in [-100, -3, 0, 1, 7, 100, 125] {
            for k in 0..4096u32 {
                let x = f32::from_bits((((e + 127) as u32) << 23) | (k << 11));
                let seed = val(recips(f(x), M).v);
                worst = worst.max((seed * f64::from(x) - 1.0).abs());
            }
        }
        // measured ROM: worst relative error 0.00745 (< 2^-7) over these inputs
        assert!(worst <= 1.0 / 128.0, "seed error {worst} exceeds 2^-7");
        assert!(worst > 1.0 / 2048.0, "the model is not suspiciously exact");
        // exponent rule: x = 1.0 (e = 0) gives exponent -1
        let o = recips(f(1.0), M);
        assert_eq!((o.v >> 31) & 0xFF, 126);
        assert_eq!(recips(f(-4.0), M).v >> 39, 1, "sign of the input");
    }

    #[test]
    fn recips_special_values() {
        let o = recips(f(0.0), M);
        assert!(o.av && o.v == 0xFF << 31);
        assert!(recips(f(-0.0), M).an);
        let o = recips(f(f32::from_bits(0x7E80_0000)), M); // 2^126
        assert!(o.az && o.v == 0);
        assert!(recips(f(f32::INFINITY), M).az);
        assert!(recips(f(f32::NAN), M).ai);
    }

    #[test]
    fn rsqrts_seed_is_within_the_documented_accuracy() {
        let mut worst = 0.0f64;
        for e in [-100, -3, -2, 0, 1, 2, 7, 100, 125] {
            for k in 0..4096u32 {
                let x = f32::from_bits((((e + 127) as u32) << 23) | (k << 11));
                let seed = val(rsqrts(f(x), M).v);
                worst = worst.max((seed * f64::from(x).sqrt() - 1.0).abs());
            }
        }
        assert!(worst <= 1.0 / 16.0, "seed error {worst} exceeds 2^-4");
        assert!(worst > 1.0 / 1024.0, "the model is not suspiciously exact");
    }

    #[test]
    fn rsqrts_special_values() {
        let o = rsqrts(f(0.0), M);
        assert!(o.av && o.v == 0xFF << 31);
        assert!(rsqrts(f(f32::INFINITY), M).az);
        let o = rsqrts(f(-4.0), M);
        assert!(o.ai && o.v == ALL_ONES);
        assert!(rsqrts(f(f32::NAN), M).ai);
    }

    #[test]
    fn newton_raphson_division_from_the_seed_converges() {
        // the lib_fdiv iteration of the CCES runtime: r = seed; repeat d = d*r, r = 2 - d*r ...
        let (n, d) = (3.0f64, 7.0f64);
        let mut r = val(recips(f(d as f32), M).v);
        let (mut q, mut dd) = (n, d);
        for _ in 0..4 {
            q *= r;
            dd *= r;
            r = 2.0 - dd;
        }
        assert!((q - n / d).abs() < 1e-7, "{q}");
    }
}
