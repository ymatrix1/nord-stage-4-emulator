//! Jump modifiers `(LA)` and `(CI)` and the compiler instructions of Type 25 (`CJUMP`, `RFRAME`).
//!
//! Provenance: bit positions VERIFIED against ADI `elfdump` of probes assembled with `easm21k
//! -proc ADSP-21489` (`jump (pc,4)(la,ci)` is `077e 0100 0004`, `jump (m8,i8)(ci)` `083e 013f`,
//! `cjump (pc,0x10)(db)` `1844 0000 0010`, `cjump 0x1234(db)` `1804 0000 1234`, `rframe` `1900
//! 0000 0000` / `1901`). The register transfers are the ones listed in PRM Table 9-2 (`CJUMP`:
//! `R2 = I6, I6 = I7` plus `S2 = I6` in SIMD mode; `RFRAME`: `I7 = I6, I6 = DM(0, I6)`); the
//! text of the table is garbled in the PDF, so the `RFRAME` pair is ASSUMED to read the old `I6`
//! for both transfers.
use super::exec::Decoded;
use super::{sign_extend, Core, Fault};
use crate::visa::bits;

impl Core {
    /// `(LA)` pops the innermost loop (and its PC stack entry) when the jump is taken; `(CI)`
    /// clears the interrupt being serviced without leaving the routine (PRM 4-36): the latch and
    /// mask pointer bits are cleared and the status stack is popped, the return address stays so
    /// that a later `RTS (LR)` returns. Calls carry neither modifier.
    pub(super) fn apply_jump_modifiers(&mut self, d: &Decoded) {
        if bits(d.w, 39, 39) == 1 {
            return;
        }
        if bits(d.w, 38, 38) == 1 {
            self.loops.pop();
        }
        if bits(d.w, 24, 24) == 1 {
            self.return_from_interrupt();
        }
    }

    /// Type 25: `RFRAME`, or `CJUMP` (a jump that also saves the frame pointer).
    pub(super) fn exec_type25(&mut self, d: &Decoded, next: u32) -> Result<u32, Fault> {
        let w = d.w;
        if bits(w, 47, 41) == 0x0C && bits(w, 40, 40) == 1 {
            let frame = self.i[6];
            let saved = self
                .dm_read(frame)
                .map_err(|address| Fault::BadAddress { pc: d.pc, address })?;
            self.i[7] = frame;
            self.i[6] = saved;
            return Ok(next);
        }
        let raw = bits(w, 23, 0) as u32;
        let target = if bits(w, 38, 38) == 1 {
            d.pc.wrapping_add(sign_extend(raw, 24))
        } else {
            raw
        };
        let frame = self.i[6];
        self.r[2] = frame;
        if self.simd() {
            self.s[2] = frame;
        }
        self.i[6] = self.i[7];
        Ok(self.take_branch(target, false, bits(w, 34, 34) == 1, next))
    }
}

#[cfg(test)]
mod tests {
    use super::super::support::{core_at_zero, load};
    use super::super::Core;

    /// ADI: `1844 0000 0010` is `cjump (pc,0x10)(db)`: the jump is delayed by two slots, and
    /// `R2 = I6, I6 = I7` happen at once.
    #[test]
    fn cjump_saves_the_frame_pointer_and_jumps_after_two_delay_slots() {
        let mut c = core_at_zero(&[0x1844, 0x0000, 0x0010, 0x0001, 0x0001, 0x0001]);
        (c.i[6], c.i[7]) = (0x9_0100, 0x9_0200);
        assert!(c.step().is_ok());
        assert_eq!((c.r[2], c.i[6], c.pc), (0x9_0100, 0x9_0200, 3));
        assert_eq!(c.run(2), None);
        assert_eq!(
            c.pc, 0x10,
            "PC-relative target 0 + 0x10 after the delay slots"
        );
        // the absolute form `1804 0000 1234` and SIMD mode (S2 = I6 as well)
        let mut c = core_at_zero(&[0x1804, 0x0000, 0x1234, 0x0001, 0x0001]);
        c.mode1 = 1 << 21;
        (c.i[6], c.i[7]) = (7, 9);
        assert_eq!(c.run(3), None);
        assert_eq!((c.pc, c.r[2], c.s[2], c.i[6]), (0x1234, 7, 7, 9));
    }

    /// ADI: `1901` and `1900 0000 0000` are `rframe`: `I7 = I6, I6 = DM(0, I6)`.
    #[test]
    fn rframe_restores_the_caller_frame() {
        for program in [&[0x1901u16][..], &[0x1900, 0x0000, 0x0000][..]] {
            let mut c = core_at_zero(program);
            c.i[6] = 0x9_0100;
            c.dm_write(0x9_0100, 0x9_0300).unwrap();
            assert!(c.step().is_ok());
            assert_eq!(
                (c.i[7], c.i[6], c.pc),
                (0x9_0100, 0x9_0300, program.len() as u32)
            );
        }
    }

    /// ADI: `077e 0000 0004` is `jump (pc,4)(la)`: it pops the innermost loop.
    #[test]
    fn jump_la_pops_the_loop_stack() {
        // lcntr=5, do (pc,3) until lce; then (la) jump out of the one-instruction body
        let mut c = core_at_zero(&[0x0C00, 0x0500, 0x0006, 0x077E, 0x0000, 0x0008]);
        assert!(c.step().is_ok());
        assert_eq!(c.loops.len(), 1);
        assert!(c.step().is_ok());
        assert_eq!((c.loops.len(), c.pc), (0, 0x3 + 8));
    }

    /// ADI: `063e 0100 0100` is `jump 0x100 (ci)`. The jump retires the interrupt (status stack
    /// popped, in-service state cleared) and keeps the return address for the `RTS (LR)` (`0abf`).
    #[test]
    fn jump_ci_retires_the_interrupt_but_keeps_the_return_address() {
        let mut c = Core::new(&[]);
        c.set_ivt_base(0x4000);
        // vector 10 (IRQ0I): word 0 is `jump 0x100 (ci)`, parcels least significant first
        let w: u64 = 0x063e_0100_0100;
        load(
            &mut c,
            0x4000 + 12 * 10,
            &[w as u16, (w >> 16) as u16, (w >> 32) as u16],
        );
        load(&mut c, 0, &[0x0001; 32]);
        load(&mut c, 0x100, &[0x0ABF]);
        c.mode1 |= 0x1000 | 1 << 21; // IRPTEN and PEYEN
        c.imask |= 1 << 10;
        c.pc = 0x10;
        c.raise_interrupt(10);
        assert!(c.step().is_ok());
        assert_eq!(c.servicing(), None, "(CI) ends the service");
        assert_eq!(c.pc, 0x100);
        assert_eq!(c.return_addresses(), &[0x10]);
        assert_eq!(
            c.mode1 & (1 << 21),
            1 << 21,
            "status stack popped: PEYEN is back"
        );
        assert!(c.step().is_ok());
        assert_eq!(
            (c.pc, c.return_addresses().len()),
            (0x10, 0),
            "RTS (LR) returns"
        );
    }
}
