//! Packed-float and bit operations of the multi-tap reverb core (`0x120D8A`) on the register
//! machine of `fw_leslie_core.rs`.
//!
//! Semantics follow `tools/fx_pe.py` and `tools/fx_rvbmulti_ops.py`, which the oracle checks:
//! `FPACK` / `FUNPACK` convert between the IEEE single and the 16-bit short float
//! ([`super::fw_short_float`]), `FDEP` deposits a bit field and ORs it into the destination word,
//! `BTGL` flips one bit. Integer and shifter operations read the upper 32 bits of a register.

use super::fw_ext40::Fp;
use super::fw_leslie_core::{Core, P, V};
use super::fw_leslie_ops::{deposit, lanes, un};
use super::fw_short_float::{fpack, funpack};

impl<F: Fp> Core<F> {
    /// `Rn = Sm`: the pair with its elements exchanged.
    pub(super) fn swap(&self, a: P<F>) -> P<F> {
        [a[1], a[0]]
    }

    /// `Rn = FPACK Fx`: the single as a 16-bit short float in the low half.
    pub(super) fn fpack_p(&self, a: P<F>) -> P<F> {
        un(a, |x| V::W(fpack(x.wd())))
    }

    /// `Rn = FUNPACK Fx`: the short float of the low half as a single.
    pub(super) fn funpack_p(&self, a: P<F>) -> P<F> {
        un(a, |x| V::W(funpack(x.wd())))
    }

    /// `Rn = Rm OR FDEP Rx BY pos:len`.
    pub(super) fn fdep_or(&self, old: P<F>, a: P<F>, pos: P<F>, len: P<F>) -> P<F> {
        [0, 1].map(|c| V::W(old[c].wd() | deposit(a[c].wd(), pos[c].wd(), len[c].wd())))
    }

    /// `Rn = BTGL Rx BY bit`.
    pub(super) fn ibtgl(&self, a: P<F>, bit: P<F>) -> P<F> {
        lanes(a, bit, |x, b| V::W(x.wd() ^ (1u32 << (b.wd() & 31))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_halves_and_toggle_follow_the_model() {
        let c = Core::<f32>::blank(&[]);
        let one = [V::W(1.0f32.to_bits()); 2];
        let p = c.fpack_p(one);
        assert_eq!(p[0].wd(), 0x3800, "1.0 as a short float");
        assert_eq!(c.funpack_p(p)[1].wd(), 1.0f32.to_bits());
        let both = c.fdep_or(p, p, [V::W(16); 2], [V::W(16); 2]);
        assert_eq!(both[0].wd(), 0x3800_3800, "two lanes in one word");
        assert_eq!(c.swap([V::W(1), V::W(2)])[0].wd(), 2);
        let t = c.ibtgl([V::W(0), V::W(1)], [V::W(0); 2]);
        assert_eq!((t[0].wd(), t[1].wd()), (1, 0));
    }
}
