//! Dual data moves of [`Core`]: Types 1a and 1b, a data-memory access and a program-memory access
//! in one instruction, 1a with a compute in parallel.
//!
//! Layout (ISA bit numbers), VERIFIED against ADI's `elfdump` on 587 instructions of the DSP
//! images (docs 5.34): bit 44 DM direction (1 = store), 43..41 DM index (DAG1), 40..38 DM modify,
//! bit 37 PM direction, 36..33 DM data register, 32..30 PM index (`I8..I15`), 29..27 PM modify
//! (`M8..M15`), 26..23 PM data register, 22..0 compute (Type 1a only). Both accesses use the
//! index before the instruction and post-modify it; stores read their registers before the
//! compute runs.
use super::decode_cache::DualMoveOp;
use super::exec::Decoded;
use super::{Core, Fault};
use crate::visa::bits;

impl Core {
    /// Executes a pre-decoded dual data move (`T1a` / `T1b`).
    pub(super) fn exec_dual_move_op(&mut self, d: &Decoded, op: DualMoveOp) -> Result<(), Fault> {
        let (dmi, dmm) = (op.dmi as usize, op.dmm as usize);
        let (pmi, pmm) = (op.pmi as usize, op.pmm as usize);
        let (dm_reg, pm_reg) = (op.dm_reg, op.pm_reg);
        let (dm_write, pm_write) = (op.dm_write, op.pm_write);
        let (dm_addr, pm_addr) = (
            self.dag_out(dmi, self.i[dmi]),
            self.dag_out(pmi, self.i[pmi]),
        );
        let (dm_next, pm_next) = (self.modified(dmi, dmm), self.modified(pmi, pmm));
        let dm_captured = self.capture(dm_reg);
        let pm_captured = self.capture(pm_reg);
        if let Some(cop) = op.compute {
            // no condition field in this type: bits 37..33 are data, so run on both elements
            // VERIFIED (census diff, 340 cases): a multiplier-plus-ALU compute in this form leaves
            // MN, MV, MU, MI (ASTAT bits 6..9) and the sticky bits MOS..MIS (STKY 6..9) as they
            // were; a multiplier-only compute updates them as usual
            let keep = (self.sys.astat_extra, self.sys.stky);
            self.on_each_pe(d, true, true, |core| core.exec_compute_op(cop))?;
            if bits(op.compute_raw, 22, 22) == 1 {
                const MULT: u32 = 0xF << 6;
                for y in 0..2 {
                    let e = &mut self.sys.astat_extra[y];
                    *e = *e & !MULT | keep.0[y] & MULT;
                    let k = &mut self.sys.stky[y];
                    *k = *k & !MULT | keep.1[y] & MULT;
                }
            }
        }
        self.mem_transfer_with(d, dm_addr, dm_reg, dm_write, dm_captured)?;
        self.mem_transfer_with(d, pm_addr, pm_reg, pm_write, pm_captured)?;
        self.broadcast_load(dmi, dm_reg, dm_write);
        self.broadcast_load(pmi, pm_reg, pm_write);
        self.i[dmi] = dm_next;
        self.i[pmi] = pm_next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::support::core_at_zero;

    /// ADI: `2cc0 921b c004` is `MRF=MRF+r0*r4(ssf), r0=dm(i6,m3), r4=pm(i10,m10)`, a step of a
    /// fixed-point dot product (ORG Core2 at `0x249170`): the multiply uses the old R0 and R4, the
    /// loads then replace them, and both indices are post-modified.
    #[test]
    fn dot_product_step_uses_old_registers_then_loads_and_post_modifies() {
        let mut c = core_at_zero(&[0x2cc0, 0x921b, 0xc004]);
        (c.i[6], c.m[3], c.i[10], c.m[10]) = (0x9_0100, 1, 0x9_0300, 2);
        (c.r[0], c.r[4]) = (0x4000_0000, 0x4000_0000);
        c.dm_write(0x9_0100, 7).unwrap();
        c.dm_write(0x9_0300, 9).unwrap();
        assert!(c.step().is_ok());
        assert_eq!(c.mr[0] >> 32, 0x2000_0000, "MRF += 0.5 * 0.5");
        assert_eq!((c.r[0], c.r[4]), (7, 9));
        assert_eq!((c.i[6], c.i[10], c.pc), (0x9_0101, 0x9_0302, 3));
    }

    /// ADI: `2c40 9211 4000` is `MRF=0, r0=dm(i6,m1), r4=pm(i10,m10)`.
    #[test]
    fn mrf_clear_with_two_loads() {
        let mut c = core_at_zero(&[0x2c40, 0x9211, 0x4000]);
        c.mr[0] = 12345;
        (c.i[6], c.m[1], c.i[10], c.m[10]) = (0x9_0100, 1, 0x9_0300, 1);
        c.dm_write(0x9_0100, 3).unwrap();
        c.dm_write(0x9_0300, 4).unwrap();
        assert!(c.step().is_ok());
        assert_eq!((c.mr[0], c.r[0], c.r[4]), (0, 3, 4));
    }

    /// A store reads its register before the same instruction's compute changes it.
    /// `dm(i7,m3)=r6` with a compute writing R6: built from the verified field positions.
    #[test]
    fn stores_read_registers_before_the_compute() {
        // MRF = MRF + R0*R4 (SSF) is compute 0x1bc004; make the DM half a store of R0 (bit 44 = 1,
        // register field 36..33 = 0) and keep the PM half a load of R4: 2cc0 921b c004 with the
        // store bit set is 3cc0...: bits 47..44 = 0011.
        let mut c = core_at_zero(&[0x3cc0, 0x921b, 0xc004]);
        (c.i[6], c.m[3], c.i[10], c.m[10]) = (0x9_0100, 1, 0x9_0300, 1);
        (c.r[0], c.r[4]) = (0x4000_0000, 0x4000_0000);
        c.dm_write(0x9_0300, 9).unwrap();
        assert!(c.step().is_ok());
        assert_eq!(c.dm_read(0x9_0100), Ok(0x4000_0000), "the old R0 is stored");
    }

    /// ADI: `3d89 c83f` is `dm(i6,m6)=r4, r0=pm(i15,m9)` (Type 1b, no compute).
    #[test]
    fn type_1b_has_no_compute_field() {
        let mut c = core_at_zero(&[0x3d89, 0xc83f]);
        (c.i[6], c.m[6], c.i[15], c.m[9]) = (0x9_0100, 2, 0x9_0400, 2);
        c.r[4] = 9;
        c.dm_write(0x9_0400, 33).unwrap();
        assert!(c.step().is_ok());
        assert_eq!((c.dm_read(0x9_0100), c.r[0], c.pc), (Ok(9), 33, 2));
    }

    #[test]
    fn simd_moves_the_pey_registers_to_the_next_words() {
        let mut c = core_at_zero(&[0x3d89, 0xc83f]);
        c.mode1 = 1 << 21;
        (c.i[6], c.m[6], c.i[15], c.m[9]) = (0x9_0100, 2, 0x9_0400, 2);
        (c.r[4], c.s[4]) = (9, 10);
        c.dm_write(0x9_0400, 33).unwrap();
        c.dm_write(0x9_0401, 44).unwrap();
        assert!(c.step().is_ok());
        assert_eq!((c.dm_read(0x9_0100), c.dm_read(0x9_0101)), (Ok(9), Ok(10)));
        assert_eq!((c.r[0], c.s[0]), (33, 44));
    }
}
