//! Single-step decoder of [`Core`]: fetches the parcels, decodes the instruction type and
//! dispatches to the compute, move and flow handlers.
use super::{Core, Fault};
use crate::visa::{bits, classify, word48, Insn, Kind};

/// One fetched instruction with the fields every handler needs.
pub(super) struct Decoded {
    pub pc: u32,
    pub insn: Insn,
    /// The instruction as a 48-bit ISA word (shorter forms are left-aligned).
    pub w: u64,
    /// Whether the `IF cond` field (bits 37..33) holds; unconditional types read it as true.
    pub cond_ok: bool,
    /// The same condition for PEy (only meaningful in SIMD mode, false otherwise).
    pub cond_y: bool,
}

impl Decoded {
    #[inline]
    pub fn kind(&self) -> Kind {
        self.insn.kind
    }
}

impl Core {
    /// Fetches the instruction at `pc`.
    #[inline]
    pub(super) fn fetch(&self, pc: u32) -> Decoded {
        if self.isa_at(pc) {
            return self.fetch_isa(pc);
        }
        let p0 = self.parcel(pc);
        let p1 = self.parcel(pc.wrapping_add(1));
        let (mut kind, mut len) = classify(p0, Some(p1));
        let mut parcels = [p0, 0, 0];
        if len > 1 {
            parcels[1] = p1;
        }
        if len > 2 {
            parcels[2] = self.parcel(pc.wrapping_add(2));
        }
        if let Some(isa) = self.isa_window_parcels(pc) {
            // interrupt vector slot: 48-bit ISA words, least significant parcel first
            parcels = isa;
            (kind, len) = (classify(isa[0], Some(isa[1])).0, 3);
        }
        let w = word48(&parcels[..len]);
        Decoded {
            pc,
            insn: Insn { kind, parcels, len },
            w,
            cond_ok: self.cond(bits(w, 37, 33)),
            cond_y: self.simd() && self.cond_for(self.flags_y, bits(w, 37, 33)),
        }
    }

    /// Runs `op` on PEx when `x_ok` and, in SIMD mode, on PEy when `y_ok`. An operation that is
    /// not modelled faults (or is skipped in lenient mode).
    pub(super) fn on_each_pe(
        &mut self,
        d: &Decoded,
        x_ok: bool,
        y_ok: bool,
        op: impl Fn(&mut Core) -> bool,
    ) -> Result<(), Fault> {
        if x_ok && !op(self) {
            self.unmodelled(d.pc, &d.insn)?;
        }
        if y_ok && self.simd() {
            self.swap_pe();
            let ok = op(self);
            self.swap_pe();
            if !ok {
                self.unmodelled(d.pc, &d.insn)?;
            }
        }
        Ok(())
    }

    /// Runs the compute field `c` of a conditional instruction on each enabled element, each
    /// with its own condition.
    pub(super) fn run_compute(&mut self, d: &Decoded, c: u64) -> Result<(), Fault> {
        self.on_each_pe(d, d.cond_ok, d.cond_y, |core| core.compute(c))
    }

    /// Executes one instruction. Returns `Ok(())` or the reason it could not.
    pub fn step(&mut self) -> Result<(), Fault> {
        if self.steps >= self.irq.due && self.sequencer_event() {
            return Ok(()); // the core slept in IDLE for this step
        }
        let d = self.fetch(self.pc);
        let mut next = d.pc.wrapping_add(if self.isa_at(d.pc) {
            1
        } else {
            d.insn.len as u32
        });
        match d.kind() {
            Kind::T21c if d.insn.parcels[0] == 0x0001 => {}
            // IDLE (0x0081 and 0x0091 both print as `idle` in ADI's elfdump)
            Kind::T21c if d.insn.parcels[0] | 0x10 == 0x0091 => self.enter_idle(),
            // the 48-bit ISA form of IDLE (`0080 0000 0000`, the boot kernel's last word)
            Kind::T21 if d.w & !(1 << 36) == 0x0080_0000_0000 => self.enter_idle(),
            Kind::T19a => self.exec_modify_imm(&d),
            Kind::T21 if d.w == 0 => {}
            // PUSH/POP LOOP, STS, PCSTK; FLUSH CACHE (not modelled) needs nothing
            Kind::T20a => self.exec_stack_op(&d),
            Kind::T13a => self.exec_do_until(&d, next)?,
            Kind::T25 => next = self.exec_type25(&d, next)?,
            Kind::T2a | Kind::T2b | Kind::T2c => self.exec_compute(&d)?,
            Kind::T6n => self.exec_shift_imm(&d)?,
            Kind::T6a => self.exec_shift_move(&d)?,
            Kind::T1a | Kind::T1b => self.exec_dual_move(&d)?,
            Kind::T8a | Kind::T9a | Kind::T9b | Kind::T11a | Kind::T11c | Kind::T12a => {
                next = self.exec_flow(&d, next)?;
            }
            Kind::T17a
            | Kind::T17b
            | Kind::T14a
            | Kind::T15a
            | Kind::T15b
            | Kind::T16a
            | Kind::T16b
            | Kind::T18a
            | Kind::T5a
            | Kind::T5b
            | Kind::T3a
            | Kind::T3b
            | Kind::T3c
            | Kind::T4a
            | Kind::T4b
            | Kind::T7a
            | Kind::T7b => self.exec_move(&d)?,
            _ => return Err(self.fault(d.pc, &d.insn)),
        }
        self.pc = self.finish_step(&d, next);
        self.steps += 1;
        if self.cb_wrap.get() != 0 {
            self.latch_cb_overflow();
        }
        if self.irq.window_len != 0 {
            self.close_window_if_left();
        }
        Ok(())
    }

