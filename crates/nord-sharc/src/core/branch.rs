//! Program flow of [`Core`]: jumps, calls and returns (with delay slots), `DO UNTIL LCE` loops.
use super::exec::Decoded;
use super::{sign_extend, Core, Fault, LoopFrame, Pending};
use crate::visa::{bits, Kind};

/// Instructions that run after a delayed branch before it takes effect.
const DELAY_SLOTS: u8 = 2;

impl Core {
    /// Takes a branch to `target`. An immediate branch returns the target (after pushing the
    /// return address for a call); a delayed one (J bit 26) is scheduled for after its two delay
    /// slots and execution continues at `next`.
    #[inline]
    fn branch(&mut self, d: &Decoded, target: u32, next: u32) -> u32 {
        let call = bits(d.w, 39, 39) == 1;
        self.apply_jump_modifiers(d);
        self.take_branch(target, call, bits(d.w, 26, 26) == 1, next)
    }

    /// Branch mechanics shared by the jump, call and `CJUMP` forms: a delayed branch is
    /// scheduled for after its two delay slots, an immediate one returns the target.
    pub(super) fn take_branch(&mut self, target: u32, call: bool, delayed: bool, next: u32) -> u32 {
        if delayed {
            self.pending = Some(Pending {
                slots: DELAY_SLOTS + 1,
                target,
                call,
            });
            return next;
        }
        if call {
            self.push_return(next);
        }
        target
    }

    /// Executes types 8a, 9a/9b (jump and call), 11a/11c (return) and 12a (loop set-up) and
    /// returns the pc of the following instruction.
    pub(super) fn exec_flow(&mut self, d: &Decoded, next: u32) -> Result<u32, Fault> {
        let (w, pc) = (d.w, d.pc);
        let relative = bits(w, 40, 40) == 1;
        match d.kind() {
            Kind::T8a if d.cond_ok => {
                let raw = bits(w, 23, 0) as u32;
                let target = if relative {
                    pc.wrapping_add(sign_extend(raw, 24))
                } else {
                    raw
                };
                Ok(self.branch(d, target, next))
            }
            Kind::T9a | Kind::T9b => {
                // JUMP|CALL (Md, Ic) or JUMP|CALL (pc + <reladdr6>); 9a only: E (bit 25) runs the
                // compute when the condition is false instead of when it is true. Indirect
                // target = Ic + Md on DAG2 (PM), ASSUMED from the syntax.
                let else_form = d.kind() == Kind::T9a && bits(w, 25, 25) == 1;
                if d.kind() == Kind::T9a && d.cond_ok != else_form {
                    self.run_compute(d, bits(w, 22, 0))?;
                }
                if !d.cond_ok {
                    return Ok(next);
                }
                let target = if relative {
                    pc.wrapping_add(sign_extend(bits(w, 32, 27) as u32, 6))
                } else {
                    self.i[8 + bits(w, 32, 30) as usize]
                        .wrapping_add(self.m[8 + bits(w, 29, 27) as usize])
                };
                Ok(self.branch(d, target, next))
            }
            Kind::T11a | Kind::T11c => {
                // RTS/RTI [(DB)] [, compute]: a delayed return runs the two following
                // instructions first. DB is bit 38 in the 16-bit form and bit 26 in the 48-bit
                // form (VERIFIED against ADI: `0aff` and `0a3e 0400 0000` are `rts (db..)`); the
                // 48-bit form's bit 25 selects `, ELSE compute` (compute runs when the condition
                // fails).
                let wide = d.kind() == Kind::T11a;
                let compute = bits(w, 22, 0);
                if wide && compute != 0 {
                    let else_form = bits(w, 25, 25) == 1;
                    if else_form {
                        self.on_each_pe(d, !d.cond_ok, !d.cond_y, |core| core.compute(compute))?;
                    } else {
                        self.run_compute(d, compute)?;
                    }
                }
                if !d.cond_ok {
                    return Ok(next);
                }
                let ret = self
                    .call_stack
                    .pop()
                    .ok_or_else(|| self.fault(pc, &d.insn))?;
                if bits(w, 40, 40) == 1 {
                    self.return_from_interrupt(); // RTI: ends the service of the interrupt
                }
                let delayed = if wide {
                    bits(w, 26, 26)
                } else {
                    bits(w, 38, 38)
                };
                if delayed == 1 {
                    self.pending = Some(Pending {
                        slots: DELAY_SLOTS + 1,
                        target: ret,
                        call: false,
                    });
                    Ok(next)
                } else {
                    Ok(ret)
                }
            }
            Kind::T12a => {
                // The end field is always PC-relative (ADI prints no absolute form; bit 23 is not
                // part of the offset: `0x800003` and `0x000003` both end 3 parcels on). Opcode
                // `0x0D` (bit 40) takes the loop count from a universal register (bits 38..32)
                // instead of the 16-bit immediate (bits 39..24).
                let raw = bits(w, 23, 0) as u32;
                let end = pc.wrapping_add(sign_extend(raw & 0x7F_FFFF, 23));
                let count = if bits(w, 40, 40) == 1 {
                    self.read_ureg(bits(w, 38, 32) as u8)
                } else {
                    bits(w, 39, 24) as u32
                };
                if count == 0 {
                    return Err(self.fault(pc, &d.insn));
                }
                self.push_loop(LoopFrame {
                    start: next,
                    end,
                    remaining: count,
                    ..LoopFrame::default()
                });
                Ok(next)
            }
            // conditional flow whose condition failed falls through
            _ => Ok(next),
        }
    }

