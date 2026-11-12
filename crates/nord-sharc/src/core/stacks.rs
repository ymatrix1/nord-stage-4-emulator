//! PC, loop and status stacks of [`Core`] with their depth limits and the STKY bits that
//! describe them. Depths (PRM 4-12, 4-37, 6-86): PC stack 30, status stack 15, loop stack 6.
use super::{Core, LoopFrame};

pub(super) const PC_STACK_DEPTH: usize = 30;
pub(super) const STATUS_STACK_DEPTH: usize = 15;
const LOOP_STACK_DEPTH: usize = 6;
/// Value of PCSTK while the PC stack is empty. VERIFIED on the simulator (`stack_measured.rs`):
/// 24 bits of ones (the PRM text suggests 26).
pub(super) const PCSTK_EMPTY: u32 = 0xFF_FFFF;

/// STKY read-only sticky stack bits: status stack overflow (23) and loop stack overflow (25).
pub(super) const SSOV: u32 = 1 << 23;
pub(super) const LSOV: u32 = 1 << 25;
/// STKY stack state bits: PC stack full (21) and empty (22), status stack empty (24), loop stack
/// empty (26). Empty bits are set at reset (PRM Figure A-4).
pub(super) const PCFL: u32 = 1 << 21;
pub(super) const PCEM: u32 = 1 << 22;
pub(super) const SSEM: u32 = 1 << 24;
pub(super) const LSEM: u32 = 1 << 26;

impl Core {
    /// Entries of the shared PC stack: return addresses plus the top-of-loop address of every
    /// loop that still has one on it.
    pub(super) fn pc_depth(&self) -> usize {
        self.call_stack.len() + self.loops.iter().filter(|f| !f.detached).count()
    }

    /// Pushes a return address on the PC stack. At 29 of 30 entries `SOVFI` is latched; a push
    /// onto a full stack is lost and PCSTKP reads 31 (ASSUMED: the PRM gives no data on what an
    /// overflowing push keeps).
    pub(super) fn push_return(&mut self, addr: u32) {
        if self.pc_depth() >= PC_STACK_DEPTH {
            self.sys.pc_overflow = true;
            self.raise_interrupt(3);
            return;
        }
        self.call_stack.push(addr);
        if self.pc_depth() >= PC_STACK_DEPTH - 1 {
            self.raise_interrupt(3);
        }
    }

    /// Pushes a loop frame (more than six sets the loop stack overflow bit and `SOVFI`; the frame
    /// is kept so execution stays well defined).
    pub(super) fn push_loop(&mut self, mut frame: LoopFrame) {
        frame.cs_depth = self.call_stack.len();
        self.loops.push(frame);
        if self.loops.len() > LOOP_STACK_DEPTH {
            self.sys.stky[0] |= LSOV;
            self.raise_interrupt(3);
        }
    }

    /// Pushes ASTATx, ASTATy and MODE1 on the status stack and clears the MMASK bits of MODE1.
    pub(super) fn push_status(&mut self) {
        if self.sys.status.len() >= STATUS_STACK_DEPTH {
            self.sys.stky[0] |= SSOV;
            self.raise_interrupt(3);
            return;
        }
        let entry = [self.astat(0), self.astat(1), self.mode1];
        self.sys.status.push(entry);
        self.set_mode1(self.mode1 & !self.mmask);
    }

    /// Pops the status stack into ASTATx, ASTATy and MODE1 (empty stack: no effect).
    pub(super) fn pop_status(&mut self) {
        if let Some([x, y, mode1]) = self.sys.status.pop() {
            self.set_astat(0, x);
            self.set_astat(1, y);
            self.set_mode1(mode1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_state_bits_follow_the_stacks() {
        let mut c = Core::new(&[]);
        c.push_return(0x1234);
        let s = c.read_ureg(0x78);
        assert_eq!(s & PCEM, 0, "PC stack not empty");
        assert_eq!((c.read_ureg(0x64), c.read_ureg(0x65)), (0x1234, 1));
        c.write_ureg(0x64, 0x55);
        assert_eq!(c.return_addresses(), &[0x55]);
        c.push_return(1);
        c.push_return(2);
        c.write_ureg(0x65, 1); // keep the oldest entry only
        assert_eq!(c.return_addresses(), &[0x55]);
        c.push_status();
        assert_eq!(c.read_ureg(0x78) & SSEM, 0);
        c.pop_status();
        assert_ne!(c.read_ureg(0x78) & SSEM, 0);
    }

    #[test]
    fn pc_stack_full_and_overflow_are_flagged_and_latch_sovfi() {
        let mut c = Core::new(&[]);
        for k in 0..28 {
            c.push_return(k);
        }
        assert_eq!(c.read_ureg(0x7A) & 8, 0, "28 of 30: no stack interrupt yet");
        c.push_return(28);
        assert_eq!(c.read_ureg(0x7A) & 8, 8, "29 of 30 latches SOVFI");
        c.push_return(29);
        assert_ne!(c.read_ureg(0x78) & PCFL, 0, "full");
        c.push_return(30);
        assert_eq!(c.read_ureg(0x65), 31, "overflowed");
        assert_eq!(c.return_addresses().len(), 30);
    }

    #[test]
    fn status_stack_is_fifteen_deep_and_overflow_is_sticky() {
        let mut c = Core::new(&[]);
        for _ in 0..15 {
            c.push_status();
        }
        assert_eq!(c.read_ureg(0x78) & SSOV, 0);
        c.push_status();
        assert_ne!(c.read_ureg(0x78) & SSOV, 0);
        c.write_ureg(0x78, 0);
        assert_ne!(c.read_ureg(0x78) & SSOV, 0, "SSOV is read-only");
    }

    #[test]
    fn loop_stack_overflow_beyond_six_nested_loops() {
        let mut c = Core::new(&[]);
        let frame = LoopFrame {
            start: 0,
            end: 3,
            remaining: 2,
            ..LoopFrame::default()
        };
        for _ in 0..6 {
            c.push_loop(frame);
        }
        assert_eq!(c.read_ureg(0x78) & (LSOV | LSEM), 0);
        c.push_loop(frame);
        assert_ne!(c.read_ureg(0x78) & LSOV, 0);
        assert_eq!(c.read_ureg(0x67), 2, "CURLCNTR");
    }

    #[test]
    fn laddr_encodes_end_address_and_loop_length() {
        let mut c = Core::new(&[]);
        assert_eq!(
            c.read_ureg(0x66),
            u32::MAX,
            "all ones while the loop stack is empty"
        );
        // three 16-bit NOPs at 0x10..0x12: a counter loop of length 3
        c.push_loop(LoopFrame {
            start: 0x10,
            end: 0x12,
            remaining: 5,
            ..LoopFrame::default()
        });
        for k in 0..3 {
            c.sram[(0x10 + k) * 2..][..2].copy_from_slice(&0x0001u16.to_le_bytes());
        }
        assert_eq!(c.read_ureg(0x66), 0x12 | 0x0F << 24 | 0b110 << 29);
    }
}
