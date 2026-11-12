//! Data and register transfers of [`Core`]: immediates, DM/PM moves with the various addressing
//! forms, register-to-register moves, `Dreg <-> CDreg` swaps, bit operations and index modify.
use super::exec::Decoded;
use super::{sign_extend, Core, Fault};
use crate::visa::{bits, ureg_load, Kind};

impl Core {
    /// The values a store of `ureg` writes (PEx, and PEy for a data register), read before the
    /// instruction's compute runs: all halves of one instruction read the old registers.
    pub(super) fn capture(&self, ureg: u8) -> (u32, u32) {
        let y = if ureg < 16 { self.s[ureg as usize] } else { 0 };
        (self.read_ureg(ureg), y)
    }

    /// Moves universal register `ureg` to or from data memory at `addr`. In SIMD mode a data
    /// register transfer also moves the PEy register to or from `addr + 1` (ASSUMED from the
    /// duplicated word pairs of the image data and the SIMD description of the reference).
    pub(super) fn mem_transfer(
        &mut self,
        d: &Decoded,
        addr: u32,
        ureg: u8,
        write: bool,
    ) -> Result<(), Fault> {
        let captured = self.capture(ureg);
        self.mem_transfer_with(d, addr, ureg, write, captured)
    }

    /// [`Core::mem_transfer`] storing the values captured before a parallel compute.
    pub(super) fn mem_transfer_with(
        &mut self,
        d: &Decoded,
        addr: u32,
        ureg: u8,
        write: bool,
        captured: (u32, u32),
    ) -> Result<(), Fault> {
        let bad = |address| Fault::BadAddress { pc: d.pc, address };
        let pair = self.simd() && ureg < 16;
        if write {
            self.dm_write(addr, captured.0).map_err(bad)?;
            if pair {
                self.dm_write(addr.wrapping_add(1), captured.1)
                    .map_err(bad)?;
            }
        } else {
            let v = self.dm_read(addr).map_err(bad)?;
            self.write_ureg(ureg, v);
            if pair {
                self.s[ureg as usize] = self.dm_read(addr.wrapping_add(1)).map_err(bad)?;
            }
        }
        Ok(())
    }