    /// Enters an interrupt service routine at `vector`: the current pc becomes the return
    /// address of the `RTI`. A harness stand-in for interrupt dispatch (no IRPTL/IMASK/priority
    /// model, no delay-slot or loop-end handling), used to run ISR bodies in isolation.
    pub fn enter_handler(&mut self, vector: u32) {
        self.call_stack.push(self.pc);
        self.pc = vector;
    }

    /// Return addresses of the active calls, outermost first.
    pub fn return_addresses(&self) -> &[u32] {
        &self.call_stack
    }

    /// End-of-step bookkeeping: loop back-edge of the innermost `DO` loop, then the countdown of
    /// a delayed branch. Returns the pc of the next instruction.
    #[inline]
    pub(super) fn finish_step(&mut self, d: &Decoded, mut next: u32) -> u32 {
        if self.loops.last().is_some_and(|f| f.term.is_some()) {
            next = self.arith_loop_step(d.pc, next);
        } else if let Some(frame) = self.loops.last_mut() {
            if d.pc == frame.end && d.kind() != Kind::T12a {
                if frame.remaining > 1 {
                    frame.remaining -= 1;
                    next = frame.start;
                } else {
                    self.loops.pop();
                }
            }
        }
        if let Some(p) = self.pending {
            if p.slots > 1 {
                self.pending = Some(Pending {
                    slots: p.slots - 1,
                    ..p
                });
            } else {
                self.pending = None;
                if p.call {
                    self.push_return(next);
                }
                next = p.target;
            }
        }
        next
    }
}

#[cfg(test)]
mod tests {
    use super::super::support::{core_at_zero, core_with, load};

    #[test]
    fn do_loop_zeroes_memory_like_the_firmware_start_up() {
        // R0 = 0; I0 = 0xc0430; DO (+3) UNTIL LCE, LCNTR = 4; DM(I0, M1) = R0; then NOP
        let mut c = core_with(&[
            0x0F80, 0x0000, 0x0F10, 0x000C, 0x0430, 0x0C00, 0x0480, 0x0003, 0x507E, 0x8000, 0x0000,
            0x0001,
        ]);
        c.m[1] = 1;
        for k in 0..6 {
            c.dm_write(0xC_0430 + k, 0xDEAD_BEEF).unwrap();
        }
        assert_eq!(c.run(2 + 1 + 4 + 1), None);
        for k in 0..4 {
            assert_eq!(c.dm_read(0xC_0430 + k).unwrap(), 0, "word {k}");
        }
        assert_eq!(c.dm_read(0xC_0434).unwrap(), 0xDEAD_BEEF);
        assert_eq!(c.i[0], 0xC_0434);
        assert!(c.loops.is_empty());
    }

    #[test]
    fn enter_handler_pushes_the_pc_and_rti_returns_to_it() {
        // vector at 0x10: RTI (type 11c, 0abe is RTS; RTI is 0b3e in the 48-bit form)
        let mut c = core_at_zero(&[0x0001, 0x0001]);
        super::super::support::load(&mut c, 0x10, &[0x0B3E, 0x0000, 0x0000]);
        c.pc = 1;
        c.enter_handler(0x10);
        assert_eq!((c.pc, c.return_addresses().to_vec()), (0x10, vec![1]));
        assert!(c.step().is_ok());
        assert_eq!(c.pc, 1, "RTI resumes where the interrupt hit");
    }

    #[test]
    fn call_and_return() {
        // CALL 0x120008; NOP ... at 0x120008: R1 = 7 (17b); RTS
        let mut c = core_with(&[0x06BE, 0x0012, 0x0008, 0x0001]);
        load(&mut c, 0x12_0008, &[0x0F81, 0x0007, 0x0A3E, 0x0000, 0x0000]);
        assert_eq!(c.run(3), None);
        assert_eq!(c.r[1], 7);
        assert_eq!(c.pc, 0x12_0003);
    }

    #[test]
    fn type_9b_indirect_call_uses_dag2_index_plus_modify() {
        // IF TRUE CALL (M8, I8): parcels 08be 003f at short-word 0
        let mut c = core_at_zero(&[0x08be, 0x003f]);
        c.i[8] = 0x1000;
        c.m[8] = 0x24;
        assert!(c.step().is_ok());
        assert_eq!(c.pc, 0x1024);
        assert_eq!(c.call_stack, vec![2]);
    }

