//! Arithmetic loops: Type 13a `DO ... UNTIL cond` (a condition other than `LCE`).
//!
//! Semantics (PRM 4-53 to 4-59, "Arithmetic Loops" and "Short Arithmetic Based Loops"): the
//! loop runs until its termination condition tests true; with a body of `N` instructions the
//! test is made when the second instruction of the body is fetched, which is four instructions
//! (pipeline depth) before the instruction that executes at that moment. At the instruction
//! level this is: the condition is evaluated on the flags that are current after the
//! instruction at body index `(N - 4) mod N` has executed; if it holds, the instruction that
//! follows and the next two still run and the loop is left after them (always after the last
//! body instruction, because the stream is periodic with period `N`). The PRM example
//! (`R5` counts 6 passes) and its short-loop rules agree with this: a three instruction loop
//! finishes one more pass, a two instruction loop one more pass (condition true by the first
//! instruction) or two (true by the second), a one instruction loop runs three more iterations.
//! `FOREVER` (code 31) never terminates.
//!
//! Provenance: Type 13a layout VERIFIED against ADI (`0e00 0000 0007` is `do (pc,0x7) until
//! eq`, TERM bits 37..33 as the `IF` condition field, `0e3e` is `until forever`). The evaluation
//! point and the extra passes are VERIFIED on the CCES ADSP-21489 simulator by
//! `loops_measured.rs` (1 072 cases: conditions `eq ne lt ge gt le` and `LCE`, bodies of 1 to 6
//! instructions, every flag-setting position) and `entry_measured.txt` (conditions true at loop
//! entry), see `docs/research/sharc-loops.md`. Not covered: SIMD loops, branches inside the
//! body, conditions other than the ALU flags.
use super::exec::Decoded;
use super::{sign_extend, Core, Fault, LoopFrame};
use crate::visa::bits;

/// Termination code `FOREVER` (Table 10-4 `TRUE/FOREVER`).
const FOREVER: u8 = 31;
/// Instructions that still run after the condition was found true (see the module comment).
const RUN_ON: u8 = 3;
/// Longest loop body the walk for the check address follows.
const WALK_LIMIT: usize = 4096;

impl Core {
    /// Type 13a: pushes an arithmetic loop whose end address is `pc + <reladdr23>`.
    pub(super) fn exec_do_until(&mut self, d: &Decoded, next: u32) -> Result<(), Fault> {
        let end = d.pc.wrapping_add(sign_extend(bits(d.w, 22, 0) as u32, 23));
        let remaining = self.loops.last().map_or(0, |f| f.remaining);
        self.push_loop(LoopFrame {
            start: next,
            end,
            remaining,
            term: Some(bits(d.w, 37, 33) as u8),
            ..LoopFrame::default()
        });
        self.refresh_check();
        // VERIFIED (simulator, `loops_measured.txt`, `entry_measured.txt`): the termination
        // condition is sampled after the instructions at body index `(N - 4) mod N` of every
        // pass, and once more after the `DO` itself when that is the sample of the previous
        // pass, i.e. for bodies of 1 and 3 instructions: a condition that already holds when
        // the loop is entered then ends it after three body instructions (three passes of a
        // one instruction body, one pass of a three instruction body).
        if let Some(top) = self.loops.last().copied() {
            let code = top.term.unwrap_or(FOREVER);
            if code != FOREVER
                && 3 % self.body_len(top.start, top.end) == 0
                && self.term_holds(code)
            {
                let skip = self.body_len(top.start, top.end) == 1;
                if let Some(t) = self.loops.last_mut() {
                    t.exit_in = RUN_ON;
                    t.skip_after = skip;
                }
            }
        }
        Ok(())
    }

    /// Number of instructions of the body `start..=end`.
    fn body_len(&self, start: u32, end: u32) -> usize {
        let (mut pc, mut n) = (start, 0usize);
        while pc <= end && n < WALK_LIMIT {
            pc = pc.wrapping_add(self.fetch(pc).insn.len.max(1) as u32);
            n += 1;
        }
        n.max(1)
    }

    /// Recomputes the address whose execution tests the termination condition of the innermost
    /// arithmetic loop (after the loop was pushed, or `PCSTK`/`LADDR` changed it).
    pub(super) fn refresh_check(&mut self) {
        let Some(f) = self.loops.last().copied().filter(|f| f.term.is_some()) else {
            return;
        };
        let mut body = Vec::new();
        let mut pc = f.start;
        while pc <= f.end && body.len() < WALK_LIMIT {
            body.push(pc);
            pc = pc.wrapping_add(self.fetch(pc).insn.len.max(1) as u32);
        }
        let n = body.len();
        let check = if n == 0 {
            u32::MAX
        } else {
            body[(n as isize - 4).rem_euclid(n as isize) as usize] // index (N - 4) mod N
        };
        if let Some(top) = self.loops.last_mut() {
            top.check = check;
        }
    }

    /// Whether the termination condition `code` holds on PEx (and PEy in SIMD mode).
    fn term_holds(&self, code: u8) -> bool {
        let c = u64::from(code);
        self.cond(c) && (!self.simd() || self.cond_for(true, c))
    }