    /// Executes the move and modify types: 3a/3b/3c, 4a/4b, 5a/5b, 7a/7b, 14a, 15a/15b, 16a,
    /// 17a/17b and 18a. Types with a compute field run it before the memory access.
    pub(super) fn exec_move(&mut self, d: &Decoded) -> Result<(), Fault> {
        let (w, kind) = (d.w, d.kind());
        let with_compute = matches!(kind, Kind::T3a | Kind::T4a | Kind::T5a | Kind::T7a);
        let write_bit = |bit| bits(w, bit, bit) == 1;
        match kind {
            Kind::T17a | Kind::T17b => {
                let (code, imm) = ureg_load(&d.insn).ok_or_else(|| self.fault(d.pc, &d.insn))?;
                let value = if kind == Kind::T17b {
                    sign_extend(imm, 16)
                } else {
                    imm
                };
                self.write_ureg(code, value);
                if self.simd() && code < 16 {
                    self.s[code as usize] = value; // broadcast to PEy
                }
            }
            Kind::T14a => {
                self.mem_transfer(
                    d,
                    bits(w, 31, 0) as u32,
                    bits(w, 38, 32) as u8,
                    write_bit(40),
                )?;
            }
            Kind::T15a => {
                let ii = bits(w, 43, 41) as usize + Self::dag_bank(bits(w, 44, 44));
                let addr = self.dag_out(ii, self.i[ii].wrapping_add(bits(w, 31, 0) as u32));
                self.mem_transfer(d, addr, bits(w, 38, 32) as u8, write_bit(40))?;
            }
            Kind::T15b => {
                // UREG 29..23, signed DATA7 22..16 (VERIFIED, docs 5.28)
                let offset = sign_extend(bits(w, 22, 16) as u32, 7);
                // G bit 37 selects program memory with the DAG2 index (ADI `i5=pm(0x8,i9)`)
                let ii = bits(w, 43, 41) as usize + Self::dag_bank(bits(w, 37, 37));
                let addr = self.dag_out(ii, self.i[ii].wrapping_add(offset));
                self.mem_transfer(d, addr, bits(w, 29, 23) as u8, write_bit(40))?;
            }
            Kind::T16a => {
                // DM/PM(Ii, Mm) = <data32>, post-modify with update
                let bank = Self::dag_bank(bits(w, 37, 37));
                let ii = bits(w, 43, 41) as usize + bank;
                let mi = bits(w, 40, 38) as usize + bank;
                let addr = self.dag_out(ii, self.i[ii]);
                let next_i = self.modified(ii, mi);
                self.dm_write(addr, bits(w, 31, 0) as u32)
                    .map_err(|address| Fault::BadAddress { pc: d.pc, address })?;
                self.i[ii] = next_i;
            }
            Kind::T16b => {
                // DM/PM(Ii, Mm) = <data16> (sign extended like Type 17b), post-modify
                let bank = Self::dag_bank(bits(w, 37, 37));
                let ii = bits(w, 43, 41) as usize + bank;
                let mi = bits(w, 40, 38) as usize + bank;
                let addr = self.dag_out(ii, self.i[ii]);
                let next_i = self.modified(ii, mi);
                let value = sign_extend(bits(w, 31, 16) as u32, 16);
                self.dm_write(addr, value)
                    .map_err(|address| Fault::BadAddress { pc: d.pc, address })?;
                self.i[ii] = next_i;
            }
            Kind::T18a => self.bit_op(d)?,
            Kind::T5a | Kind::T5b if d.cond_ok => {
                if bits(w, 43, 43) == 1 {
                    // `Rx <-> Sy` [, compute]: Rx bits 26..23, Sy bits 41..38 (VERIFIED against
                    // ADI elfdump on 1,087 swaps, docs 5.34). In SIMD mode the swap is the same
                    // exchange (the PEy file is the S file).
                    let (s, r) = (bits(w, 41, 38) as usize, bits(w, 26, 23) as usize);
                    self.run_move_compute(d, with_compute)?;
                    std::mem::swap(&mut self.r[r], &mut self.s[s]);
                } else {
                    let src = ((bits(w, 42, 38) << 2) | bits(w, 32, 31)) as u8;
                    self.move_ureg(bits(w, 29, 23) as u8, src);
                    self.run_move_compute(d, with_compute)?;
                }
            }
            Kind::T3a | Kind::T3b if d.cond_ok => {
                let bank = Self::dag_bank(bits(w, 32, 32));
                let ii = bits(w, 43, 41) as usize + bank;
                let mi = bits(w, 40, 38) as usize + bank;
                let modified = self.modified(ii, mi);
                // post-modify: access at I and update it; pre-modify: access at I + M
                let (addr, update) = if bits(w, 44, 44) == 1 {
                    (self.dag_out(ii, self.i[ii]), Some(modified))
                } else {
                    (self.dag_out(ii, modified), None)
                };
                let ureg = bits(w, 29, 23) as u8;
                let captured = self.capture(ureg);
                self.run_move_compute(d, with_compute)?;
                self.mem_transfer_with(d, addr, ureg, write_bit(31), captured)?;
                if let Some(u) = update {
                    self.i[ii] = u;
                }
            }
            Kind::T3c => {
                // DM(Ii, Mm) <-> Rn, post-modify, DAG1 (bits 43..41 I, 40..38 M, 37 D, 35..32 Rn)
                let (ii, mi) = (bits(w, 43, 41) as usize, bits(w, 40, 38) as usize);
                let addr = self.dag_out(ii, self.i[ii]);
                let next_i = self.modified(ii, mi);
                self.mem_transfer(d, addr, bits(w, 35, 32) as u8, write_bit(37))?;
                self.i[ii] = next_i;
            }
            Kind::T7a | Kind::T7b if d.cond_ok => {
                // [IF cond] [compute,] [Id =] MODIFY(Is, M): G bit 38 selects DAG2, the
                // destination is Is XOR bits 26..24 (stays in the bank), the modify always
                // honours L/B (circular). VERIFIED against ADI elfdump on 51 samples (docs 5.34).
                let bank = Self::dag_bank(bits(w, 38, 38));
                let src = bits(w, 32, 30) as usize + bank;
                let mi = bits(w, 29, 27) as usize + bank;
                let dst = src ^ bits(w, 26, 24) as usize;
                self.run_move_compute(d, with_compute)?;
                self.i[dst] = self.modified_always(src, self.m[mi]);
            }
            Kind::T4a | Kind::T4b if d.cond_ok => {
                let ii = bits(w, 43, 41) as usize + Self::dag_bank(bits(w, 40, 40));
                // DATA6 is a signed offset; the U bit (38) selects post-modify with update
                // (`dm(i6,2)`: access at I, then I += DATA6) instead of the pre-modify offset
                // `dm(2,i6)` (VERIFIED against ADI elfdump, 220 post and 1,576 pre samples, docs 5.34)
                let data = sign_extend(bits(w, 32, 27) as u32, 6);
                let (addr, update) = if bits(w, 38, 38) == 1 {
                    (
                        self.dag_out(ii, self.i[ii]),
                        Some(self.modified_by(ii, data)),
                    )
                } else {
                    (self.dag_out(ii, self.i[ii].wrapping_add(data)), None)
                };
                let ureg = bits(w, 26, 23) as u8;
                let captured = self.capture(ureg);
                self.run_move_compute(d, with_compute)?;
                self.mem_transfer_with(d, addr, ureg, write_bit(39), captured)?;
                if let Some(u) = update {
                    self.i[ii] = u;
                }
            }
            // a conditional form whose condition failed does nothing
            Kind::T5a
            | Kind::T5b
            | Kind::T3a
            | Kind::T3b
            | Kind::T7a
            | Kind::T7b
            | Kind::T4a
            | Kind::T4b => {}
            _ => return Err(self.fault(d.pc, &d.insn)),
        }
        Ok(())
    }

