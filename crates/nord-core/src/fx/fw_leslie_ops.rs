//! Arithmetic and flag operations of the Leslie register machine (`fw_leslie_core.rs`).
//!
//! Semantics follow `tools/fx_pe.py` and `tools/fx_leslie_ops.py`, which the oracle checks:
//! float results are rounded to the register format `F` ([`super::fw_ext40`]), integer and
//! shifter operations read the upper 32 bits of a register, conditions act per element.

use super::fw_ext40::Fp;
use super::fw_leslie_core::{Core, P, V};

/// Condition flags `(AN, AZ)` of a result, per element.
pub(super) type Flags = ([bool; 2], [bool; 2]);

pub(super) fn lanes<F: Fp>(a: P<F>, b: P<F>, f: impl Fn(V<F>, V<F>) -> V<F>) -> P<F> {
    [f(a[0], b[0]), f(a[1], b[1])]
}

pub(super) fn un<F: Fp>(a: P<F>, f: impl Fn(V<F>) -> V<F>) -> P<F> {
    [f(a[0]), f(a[1])]
}

/// The word mask of `len` bits.
fn mask(len: u32) -> u32 {
    if len >= 32 {
        u32::MAX
    } else {
        (1u32 << len) - 1
    }
}

/// `FEXT`: `len` bits at `pos`, optionally sign extended (`fx_pe.extract`).
fn extract(x: u32, pos: u32, len: u32, signed: bool) -> u32 {
    let mut v = if pos < 32 { x >> pos } else { 0 };
    v &= mask(len);
    if signed && len > 0 && pos + len <= 32 && (v >> (len - 1)) & 1 == 1 {
        v |= !mask(len);
    }
    v
}

/// `FDEP`: the low `len` bits of `x` moved to `pos` (`fx_pe.deposit`).
pub(super) fn deposit(x: u32, pos: u32, len: u32) -> u32 {
    ((u64::from(x & mask(len))) << pos.min(63)) as u32
}

/// A float as the signed 32-bit word of a conversion, all ones when out of range.
fn to_int_word(v: f64) -> u32 {
    if (-2_147_483_648.0..2_147_483_648.0).contains(&v) {
        v as i64 as i32 as u32
    } else {
        u32::MAX
    }
}

impl<F: Fp> Core<F> {
    pub(super) fn fadd(&self, a: P<F>, b: P<F>) -> P<F> {
        lanes(a, b, |x, y| V::F(Self::f(x).add(Self::f(y))))
    }
    pub(super) fn fsub(&self, a: P<F>, b: P<F>) -> P<F> {
        lanes(a, b, |x, y| V::F(Self::f(x).sub(Self::f(y))))
    }
    pub(super) fn fmul(&self, a: P<F>, b: P<F>) -> P<F> {
        lanes(a, b, |x, y| V::F(Self::f(x).mul(Self::f(y))))
    }
    /// `Fn = PASS Fx`: the value as a float, no rounding.
    pub(super) fn fpass(&self, a: P<F>) -> P<F> {
        un(a, |x| V::F(Self::f(x)))
    }
    /// `Rn = Sm`: the pair with its elements exchanged.
    pub(super) fn fabs(&self, a: P<F>) -> P<F> {
        un(a, |x| V::F(F::from_f64(Self::f(x).to_f64().abs())))
    }
    /// `Fn = CLIP Fx BY Fy`: `a` limited to `+-|b|`.
    pub(super) fn fclip(&self, a: P<F>, b: P<F>) -> P<F> {
        lanes(a, b, |x, y| {
            let (p, q) = (Self::f(x).to_f64(), Self::f(y).to_f64().abs());
            V::F(F::from_f64(if p.abs() <= q { p } else { q.copysign(p) }))
        })
    }
    /// `Fn = FLOAT Rx BY scale`.
    pub(super) fn ffloat(&self, a: P<F>, s: P<F>) -> P<F> {
        lanes(a, s, |x, y| {
            let (i, sc) = (x.wd() as i32, y.wd() as i32);
            V::F(F::from_f64(f64::from(i) * 2f64.powi(sc)))
        })
    }
    /// `Rn = FIX Fx BY scale`: round to nearest even, all ones when out of range.
    pub(super) fn ffix(&self, a: P<F>, s: P<F>) -> P<F> {
        lanes(a, s, |x, y| {
            let q = Self::f(x).to_f64();
            if q == 0.0 {
                return V::W(0);
            }
            V::W(to_int_word(
                (q * 2f64.powi(y.wd() as i32)).round_ties_even(),
            ))
        })
    }
    /// `Rn = TRUNC Fx`.
    pub(super) fn ftrunc(&self, a: P<F>) -> P<F> {
        un(a, |x| V::W(to_int_word(Self::f(x).to_f64().trunc())))
    }
    pub(super) fn fext_p(&self, a: P<F>, pos: P<F>, len: P<F>, se: P<F>) -> P<F> {
        [0, 1].map(|c| {
            V::W(extract(
                a[c].wd(),
                pos[c].wd(),
                len[c].wd(),
                se[c].wd() != 0,
            ))
        })
    }
    pub(super) fn fdep_p(&self, a: P<F>, pos: P<F>, len: P<F>) -> P<F> {
        [0, 1].map(|c| V::W(deposit(a[c].wd(), pos[c].wd(), len[c].wd())))
    }
    pub(super) fn iadd(&self, a: P<F>, b: P<F>) -> P<F> {
        lanes(a, b, |x, y| V::W(x.wd().wrapping_add(y.wd())))
    }
    pub(super) fn isub(&self, a: P<F>, b: P<F>) -> P<F> {
        lanes(a, b, |x, y| V::W(x.wd().wrapping_sub(y.wd())))
    }
    pub(super) fn iand(&self, a: P<F>, b: P<F>) -> P<F> {
        lanes(a, b, |x, y| V::W(x.wd() & y.wd()))
    }
    pub(super) fn ipass(&self, a: P<F>) -> P<F> {
        un(a, |x| V::W(x.wd()))
    }