    #[test]
    fn delayed_call_runs_two_slots_then_jumps_and_returns_after_them() {
        // CALL 0x10 (DB) then two NOP slots: the call takes effect after the slots
        let mut c = core_at_zero(&[0x06be, 0x0400, 0x0010, 0x0001, 0x0001, 0x0001]);
        assert!(c.step().is_ok());
        assert_eq!(c.pc, 3, "the first delay slot comes next");
        assert!(c.step().is_ok());
        assert_eq!(c.pc, 4, "the second delay slot comes next");
        assert!(c.step().is_ok());
        assert_eq!(c.pc, 0x0010);
        assert_eq!(
            c.call_stack,
            vec![5],
            "return address is after the delay slots"
        );
    }

    #[test]
    fn delayed_indirect_jump_without_call_keeps_the_stack_empty() {
        // JUMP (M8, I8) (DB): 08be with the call bit cleared and J set -> 083e 043f
        let mut c = core_at_zero(&[0x083e, 0x043f, 0x0001, 0x0001]);
        c.i[8] = 0x40;
        assert_eq!(c.run(3), None);
        assert_eq!(c.pc, 0x40);
        assert!(c.call_stack.is_empty());
    }

    #[test]
    fn return_addresses_list_the_active_calls() {
        // CALL 0x10 then, at 0x10, CALL 0x20: two nested calls
        let mut c = core_at_zero(&[0x06be, 0x0000, 0x0010]);
        load(&mut c, 0x10, &[0x06be, 0x0000, 0x0020]);
        assert_eq!(c.run(2), None);
        assert_eq!(c.return_addresses(), &[3, 0x13]);
    }

    /// ADI: `0d05 0080 0006` is `lcntr=r5, do (pc,..) until lce`: the count comes from R5 and the
    /// loop body ends 6 parcels on; `0d00 0000 0003` has bit 23 clear and is relative as well.
    #[test]
    fn do_until_takes_its_count_from_a_register_and_is_pc_relative() {
        // DO (pc,3) until lce, lcntr=r0 ; body: R1 = R1 + 1 (type 2c, 1 parcel? use 3a NOP-like word)
        let mut c = core_at_zero(&[0x0d00, 0x0000, 0x0003, 0x0001, 0x0001]);
        c.r[0] = 3;
        // the body is the single 16-bit NOP at parcel 3: ends at pc + 3
        assert_eq!(c.run(1 + 3), None);
        assert_eq!(
            c.pc, 4,
            "three passes of the body, then the code after the loop"
        );
        assert!(c.loops.is_empty());
    }

    /// ADI: `0afe` is `rts (db)`; the two instructions after it run before the return.
    #[test]
    fn delayed_return_runs_two_slots_then_returns() {
        let mut c = core_at_zero(&[0x06be, 0x0000, 0x0010, 0x0001]);
        load(&mut c, 0x10, &[0x0afe, 0x0001, 0x0001, 0x0001]);
        assert_eq!(c.run(2), None);
        assert_eq!(c.pc, 0x11, "first delay slot after RTS (DB)");
        assert_eq!(c.run(2), None);
        assert_eq!(c.pc, 3, "returns to the instruction after the call");
        assert!(c.return_addresses().is_empty());
        // the non-delayed form `0abe` returns at once
        let mut c = core_at_zero(&[0x06be, 0x0000, 0x0010, 0x0001]);
        load(&mut c, 0x10, &[0x0abe, 0x0001]);
        assert_eq!(c.run(2), None);
        assert_eq!(c.pc, 3);
    }

    /// ADI: `0a3e 0400 0000` is `rts (db)` in the 48-bit form (DB is bit 26 there, not bit 38),
    /// `0a3e 0400 1234` adds the compute `r2=r3+r4`.
    #[test]
    fn wide_delayed_return_runs_two_slots_and_its_compute() {
        let mut c = core_at_zero(&[0x06be, 0x0000, 0x0010, 0x0001]);
        load(
            &mut c,
            0x10,
            &[0x0a3e, 0x0400, 0x1234, 0x0001, 0x0001, 0x0001],
        );
        (c.r[3], c.r[4]) = (5, 6);
        assert_eq!(c.run(2), None);
        assert_eq!(
            (c.r[2], c.pc),
            (11, 0x13),
            "the compute ran; first delay slot next"
        );
        assert_eq!(c.run(2), None);
        assert_eq!(c.pc, 3, "returned after the two delay slots");
        // `0a3e 0000 0000` returns at once
        let mut c = core_at_zero(&[0x06be, 0x0000, 0x0010, 0x0001]);
        load(&mut c, 0x10, &[0x0a3e, 0x0000, 0x0000]);
        assert_eq!(c.run(2), None);
        assert_eq!(c.pc, 3);
    }

    #[test]
    fn conditional_branch_with_a_false_condition_falls_through() {
        // IF EQ JUMP 0x10 (cond 0 = EQ, AZ clear): 0x0600.. direct jump parcels 0600 0000 0010
        let mut c = core_at_zero(&[0x0600, 0x0000, 0x0010]);
        assert!(c.step().is_ok());
        assert_eq!(c.pc, 3);
        c.pc = 0;
        c.flags.az = true;
        assert!(c.step().is_ok());
        assert_eq!(c.pc, 0x10);
    }
}
