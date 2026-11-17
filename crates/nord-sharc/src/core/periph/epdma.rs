//! Range-transfer registers of the ORG Core2 section at `DM(0x3E000..0x3E03F)` (the IOP window of
//! the core, below the SRAM): the start-up and the voice-record scan of the real ORG Core2 image
//! program them, and a pass reads the completion count back as a loop count.
//!
//! What the image does (`docs/research/dsp-peripherals-audio.md` section 2, listing
//! `sharc-disasm core2_sram.bin 0x24E2A0 0x24E300`):
//!
//! | Access | Where | Meaning (reading) |
//! |---|---|---|
//! | `DM(0x3E018..0x3E01F) =` `0x20000000 0x20080000 0x200BA984 0x20100000 0x28240000 ...` | boot, `0x125C8F..0x125CB9` | eight region base words (L2 and core alias windows) |
//! | `DM(0x3E002) = 0xBF00`, `DM(0x3E000) = 0x01414141` | boot, `0x125D13`, `0x125D4C` | enable words |
//! | `USTAT1 = DM(0x3E002); BIT SET USTAT1 0x3; DM(0x3E002) = USTAT1` | `0x127145..0x12714B`, once per started range | start mask (bits 1:0, `0xBF00` becomes `0xBF03`) |
//! | `DM(0x3E010) = start; DM(0x3E011) = end` | `0x127150`, `0x127155` | one L2 range of a voice record, e.g. `0x200C0000..0x200C9C44` |
//! | `R0 = DM(0x3E021); DO (PC,3) UNTIL LCE, LCNTR = R0` | `0x12717C..0x12717F` | the completion count is a loop count |
//!
//! The core runs in SIMD mode (`MODE1.PEYEN`), so every data access moves the register pair
//! `(addr, addr + 1)`: `DM(0x3E021)` and `DM(0x3E022)` are read together, `DM(0x3E011)` and
//! `DM(0x3E012)` written together (census: [`crate::core::periph::AccessLog`], `sharc-sys --census`).
//!
//! Model rule: a range is **opened** by a write of `DM(0x3E010)` while both bits of
//! [`START_MASK`] are set in `DM(0x3E002)` (the mask is set first, at `0x12714B`, and the
//! firmware never clears it). The transfer completes at once (the oracle's L2 is plain memory,
//! nothing has to move): the next read of `DM(0x3E021)` returns the number of ranges opened since
//! its last read (then clears it) and records `(DM(0x3E010), DM(0x3E011))` of each. A read with
//! nothing opened returns 0. The SIMD pair store writes `0x3E011` twice (first the PEy word),
//! so the end word is taken at completion, not at the first write. ASSUMED: the register name,
//! the unit of the count and "one range, count 1" are not documented; the count is what the old
//! `--pre 0x3E021=1` stub supplied, the rule only ties it to the start. Confirm with a hardware
//! read of `0x3E021` or a CCES simulator run.
use std::collections::BTreeMap;

pub const BASE: u32 = 0x3_E000;
pub const WORDS: u32 = 0x40;
/// Control word whose [`START_MASK`] bits arm a range.
pub const CTL: u32 = 0x3_E002;
pub const RANGE_START: u32 = 0x3_E010;
pub const RANGE_END: u32 = 0x3_E011;
/// Completion count read back by the voice-record scan.
pub const COUNT: u32 = 0x3_E021;
/// Bits of [`CTL`] the start routine sets (`BIT SET USTAT1 0x3`).
pub const START_MASK: u32 = 0x3;

/// The register block with its start rule.
#[derive(Debug, Default)]
pub struct EpDma {
    regs: BTreeMap<u32, u32>,
    open: bool,
    pending: u32,
    /// `(start, end)` of every started range, in order.
    pub started: Vec<(u32, u32)>,
}

impl EpDma {
    /// Whether `addr` is one of the block's registers.
    pub fn contains(addr: u32) -> bool {
        addr.wrapping_sub(BASE) < WORDS
    }

    /// Register read: [`COUNT`] completes the open range and returns and clears the completion
    /// count, the others read back the last written value (0 before the first write).
    pub fn read(&mut self, addr: u32) -> u32 {
        if addr == COUNT {
            if std::mem::take(&mut self.open) {
                self.pending += 1;
                let get = |a| self.regs.get(&a).copied().unwrap_or(0);
                self.started.push((get(RANGE_START), get(RANGE_END)));
            }
            return std::mem::take(&mut self.pending);
        }
        self.regs.get(&addr).copied().unwrap_or(0)
    }

    /// Register write; a [`RANGE_START`] write while [`CTL`] has [`START_MASK`] set opens a range.
    pub fn write(&mut self, addr: u32, v: u32) {
        self.regs.insert(addr, v);
        let ctl = self.regs.get(&CTL).copied().unwrap_or(0);
        if addr == RANGE_START && ctl & START_MASK == START_MASK {
            self.open = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_opened_range_completes_at_the_count_read_and_the_count_clears() {
        let mut e = EpDma::default();
        assert_eq!(e.read(COUNT), 0, "nothing opened");
        e.write(RANGE_START, 1);
        e.write(RANGE_END, 2);
        assert_eq!(e.read(COUNT), 0, "start mask not set: no range opens");
        e.write(CTL, 0xBF00 | START_MASK);
        e.write(RANGE_START, 0x200C_0000);
        e.write(RANGE_END, 0x1234); // the SIMD pair store writes the PEy word first
        e.write(RANGE_END, 0x200C_9C44);
        assert_eq!(e.read(RANGE_END), 0x200C_9C44, "plain registers read back");
        assert_eq!((e.read(COUNT), e.read(COUNT)), (1, 0));
        assert_eq!(e.started, vec![(0x200C_0000, 0x200C_9C44)]);
        // a second range opened and read gives a count of 1 again
        e.write(RANGE_START, 0x200D_0000);
        assert_eq!(e.read(COUNT), 1);
        assert_eq!(e.started.len(), 2);
    }

    #[test]
    fn the_block_covers_0x3e000_to_0x3e03f() {
        assert!(EpDma::contains(0x3_E000) && EpDma::contains(0x3_E03F));
        assert!(!EpDma::contains(0x3_DFFF) && !EpDma::contains(0x3_E040));
    }
}
