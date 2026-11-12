//! The shared PC stack as the program sees it (`PCSTK`, `LADDR`, `CURLCNTR`) and the Type 20a
//! stack instructions `PUSH/POP LOOP`, `PUSH/POP STS` and `PUSH/POP PCSTK`.
//!
//! The core keeps return addresses (`call_stack`) and loop frames (`loops`) apart, while the
//! hardware keeps return addresses and the top-of-loop address of every `DO` loop on one PC
//! stack. A frame's `cs_depth` (return addresses below its entry) restores that order: the loop
//! owns the top of the PC stack while no call was made since it was pushed. `detached` marks a
//! frame whose entry was popped (`POP PCSTK`) or not pushed yet (`PUSH LOOP`), which is how the
//! save and restore sequence of the PRM (Listing 4-6, `POP LOOP; POP PCSTK` ... `PUSH LOOP;
//! CURLCNTR = ...; LADDR = ...; PUSH PCSTK; PCSTK = ...`) works.
//!
//! Provenance: Type 20a bit positions VERIFIED against ADI `elfdump` (`push loop` `1780`, `pop
//! loop` `1740`, `push sts` `1720`, `pop sts` `1710`, `push pcstk` `1708`, `pop pcstk` `1704`,
//! `flush cache` `1702`); the order of loop and PC stack entries is ASSUMED from PRM 4-75/4-76.
use super::exec::Decoded;
use super::stacks::PCSTK_EMPTY;
use super::{Core, LoopFrame};
use crate::visa::{bits, classify};

/// LADDR termination code of a counter loop (ASSUMED: the `LCE` code of Table 10-4).
const LADDR_LCE: u32 = 0x0F;

impl Core {
    /// Whether the top of the PC stack is the top-of-loop address of the innermost loop.
    fn loop_on_top(&self) -> bool {
        self.loops
            .last()
            .is_some_and(|f| !f.detached && f.cs_depth >= self.call_stack.len())
    }

    /// PCSTK: the top entry of the PC stack, `0x3FFFFFF` while it is empty.
    pub(super) fn pcstk(&self) -> u32 {
        match self.loops.last() {
            Some(f) if self.loop_on_top() => f.start,
            _ => self.call_stack.last().copied().unwrap_or(PCSTK_EMPTY),
        }
    }

    /// Writes the top entry of the PC stack (a return address or a top-of-loop address).
    pub(super) fn set_pcstk(&mut self, v: u32) {
        let v = v & 0xFF_FFFF;
        if self.loop_on_top() {
            if let Some(f) = self.loops.last_mut() {
                f.start = v;
            }
            self.refresh_check();
        } else if let Some(top) = self.call_stack.last_mut() {
            *top = v;
        }
    }

    /// LADDR: loop end address, termination code and loop type by length (`LCE` = `0x0F`).
    pub(super) fn laddr(&self) -> u32 {
        let Some(f) = self.loops.last() else {
            return u32::MAX; // VERIFIED: all ones with an empty loop stack
        };
        if f.end == 0 {
            return 0; // VERIFIED: a fresh `PUSH LOOP` entry reads 0
        }
        let mut pc = f.start;
        let mut n = 0;
        while pc <= f.end && n < 4 {
            pc += classify(self.parcel(pc), Some(self.parcel(pc + 1))).1 as u32;
            n += 1;
        }
        let ty = [0b111, 0b010, 0b100, 0b110, 0b111][n.min(4)];
        let term = f.term.map_or(LADDR_LCE, u32::from);
        f.end & 0xFF_FFFF | term << 24 | ty << 29
    }

    /// Writes LADDR of the innermost loop: end address (bits 23..0) and termination code (28..24).
    pub(super) fn set_laddr(&mut self, v: u32) {
        let term = (v >> 24 & 0x1F) as u8;
        if let Some(f) = self.loops.last_mut() {
            f.end = v & 0xFF_FFFF;
            f.term = (u32::from(term) != LADDR_LCE).then_some(term);
        }
        self.refresh_check();
    }