    /// Runs the compute field in bits 22..0 when the instruction type has one.
    #[inline]
    fn run_move_compute(&mut self, d: &Decoded, present: bool) -> Result<(), Fault> {
        if present {
            self.run_compute(d, bits(d.w, 22, 0))
        } else {
            Ok(())
        }
    }

    /// Type 18a: `BIT SET|CLR|TGL|TST|XOR sreg, <data32>`.
    fn bit_op(&mut self, d: &Decoded) -> Result<(), Fault> {
        let sreg = (0x70 | bits(d.w, 35, 32)) as u8;
        let data = bits(d.w, 31, 0) as u32;
        let cur = self.read_ureg(sreg);
        match bits(d.w, 39, 37) {
            0 => self.write_ureg(sreg, cur | data),
            1 => self.write_ureg(sreg, cur & !data),
            2 => self.write_ureg(sreg, cur ^ data),
            4 => self.flags.tf = cur & data == data,
            5 => self.flags.tf = cur == data,
            _ => return Err(self.fault(d.pc, &d.insn)),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::support::{core_at_zero, core_with};

    #[test]
    fn immediates_registers_and_bit_ops() {
        // I0 = 0xc0430; M1 = 1; BIT SET MODE1 0x4f8; BIT CLR MODE1 0x8; NOP
        let mut c = core_with(&[
            0x0F10, 0x000C, 0x0430, 0x0FA1, 0x0001, 0x1402, 0x0000, 0x04F8, 0x1422, 0x0000, 0x0008,
            0x0001,
        ]);
        assert_eq!(c.run(5), None);
        assert_eq!(c.mode1, 0x4F0);
        // BIT SET MODE1 0x4F8 selected the secondary sets (banks.rs): the registers written
        // before it are in the hidden primary set
        assert_eq!((c.i[0], c.m[1]), (0, 0));
        c.write_ureg(0x72, 0);
        assert_eq!((c.i[0], c.m[1]), (0xC_0430, 1));
        assert_eq!(c.steps, 5);
    }

    #[test]
    fn type_7b_modifies_the_index_register_by_the_modify_register() {
        // IF TRUE MODIFY(I1, M3): VISA word 043e 583f at short-word 0
        let mut c = core_at_zero(&[0x043e, 0x583f]);
        c.i[1] = 100;
        c.m[3] = 5;
        assert!(c.step().is_ok());
        assert_eq!(c.i[1], 105);
        assert_eq!(c.pc, 2);
    }

    #[test]
    fn type_3c_loads_and_stores_with_post_modify() {
        // R0 = DM(I2, M2): 16-bit word 0x9?: DMI=2, DMM=2, D=0, bit36=1, DREG=0
        let p: u16 = (0b1001 << 12) | (2 << 9) | (2 << 6) | (1 << 4);
        let mut c = core_at_zero(&[p]);
        c.i[2] = 0x9_0010;
        c.m[2] = 3;
        c.dm_write(0x9_0010, 42).unwrap();
        assert!(c.step().is_ok());
        assert_eq!(c.r[0], 42);
        assert_eq!(c.i[2], 0x9_0013);
    }

    #[test]
    fn type_15b_loads_into_the_register_field_with_a_signed_offset() {
        // I8 = DM(0, I7): 9e08 0c00 (UREG 0x18 in bits 29..23, DATA7 = 0)
        let mut c = core_at_zero(&[0x9e08, 0x0c00]);
        c.i[7] = 0x9_0100;
        c.dm_write(0x9_0100, 0x12_538C).unwrap();
        assert!(c.step().is_ok());
        assert_eq!(
            c.i[8], 0x12_538C,
            "the module record's code pointer lands in I8"
        );
        // R0 = DM(-6, I1): 9208 007a (UREG 0, DATA7 = 0x7a = -6)
        let mut c = core_at_zero(&[0x9208, 0x007a]);
        c.i[1] = 0x9_0200;
        c.dm_write(0x9_0200 - 6, 0xFFFE).unwrap();
        assert!(c.step().is_ok());
        assert_eq!(c.r[0], 0xFFFE);
    }

    #[test]
    fn simd_transfers_move_the_pey_register_to_the_next_word() {
        // R4 = DM(0, I2) in SIMD mode: 3c 16-bit word, DMI=2 DMM=2 D=0
        let p: u16 = (0b1001 << 12) | (2 << 9) | (2 << 6) | (1 << 4) | 4;
        let mut c = core_at_zero(&[p]);
        c.mode1 = 1 << 21;
        c.i[2] = 0x9_0010;
        c.dm_write(0x9_0010, 11).unwrap();
        c.dm_write(0x9_0011, 22).unwrap();
        assert!(c.step().is_ok());
        assert_eq!((c.r[4], c.s[4]), (11, 22));
    }

    /// ADI: `9228 0a88` is `i5=pm(0x8,i9)`: G bit 37 selects PM and the DAG2 index.
    #[test]
    fn type_15b_program_memory_form_uses_the_dag2_index() {
        let mut c = core_at_zero(&[0x9228, 0x0a88]);
        (c.i[1], c.i[9]) = (0x9_0000, 0x9_0200);
        c.dm_write(0x9_0208, 0x1234).unwrap();
        assert!(c.step().is_ok());
        assert_eq!(c.i[5], 0x1234);
    }

    #[test]
    fn type_4_offset_is_signed() {
        // ADI: `6a3f 218d a00f` is `f0=float r0 by r15, r3=dm(0xffffffe4,i5)` (offset -28)
        let mut c = core_at_zero(&[0x6a3f, 0x218d, 0xa00f]);
        c.i[5] = 0x9_0100;
        c.r[0] = 3;
        c.dm_write(0x9_0100 - 28, 77).unwrap();
        assert!(c.step().is_ok());
        assert_eq!(c.r[3], 77);
        assert_eq!(f32::from_bits(c.r[0]), 3.0);
    }

    /// ADI: `6c7e 10bf` is `r1=dm(i6,0x2)`: post-modify, the access is at I6 and I6 moves by 2.
    #[test]
    fn type_4_post_modify_updates_the_index() {
        let mut c = core_at_zero(&[0x6c7e, 0x10bf]);
        c.i[6] = 0x9_0100;
        c.dm_write(0x9_0100, 5).unwrap();
        c.dm_write(0x9_0102, 6).unwrap();
        assert!(c.step().is_ok());
        assert_eq!((c.r[1], c.i[6]), (5, 0x9_0102));
        // the pre-modify offset form `dm(0x2,i6)` leaves the index alone: 6c3e = bit 38 clear
        let mut c = core_at_zero(&[0x6c3e, 0x10bf]);
        c.i[6] = 0x9_0100;
        c.dm_write(0x9_0102, 6).unwrap();
        assert!(c.step().is_ok());
        assert_eq!((c.r[1], c.i[6]), (6, 0x9_0100));
    }

    /// ADI: `043e e73f` is `i4=modify(i3,m4)`: the destination is the source XOR bits 26..24.
    #[test]
    fn modify_destination_is_source_xor_the_field() {
        let mut c = core_at_zero(&[0x043e, 0xe73f]);
        (c.i[3], c.m[4]) = (100, 7);
        assert!(c.step().is_ok());
        assert_eq!((c.i[4], c.i[3]), (107, 100));
    }

    #[test]
    fn type_5a_swaps_a_data_register_with_its_pey_register() {
        // ADI: `7a3e 0480 1001` is `r0=r0+r1, r9<->s8`
        let mut c = core_at_zero(&[0x7a3e, 0x0480, 0x1001]);
        (c.r[9], c.s[8], c.r[0], c.r[1]) = (7, 9, 1, 2);
        assert!(c.step().is_ok());
        assert_eq!((c.r[9], c.s[8], c.r[0]), (9, 7, 3));
        assert_eq!(c.pc, 3);
        // `703e 933f` is `m6=r1`, `753e 93bf` is `m7=s1`
        let mut c = core_at_zero(&[0x703e, 0x933f, 0x753e, 0x93bf]);
        (c.r[1], c.s[1]) = (11, 22);
        assert_eq!(c.run(2), None);
        assert_eq!((c.m[6], c.m[7]), (11, 22));
    }
}