    /// `(AN, AZ)` of a result: words by sign and zero, floats by value.
    pub(super) fn flags(&self, p: P<F>) -> Flags {
        let one = |v: V<F>| match v {
            V::W(w) => (w & 0x8000_0000 != 0, w == 0),
            V::F(f) => (f.to_f64() < 0.0, f.to_f64() == 0.0),
        };
        let (a, b) = (one(p[0]), one(p[1]));
        ([a.0, b.0], [a.1, b.1])
    }
    /// `COMP(Fx, Fy)`: `AN` when `x < y`, `AZ` when equal.
    pub(super) fn comp(&self, a: P<F>, b: P<F>) -> Flags {
        let x = |c: usize| (Self::f(a[c]).to_f64(), Self::f(b[c]).to_f64());
        let (p, q) = (x(0), x(1));
        ([p.0 < p.1, q.0 < q.1], [p.0 == p.1, q.0 == q.1])
    }
    /// `IF NE`: the new value where the last result was not zero.
    pub(super) fn cond_ne(&self, old: P<F>, new: P<F>) -> P<F> {
        [0, 1].map(|c| if self.az[c] { old[c] } else { new[c] })
    }
    /// `IF GE`: the new value where the last `COMP` was not less.
    pub(super) fn cond_ge(&self, old: P<F>, new: P<F>) -> P<F> {
        [0, 1].map(|c| if self.an[c] { old[c] } else { new[c] })
    }
    /// `IF TF`: the new value where the test flag is set.
    pub(super) fn cond_tf(&self, old: P<F>, new: P<F>) -> P<F> {
        [0, 1].map(|c| if self.tf[c] { new[c] } else { old[c] })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_and_deposit_follow_the_model_helpers() {
        assert_eq!(extract(0xFFFF_0000, 16, 4, false), 0xF);
        assert_eq!(extract(0x0000_8000, 15, 1, true), 0xFFFF_FFFF);
        assert_eq!(extract(0x1234, 40, 4, false), 0);
        assert_eq!(deposit(0x1F, 5, 4), 0xF << 5);
        assert_eq!(to_int_word(-1.5f64.trunc()), u32::MAX);
        assert_eq!(to_int_word(3e9), u32::MAX);
    }

    #[test]
    fn conversions_round_and_truncate_like_the_dsp() {
        let c = Core::<f32>::new();
        let f = |x: f32| [V::F(x); 2];
        let w = |x: u32| [V::W(x); 2];
        assert_eq!(c.ffix(f(2.5), w(0))[0].wd(), 2, "ties to even");
        assert_eq!(c.ffix(f(3.5), w(0))[0].wd(), 4);
        assert_eq!(c.ftrunc(f(-2.9))[0].wd(), (-2i32) as u32);
        assert_eq!(c.ffloat(w(5), w(1))[0].wd(), 10.0f32.to_bits());
        assert_eq!(c.fclip(f(3.0), f(-2.0))[0].wd(), 2.0f32.to_bits());
        assert_eq!(c.fclip(f(-3.0), f(2.0))[0].wd(), (-2.0f32).to_bits());
    }

    #[test]
    fn flags_and_conditions_act_per_element() {
        let mut c = Core::<f32>::new();
        let (an, az) = c.flags([V::W(0), V::W(0x8000_0000)]);
        assert_eq!((an, az), ([false, true], [true, false]));
        c.az = az;
        let r = c.cond_ne([V::W(1), V::W(1)], [V::W(2), V::W(2)]);
        assert_eq!((r[0].wd(), r[1].wd()), (1, 2));
    }
}
