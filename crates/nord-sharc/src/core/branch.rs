//! Program flow of [`Core`]: jumps, calls and returns (with delay slots), `DO UNTIL LCE` loops.
use super::decode_cache::FlowOp;
use super::exec::Decoded;
use super::{Core, Fault, LoopFrame, Pending};
use crate::visa::Kind;

/// Instructions that run after a delayed branch before it takes effect.
const DELAY_SLOTS: u8 = 2;

impl Core {
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

    /// Executes a pre-decoded flow instruction (`T8a`, `T9a`/`T9b`, `T11a`/`T11c`, `T12a`).
    pub(super) fn exec_flow_op(
        &mut self,
        d: &Decoded,
        op: FlowOp,
        next: u32,
    ) -> Result<u32, Fault> {
        match op {
            FlowOp::JumpDirect {
                target,
                call,
                delayed,
                ..
            } => {
                if !d.cond_ok {
                    return Ok(next);
                }
                self.apply_jump_modifiers(d);
                Ok(self.take_branch(target, call, delayed, next))
            }
            FlowOp::JumpIndirect {
                reg_i,
                reg_m,
                rel_target,
                call,
                delayed,
                else_form,
                compute,
                ..
            } => {
                if let Some(cop) = compute {
                    if d.cond_ok != else_form {
                        self.on_each_pe(d, d.cond_ok, d.cond_y, |core| core.exec_compute_op(cop))?;
                    }
                }
                if !d.cond_ok {
                    return Ok(next);
                }
                let target = rel_target
                    .unwrap_or_else(|| self.i[reg_i as usize].wrapping_add(self.m[reg_m as usize]));
                self.apply_jump_modifiers(d);
                Ok(self.take_branch(target, call, delayed, next))
            }
            FlowOp::Return {
                rti,
                delayed,
                else_form,
                compute,
                ..
            } => {
                if let Some(cop) = compute {
                    if else_form {
                        self.on_each_pe(d, !d.cond_ok, !d.cond_y, |core| {
                            core.exec_compute_op(cop)
                        })?;
                    } else {
                        self.on_each_pe(d, d.cond_ok, d.cond_y, |core| core.exec_compute_op(cop))?;
                    }
                }
                if !d.cond_ok {
                    return Ok(next);
                }
                let ret = self
                    .call_stack
                    .pop()
                    .ok_or_else(|| self.fault(d.pc, &d.insn))?;
                if rti {
                    self.return_from_interrupt();
                }
                if delayed {
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
            FlowOp::DoUntilCounter {
                end,
                count_ureg,
                imm_count,
            } => {
                let count = match count_ureg {
                    Some(u) => self.read_ureg(u),
                    None => imm_count,
                };
                if count == 0 {
                    return Err(self.fault(d.pc, &d.insn));
                }
                self.push_loop(LoopFrame {
                    start: next,
                    end,
                    remaining: count,
                    ..LoopFrame::default()
                });
                Ok(next)
            }
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
    pub(super) fn finish_step(&mut self, pc: u32, kind: Kind, mut next: u32) -> u32 {
        if self.loops.last().is_some_and(|f| f.term.is_some()) {
            next = self.arith_loop_step(pc, next);
        } else if let Some(frame) = self.loops.last_mut() {
            if pc == frame.end && kind != Kind::T12a {
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