    /// Writes CURLCNTR (and LCNTR) of the innermost loop.
    pub(super) fn set_curlcntr(&mut self, v: u32) {
        if let Some(f) = self.loops.last_mut() {
            f.remaining = v;
        }
    }

    /// `PUSH LOOP`: an empty loop frame whose registers the program then loads.
    fn push_loop_stack(&mut self) {
        // VERIFIED: a fresh entry has CURLCNTR = 1
        self.push_loop(LoopFrame {
            detached: true,
            remaining: 1,
            ..LoopFrame::default()
        });
    }

    /// `POP LOOP`: drops the innermost frame. Its top-of-loop entry stays on the PC stack as a
    /// plain entry, which `POP PCSTK` removes (Listing 4-6).
    fn pop_loop_stack(&mut self) {
        if let Some(f) = self.loops.pop() {
            if !f.detached {
                self.call_stack.push(f.start);
            }
        }
    }

    /// `PUSH PCSTK`: a new PC stack entry. Right after `PUSH LOOP` it is the entry of that loop.
    fn push_pc_stack(&mut self, next: u32) {
        let depth = self.call_stack.len();
        if let Some(f) = self.loops.last_mut().filter(|f| f.detached) {
            *f = LoopFrame {
                detached: false,
                cs_depth: depth,
                start: next, // VERIFIED: PUSH PCSTK stores the address of the next instruction
                ..*f
            };
        } else {
            self.push_return(next); // VERIFIED: the address of the next instruction
        }
    }

    /// `POP PCSTK`: drops the top entry of the PC stack.
    fn pop_pc_stack(&mut self) {
        if self.loop_on_top() {
            if let Some(f) = self.loops.last_mut() {
                f.detached = true;
            }
        } else {
            self.call_stack.pop();
        }
    }