    /// End-of-instruction bookkeeping of the innermost arithmetic loop for the instruction at
    /// `pc`, which was followed by `next`. Returns the pc to continue at.
    pub(super) fn arith_loop_step(&mut self, pc: u32, mut next: u32) -> u32 {
        let Some(f) = self.loops.last().copied() else {
            return next;
        };
        let in_body = pc >= f.start && pc <= f.end;
        if in_body && f.exit_in > 0 {
            if f.exit_in == 1 {
                self.loops.pop(); // the last body instruction ran: fall out of the loop
                if f.skip_after {
                    // VERIFIED (simulator): the instruction after such a loop does not execute
                    return next.wrapping_add(self.fetch(next).insn.len.max(1) as u32);
                }
                return next;
            }
            if let Some(top) = self.loops.last_mut() {
                top.exit_in -= 1;
            }
        } else if pc == f.check && f.term.is_some_and(|t| t != FOREVER) {
            let code = f.term.unwrap_or(FOREVER);
            if self.term_holds(code) {
                if let Some(top) = self.loops.last_mut() {
                    top.exit_in = RUN_ON;
                }
            }
        }
        if pc == f.end {
            next = f.start;
        }
        next
    }
}

#[cfg(test)]
mod tests {
    use super::super::support::core_at_zero;
    use super::LoopFrame;

    /// ADI: `013e 0002 a000` is `r0=r0-1`, `0000 0000 0000` is `nop`.
    const DEC_R0: [u16; 3] = [0x013E, 0x0002, 0xA000];
    const NOP: [u16; 3] = [0, 0, 0];

    fn program(parts: &[&[u16]]) -> Vec<u16> {
        parts.iter().flat_map(|p| p.iter().copied()).collect()
    }

    #[test]
    fn check_address_is_index_n_minus_4_of_the_body() {
        // eight 16-bit NOPs: bodies of 1..5 instructions starting at 0
        let mut c = core_at_zero(&[0x0001; 8]);
        c.push_loop(LoopFrame {
            start: 0,
            end: 5,
            term: Some(0),
            ..LoopFrame::default()
        });
        c.refresh_check();
        assert_eq!(c.loops[0].check, 2, "six instructions: after index 2");
        for (end, check) in [(0u32, 0u32), (1, 0), (2, 2), (3, 0), (4, 1)] {
            c.loops[0].end = end;
            c.refresh_check();
            assert_eq!(c.loops[0].check, check, "body of {} instructions", end + 1);
        }
    }

    /// The example of PRM 4-53: `DO label UNTIL EQ` over six instructions; after termination
    /// `R5 = 0; R6 = 4; R7 = 8`. Instructions as assembled by `easm21k` and printed by ADI:
    /// `r7=0xe`, `r6=0xa`, `r5=0x6`, `do (pc,..) until eq`, `r6=r6-1` (`c666`), `r7=r7-1`
    /// (`c677`), `r5=r5-1` (`013e 0002 a550`), three NOPs; the DO at 6, the last NOP at 0x14.
    #[test]
    fn prm_arithmetic_loop_example() {
        let code = program(&[
            &[0x0F87, 0x000E, 0x0F86, 0x000A, 0x0F85, 0x0006],
            &[0x0E00, 0x0000, 0x000E],
            &[0xC666, 0xC677, 0x013E, 0x0002, 0xA550],
            &NOP,
            &NOP,
            &NOP,
            &[0xC500],
        ]);
        let mut c = core_at_zero(&code);
        assert_eq!(c.run(3 + 1 + 6 * 6), None);
        assert_eq!((c.r[5], c.r[6], c.r[7]), (0, 4, 8));
        assert_eq!(c.pc, 0x17, "the loop was left after the sixth pass");
        assert!(c.loops.is_empty());
    }

    /// PRM 4-58: after the cycle in which the condition becomes true, a one instruction loop
    /// runs three more iterations; a two or three instruction loop one more full pass.
    #[test]
    fn short_arithmetic_loops_run_the_documented_extra_passes() {
        // N = 1: `do (pc,3) until eq` then `r0=r0-1`
        let one = program(&[&[0x0E00, 0x0000, 0x0003], &DEC_R0, &[0xC500]]);
        let mut c = core_at_zero(&one);
        c.r[0] = 5;
        assert_eq!(c.run(1 + 8), None);
        assert_eq!((c.r[0] as i32, c.pc), (-3, 6), "5 iterations + 3 more");
        // N = 2: the decrement first, then a NOP
        let two = program(&[&[0x0E00, 0x0000, 0x0006], &DEC_R0, &NOP, &[0xC500]]);
        let mut c = core_at_zero(&two);
        c.r[0] = 5;
        assert_eq!(c.run(1 + 6 * 2), None);
        assert_eq!(
            (c.r[0] as i32, c.pc),
            (-1, 9),
            "condition true in pass 5, pass 6 completes"
        );
        // N = 3
        let three = program(&[&[0x0E00, 0x0000, 0x0009], &DEC_R0, &NOP, &NOP, &[0xC500]]);
        let mut c = core_at_zero(&three);
        c.r[0] = 5;
        assert_eq!(c.run(1 + 6 * 3), None);
        assert_eq!((c.r[0] as i32, c.pc), (-1, 12));
    }

    #[test]
    fn forever_never_terminates_and_other_codes_use_the_flags() {
        let code = program(&[&[0x0E3E, 0x0000, 0x0003], &NOP, &[0xC500]]);
        let mut c = core_at_zero(&code);
        assert_eq!(c.run(200), None);
        assert_eq!(c.loops.len(), 1, "DO FOREVER stays on the loop stack");
        assert!(c.loops[0].term == Some(31));
    }

    #[test]
    fn do_until_flag_input_follows_the_pin() {
        // DO ... UNTIL FLAG2_IN (code 11): a single NOP body; raising the pin ends the loop
        let code = program(&[&[0x0E16, 0x0000, 0x0003], &NOP, &[0xC500]]);
        let mut c = core_at_zero(&code);
        assert_eq!(c.run(20), None);
        assert_eq!(c.loops.len(), 1);
        c.set_flag_input(2, true);
        assert_eq!(c.run(10), None);
        assert!(
            c.loops.is_empty(),
            "left three iterations after the pin rose"
        );
    }
}
