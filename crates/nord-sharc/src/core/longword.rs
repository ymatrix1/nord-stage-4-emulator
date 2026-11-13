//! Long-word (`LW`) data moves: `Rn = DM(...) (LW)` and `DM(...) = Rn (LW)` transfer two
//! consecutive words, register `Rn` with the word at `addr` and register `Rn+1` with `addr+1`.
//!
//! VERIFIED against ADI elfdump of the SY1SY2 image (34 `(LW)` instructions, e.g. `5abe c000 0000`
//! is `dm(i5,m2)=r0 (LW)`, `1180 0009 2196` is `dm(0x92196)=r0 (LW)`, `9688 0160` is
//! `r2=dm(0xffffffe0,i3) (LW)`), and by use: the firmware zeroes `R0` and `R1` and then stores
//! `R0 (LW)` to clear stereo pairs and the 64-bit voice masks. Bit position: bit 39 of the 48-bit
//! word in Types 14a, 15a and 15b, bit 30 in Types 3a and 3b (compare the same instruction with
//! and without `(LW)` in the ADI listing). The pair is `Rn, Rn+1` for an even register code `n`,
//! data registers and index registers alike (`i2=dm(0x94036) (LW)` in the SMP image loads I2 and
//! I3). The transfer itself is `Core::mem_transfer_with` (`moves.rs`): VERIFIED by the census diff
//! (`docs/research/sharc-census-diff.md`) the pair is `Rn, Rn^1` at `addr & !1`, an odd register
//! takes the lower word, and the same rule holds in SIMD mode (the 25 LW shapes of
//! `census_regress.rs`). The Type 4 forms are not modelled.
use super::Core;
use crate::visa::{bits, Kind};

impl Core {
    /// Whether the move instruction carries the `LW` flag.
    pub(super) fn lw_flag(kind: Kind, w: u64) -> bool {
        match kind {
            Kind::T14a | Kind::T15a | Kind::T15b => bits(w, 39, 39) == 1,
            Kind::T3a | Kind::T3b => bits(w, 30, 30) == 1,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::support::core_at_zero;

    /// ADI: `1180 0009 2196` is `dm(0x92196)=r0 (LW)`; the plain form `1100 ...` stores one word.
    #[test]
    fn type_14a_store_writes_the_register_pair() {
        let mut c = core_at_zero(&[0x1180, 0x0009, 0x2196]);
        (c.r[0], c.r[1]) = (0xAAAA, 0xBBBB);
        c.dm_write(0x9_2198, 7).unwrap();
        assert!(c.step().is_ok());
        assert_eq!(c.dm_read(0x9_2196).unwrap(), 0xAAAA);
        assert_eq!(c.dm_read(0x9_2197).unwrap(), 0xBBBB);
        assert_eq!(c.dm_read(0x9_2198).unwrap(), 7);
        let mut plain = core_at_zero(&[0x1100, 0x0009, 0x2196]);
        (plain.r[0], plain.r[1]) = (0xAAAA, 0xBBBB);
        assert!(plain.step().is_ok());
        assert_eq!(plain.dm_read(0x9_2197).unwrap(), 0);
    }

    /// ADI: `1088 0009 219a` is `r8=dm(0x9219a) (LW)`: loads R8 and R9.
    #[test]
    fn type_14a_load_fills_the_register_pair() {
        let mut c = core_at_zero(&[0x1088, 0x0009, 0x219a]);
        c.dm_write(0x9_219a, 0x11).unwrap();
        c.dm_write(0x9_219b, 0x22).unwrap();
        assert!(c.step().is_ok());
        assert_eq!((c.r[8], c.r[9]), (0x11, 0x22));
    }

    /// ADI: `9688 0160` is `r2=dm(0xffffffe0,i3) (LW)` (Type 15b, offset -32).
    #[test]
    fn type_15b_load_fills_the_register_pair() {
        let mut c = core_at_zero(&[0x9688, 0x0160]);
        c.i[3] = 0x9_0100;
        c.dm_write(0x9_0100 - 32, 5).unwrap();
        c.dm_write(0x9_0100 - 31, 6).unwrap();
        assert!(c.step().is_ok());
        assert_eq!((c.r[2], c.r[3]), (5, 6));
    }

    /// ADI: `5abe c000 0000` is `dm(i5,m2)=r0 (LW)`: stereo pair cleared, I5 += M2 (the frame
    /// clear of the SY1SY2 audio ISR with R0 = R1 = 0).
    #[test]
    fn type_3a_store_with_post_modify_clears_a_stereo_pair() {
        let mut c = core_at_zero(&[0x5abe, 0xc000, 0x0000]);
        (c.i[5], c.m[2]) = (0x9_0200, 2);
        (c.r[0], c.r[1]) = (0, 0);
        c.dm_write(0x9_0200, 9).unwrap();
        c.dm_write(0x9_0201, 8).unwrap();
        assert!(c.step().is_ok());
        assert_eq!(
            (c.dm_read(0x9_0200).unwrap(), c.dm_read(0x9_0201).unwrap()),
            (0, 0)
        );
        assert_eq!(c.i[5], 0x9_0202);
        // without the flag (bit 30 of the low word) only the first word is written
        let mut plain = core_at_zero(&[0x5abe, 0x8000, 0x0000]);
        (plain.i[5], plain.m[2]) = (0x9_0200, 2);
        plain.dm_write(0x9_0201, 8).unwrap();
        assert!(plain.step().is_ok());
        assert_eq!(plain.dm_read(0x9_0201).unwrap(), 8);
    }

    /// ADI (SMP image): `1092 0009 4036` is `i2=dm(0x94036) (LW)`: loads I2 and I3.
    #[test]
    fn index_register_pairs_load_together() {
        let mut c = core_at_zero(&[0x1092, 0x0009, 0x4036]);
        c.dm_write(0x9_4036, 0x9_0100).unwrap();
        c.dm_write(0x9_4037, 0x9_0200).unwrap();
        assert!(c.step().is_ok());
        assert_eq!((c.i[2], c.i[3]), (0x9_0100, 0x9_0200));
    }
}