    /// Type 20a: pushes or pops the loop, status and PC stacks (and flushes the cache, which is
    /// not modelled). Bits 39/38 loop, 37/36 status, 35/34 PC stack, 33 flush; a push wins when
    /// both bits of a pair are set (ADI prints `push loop` for `17c0`).
    pub(super) fn exec_stack_op(&mut self, d: &Decoded) {
        let w = d.w;
        let pair = |push, pop| (bits(w, push, push) == 1, bits(w, pop, pop) == 1);
        match pair(39, 38) {
            (true, _) => self.push_loop_stack(),
            (_, true) => self.pop_loop_stack(),
            _ => {}
        }
        match pair(37, 36) {
            (true, _) => self.push_status(),
            (_, true) => self.pop_status(),
            _ => {}
        }
        match pair(35, 34) {
            (true, _) => self.push_pc_stack(d.pc.wrapping_add(d.insn.len as u32)),
            (_, true) => self.pop_pc_stack(),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::support::{core_at_zero, decoded};
    use super::*;

    /// ADI: `1780 0000 0000` is `push loop`, `1708` `push pcstk`, `1740` `pop loop`, `1704`
    /// `pop pcstk`.
    #[test]
    fn listing_4_6_saves_and_restores_two_nested_loops() {
        let mut c = core_at_zero(&[0x0001; 4]);
        // outer loop 0x100..0x140 (3 left), inner loop 0x120..0x130 (7 left)
        c.push_loop(LoopFrame {
            start: 0x100,
            end: 0x140,
            remaining: 3,
            ..LoopFrame::default()
        });
        c.push_loop(LoopFrame {
            start: 0x120,
            end: 0x130,
            remaining: 7,
            ..LoopFrame::default()
        });
        let (laddr_in, cnt_in, pc_in) = (c.laddr(), 7, c.pcstk());
        assert_eq!(pc_in, 0x120, "the inner loop owns the top of the PC stack");
        // POP LOOP; POP PCSTK twice
        let pops = [[0x1740u16, 0, 0], [0x1704, 0, 0]];
        let mut saved = Vec::new();
        for _ in 0..2 {
            saved.push((c.laddr(), c.loops.last().unwrap().remaining, c.pcstk()));
            for chunk in &pops {
                let d = decoded(&chunk[..]);
                c.exec_stack_op(&d);
            }
        }
        assert!(c.loops.is_empty() && c.call_stack.is_empty());
        assert_eq!(c.pc_depth(), 0);
        assert_eq!(saved[0], (laddr_in, cnt_in, 0x120));
        // restore: PUSH LOOP; CURLCNTR = ..; LADDR = ..; PUSH PCSTK; PCSTK = ..
        for &(laddr, count, pcstk) in saved.iter().rev() {
            let d = decoded(&[0x1780, 0, 0]);
            c.exec_stack_op(&d);
            c.write_ureg(0x67, count);
            c.write_ureg(0x66, laddr);
            let d = decoded(&[0x1708, 0, 0]);
            c.exec_stack_op(&d);
            c.write_ureg(0x64, pcstk);
        }
        let frames: Vec<_> = c
            .loops
            .iter()
            .map(|f| (f.start, f.end, f.remaining))
            .collect();
        assert_eq!(frames, [(0x100, 0x140, 3), (0x120, 0x130, 7)]);
        assert!(
            c.call_stack.is_empty(),
            "PUSH PCSTK after PUSH LOOP is the loop's entry"
        );
        assert_eq!(c.pc_depth(), 2);
    }

    #[test]
    fn pcstk_is_the_loop_start_until_a_call_is_made() {
        let mut c = core_at_zero(&[0x0001]);
        c.push_loop(LoopFrame {
            start: 0x40,
            end: 0x44,
            remaining: 2,
            ..LoopFrame::default()
        });
        assert_eq!(c.read_ureg(0x64), 0x40);
        c.push_return(0x99);
        assert_eq!(c.read_ureg(0x64), 0x99);
        c.write_ureg(0x64, 0x77);
        assert_eq!(c.return_addresses(), &[0x77]);
        c.call_stack.pop();
        // the boot kernel loads PCSTK right after its DO to move the top of the loop
        c.write_ureg(0x64, 0x8C003);
        assert_eq!(c.loops[0].start, 0x8C003);
    }

    #[test]
    fn push_and_pop_sts_use_the_status_stack() {
        let mut c = core_at_zero(&[0x0001]);
        c.mode1 = 0x1000 | 1 << 21;
        c.flags.az = true;
        let d = decoded(&[0x1720, 0, 0]);
        c.exec_stack_op(&d);
        assert_eq!(
            c.mode1, 0x1000,
            "PUSH STS applies MMASK (PEYEN) to MODE1 (PRM 5754)"
        );
        c.flags.az = false;
        c.mode1 = 0;
        let d = decoded(&[0x1710, 0, 0]);
        c.exec_stack_op(&d);
        assert_eq!(c.mode1, 0x1000 | 1 << 21);
        assert!(c.flags.az, "ASTATx restored");
    }

    #[test]
    fn push_pcstk_pushes_a_plain_entry_and_pop_removes_it() {
        let mut c = core_at_zero(&[0x0001]);
        let d = decoded(&[0x1708, 0, 0]);
        c.exec_stack_op(&d);
        assert_eq!(c.pc_depth(), 1);
        c.write_ureg(0x64, 0x1234);
        assert_eq!(c.return_addresses(), &[0x1234]);
        let d = decoded(&[0x1704, 0, 0]);
        c.exec_stack_op(&d);
        assert_eq!(c.pc_depth(), 0);
    }

    #[test]
    fn laddr_reports_the_termination_code_of_an_arithmetic_loop() {
        let mut c = core_at_zero(&[0x0001]);
        c.push_loop(LoopFrame {
            start: 0x10,
            end: 0x12,
            remaining: 0,
            term: Some(0),
            ..LoopFrame::default()
        });
        assert_eq!(c.laddr() >> 24 & 0x1F, 0, "EQ");
        c.write_ureg(0x66, 0x0F00_0012);
        assert_eq!(c.loops[0].term, None, "code 0x0F is a counter loop");
    }
}