    /// Types 2a/2b (23-bit compute) and 2c (short compute).
    fn exec_compute(&mut self, d: &Decoded) -> Result<(), Fault> {
        match d.kind() {
            Kind::T2b => {
                let c = bits(d.w, 38, 16);
                self.on_each_pe(d, true, true, |core| core.compute(c))
            }
            Kind::T2c => {
                let c = bits(d.w, 43, 32);
                self.on_each_pe(d, true, true, |core| core.short_compute(c))
            }
            _ => self.run_compute(d, bits(d.w, 22, 0)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::support::core_with;
    use super::*;

    #[test]
    fn nop_forms_advance_by_their_length() {
        // 16-bit NOP (type 21c) then the all-zero 48-bit NOP
        let mut c = core_with(&[0x0001, 0x0000, 0x0000, 0x0000]);
        assert_eq!(c.run(2), None);
        assert_eq!(c.pc, 0x12_0000 + 1 + 3);
    }

    #[test]
    fn conditional_compute_is_skipped_when_the_condition_fails() {
        // IF NE R3 = R1 + R2 (type 2a, cond 16 = NE holds while AZ is clear)
        let program = [0x0120u16, 0x0000, 0x1312];
        let mut c = core_with(&program);
        (c.r[1], c.r[2]) = (4, 5);
        assert_eq!(c.fetch(c.pc).kind(), Kind::T2a);
        assert_eq!(c.step(), Ok(()));
        assert_eq!((c.r[3], c.pc), (9, 0x12_0003));
        let mut skipped = core_with(&program);
        (skipped.r[1], skipped.r[2], skipped.flags.az) = (4, 5, true);
        assert_eq!(skipped.step(), Ok(()));
        assert_eq!(skipped.r[3], 0);
    }

    #[test]
    fn simd_compute_runs_on_both_elements_with_their_own_flags() {
        // R3 = R1 + R2 (type 2a, unconditional TRUE = cond 31): parcels 013e 0000 1312
        let program = [0x013eu16, 0x0000, 0x1312];
        let mut c = core_with(&program);
        c.mode1 = 1 << 21;
        (c.r[1], c.r[2], c.s[1], c.s[2]) = (4, 5, 10, 20);
        assert_eq!(c.step(), Ok(()));
        assert_eq!((c.r[3], c.s[3]), (9, 30));
        assert_eq!((c.flags.az, c.flags_y.az), (false, false));
        // without SIMD only PEx runs
        let mut x_only = core_with(&program);
        (x_only.r[1], x_only.r[2], x_only.s[1], x_only.s[2]) = (4, 5, 10, 20);
        assert_eq!(x_only.step(), Ok(()));
        assert_eq!((x_only.r[3], x_only.s[3]), (9, 0));
    }

    #[test]
    fn simd_conditions_are_evaluated_per_element() {
        // IF EQ R3 = R1 + R2 where only PEy has AZ set
        let program = [0x0100u16, 0x0000, 0x1312];
        let mut c = core_with(&program);
        c.mode1 = 1 << 21;
        (c.r[1], c.r[2], c.s[1], c.s[2]) = (4, 5, 10, 20);
        c.flags_y.az = true;
        assert_eq!(c.step(), Ok(()));
        assert_eq!((c.r[3], c.s[3]), (0, 30), "only PEy meets the condition");
    }

    #[test]
    fn faulting_step_leaves_state_untouched() {
        let mut c = core_with(&[0xd70a, 0x3c23, 0xd70a]);
        let before = (c.pc, c.steps);
        assert!(c.step().is_err());
        assert_eq!((c.pc, c.steps), before);
    }

    #[test]
    fn simd_multiplier_flags_belong_to_each_element() {
        // R3 = R1 * R2 (signed integer): PEx positive, PEy negative product -> MN (ASTAT bit 6) only in ASTATy
        let mut c = core_with(&[0x0000u16]);
        c.mode1 = 1 << 21;
        c.set_astat(0, 0);
        (c.r[1], c.r[2], c.s[1], c.s[2]) = (0x4000_0000, 0x4000_0000, 0xC000_0000, 0x4000_0000);
        c.on_each_pe(&c.fetch(0), true, true, |core| {
            core.compute((1 << 20) | (0x70 << 12) | (3 << 8) | (1 << 4) | 2)
        })
        .unwrap();
        assert_eq!((c.astat(0) >> 6) & 1, 0);
        assert_eq!((c.astat(1) >> 6) & 1, 1);
    }
}
