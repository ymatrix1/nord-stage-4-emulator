//! System Event Controller (SEC0) of the ADSP-2157x at `0x3108_9000`, core-interface part only.
//!
//! Register map VERIFIED against `ADI_SEC_TypeDef` in CCES 2.12.1 `SHARC/include/sys/ADSP_21571_device.h`:
//! `GCTL +0x00, GSTAT +0x04, RAISE +0x08, END +0x0C`, `CCTL1 +0x440 .. CSID1 +0x45C`,
//! `CCTL2 +0x480 .. CSID2 +0x49C`, `SB[n].SCTL +0x800 + 8n`. The ORG Core1 ISR reads
//! `DM(0x3108945C)` (CSID1) and writes the id to `DM(0x3108900C)` (END) (`docs/research/dsp-rx-protocol.md`
//! section 4; FX Core1 reads `0x31089440` = CCTL1).
//!
//! ASSUMED (to be measured on the CCES simulator, `docs/research/sharc-chip-identity.md` section 6):
//! a read of CSIDn returns the lowest pending source id and makes it active, 0 when nothing is
//! pending; a write of an id to END ends it; the core interrupt is asserted while something is
//! pending and nothing is active; source enables (SCTL) are not checked.
use std::collections::{BTreeSet, HashMap};

/// Base address of SEC0.
pub const BASE: u32 = 0x3108_9000;
/// Size of the register window.
pub const SIZE: u32 = 0x1000;
/// Offset of `SEC_END`.
pub const END: u32 = 0x0C;
/// Offset of `SEC_CSID1`; core 2 is `CSID1 + 0x40`.
pub const CSID1: u32 = 0x45C;
/// Distance between the core interfaces of core 1 and core 2.
pub const CORE_STRIDE: u32 = 0x40;

/// SEC state for the two SHARC cores (index 0 = Core1, 1 = Core2).
#[derive(Default)]
pub struct Sec {
    pending: [BTreeSet<u32>; 2],
    active: [Option<u32>; 2],
    asserted: [bool; 2],
    regs: HashMap<u32, u32>,
}

impl Sec {
    /// Whether `addr` is a SEC register.
    pub fn contains(addr: u32) -> bool {
        addr.wrapping_sub(BASE) < SIZE
    }

    /// Raises source `id` towards `core`.
    pub fn raise(&mut self, core: usize, id: u32) {
        if self.active[core] != Some(id) {
            self.pending[core].insert(id);
        }
    }

    /// Whether the core interrupt line must be (newly) asserted for `core`.
    pub fn poll_irq(&mut self, core: usize) -> bool {
        let want = !self.pending[core].is_empty() && self.active[core].is_none();
        let edge = want && !self.asserted[core];
        self.asserted[core] = want;
        edge
    }

    /// Register read.
    pub fn read(&mut self, addr: u32) -> u32 {
        let off = addr - BASE;
        for core in 0..2 {
            if off == CSID1 + CORE_STRIDE * core as u32 {
                if self.active[core].is_none() {
                    self.active[core] = self.pending[core].pop_first();
                }
                self.asserted[core] = false;
                return self.active[core].unwrap_or(0);
            }
        }
        self.regs.get(&off).copied().unwrap_or(0)
    }

    /// Register write.
    pub fn write(&mut self, addr: u32, value: u32) {
        let off = addr - BASE;
        if off == END {
            for core in 0..2 {
                if self.active[core] == Some(value) {
                    self.active[core] = None;
                }
            }
        } else if off == 0x08 {
            // RAISE: software-raise a source for Core1 (ASSUMED routing)
            self.raise(0, value);
        } else {
            self.regs.insert(off, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csid_activates_lowest_pending_and_end_releases() {
        let mut s = Sec::default();
        assert!(!s.poll_irq(0));
        s.raise(0, 90);
        s.raise(0, 69);
        assert!(s.poll_irq(0), "edge on first pending source");
        assert!(!s.poll_irq(0), "no second edge");
        assert_eq!(s.read(0x3108_945C), 69);
        assert_eq!(s.read(0x3108_945C), 69, "stays active until END");
        assert!(!s.poll_irq(0), "active blocks the line");
        s.write(0x3108_900C, 69);
        assert!(s.poll_irq(0), "92 pending again after END");
        assert_eq!(s.read(0x3108_945C), 90);
        s.write(0x3108_900C, 90);
        assert_eq!(s.read(0x3108_945C), 0, "nothing pending reads 0");
    }

    #[test]
    fn cores_are_separate_and_other_registers_store() {
        let mut s = Sec::default();
        s.raise(1, 79);
        assert_eq!(s.read(0x3108_945C), 0);
        assert_eq!(s.read(0x3108_949C), 79);
        s.write(0x3108_9AF0, 0x5);
        assert_eq!(s.read(0x3108_9AF0), 5);
        assert!(Sec::contains(0x3108_9440) && !Sec::contains(0x3108_A000));
    }
}
