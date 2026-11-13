//! ISA mode: fixed 48-bit instruction words addressed in words, as in the boot kernel of the
//! SY1SY2 and SMP images (`docs/research/sharc-sy1sy2-smp.md`).
//!
//! In VISA mode `pc` counts 16-bit parcels of the flat SRAM image. In ISA mode the program lives
//! in a separate store starting at [`ISA_BASE`] (the 48-bit address of block 0 of the ADSP-21489,
//! datasheet table of memory ranges), `pc` counts 48-bit words, every instruction advances it by
//! one and branch targets, loop ends and return addresses are word addresses. Data accesses still
//! go to the flat SRAM image (they do not alias the code store, a simplification).
use super::exec::Decoded;
use super::Core;
use crate::visa::{bits, classify_isa, word48, Insn, Kind};

/// 48-bit address of the first word of block 0 (the interrupt vector table, reset at +5 in the
/// kernel images). VERIFIED: the kernel's direct calls and jumps target `0x8C0xx`.
pub const ISA_BASE: u32 = 0x8_C000;

impl Core {
    /// Loads a raw ISA image (6 bytes per word, little endian, as stored in the firmware) at
    /// [`ISA_BASE`] and switches the core to ISA mode.
    pub fn load_isa(&mut self, image: &[u8]) {
        self.isa_code.clear();
        for w in image.as_chunks::<6>().0 {
            let lo = u16::from_le_bytes([w[0], w[1]]);
            let mid = u16::from_le_bytes([w[2], w[3]]);
            let hi = u16::from_le_bytes([w[4], w[5]]);
            self.isa_code.extend_from_slice(&[hi, mid, lo]);
        }
    }

    /// Rebuilds the ISA words covering normal-word address `addr` from the flat SRAM image after
    /// a DMA wrote there. Block 0 holds the 48-bit space from [`ISA_BASE`] at normal-word
    /// `0x92000` (VERIFIED: `0x80000 + (0x8C000 - 0x80000) * 6 / 4`, ADSP-21489 block 0 map).
    pub(super) fn isa_resync(&mut self, addr: u32) {
        const NW_BASE: u32 = 0x9_2000;
        if self.isa_code.is_empty() || !(NW_BASE..NW_BASE + 0x1_0000).contains(&addr) {
            return;
        }
        let off = (addr - NW_BASE) as usize * 4;
        for w in off / 6..=(off + 3) / 6 {
            let at = NW_BASE as usize * 4 + 6 * w;
            let Some(b) = self.sram.get(at..at + 6) else {
                return;
            };
            let parcel = |k: usize| u16::from_le_bytes([b[k], b[k + 1]]);
            let (lo, mid, hi) = (parcel(0), parcel(2), parcel(4));
            if self.isa_code.len() < 3 * (w + 1) {
                return;
            }
            self.isa_code[3 * w..3 * w + 3].copy_from_slice(&[hi, mid, lo]);
        }
    }

    /// Whether the instruction at `pc` is an ISA-mode word: an ISA image is loaded and `pc` is
    /// below the short-word space (`0x100000..`), where VISA programs live (the SY1SY2 and SMP
    /// kernels jump from the ISA vector table into VISA code, `docs/research/sharc-sy1sy2-smp.md`).
    #[inline]
    pub(super) fn isa_at(&self, pc: u32) -> bool {
        self.isa_mode() && pc < 0x10_0000
    }

    /// Whether the core runs ISA-mode code.
    #[inline]
    pub(super) fn isa_mode(&self) -> bool {
        !self.isa_code.is_empty()
    }

    /// The raw 48-bit word at ISA address `pc`, if inside the loaded image.
    pub fn isa_word(&self, pc: u32) -> Option<u64> {
        let o = (pc.wrapping_sub(ISA_BASE) as usize).checked_mul(3)?;
        let p = self.isa_code.get(o..o + 3)?;
        Some(word48(p))
    }

    /// Fetches the word at `pc`; an address outside the image yields an `Unknown` instruction so
    /// that the run stops there instead of executing zeros.
    pub(super) fn fetch_isa(&self, pc: u32) -> Decoded {
        let w = self.isa_word(pc);
        let value = w.unwrap_or(0);
        let parcels = [(value >> 32) as u16, (value >> 16) as u16, value as u16];
        let kind = if w.is_some() {
            classify_isa(parcels[0])
        } else {
            Kind::Unknown
        };
        Decoded {
            pc,
            insn: Insn {
                kind,
                parcels,
                len: 3,
            },
            w: value,
            cond_ok: self.cond(bits(value, 37, 33)),
            cond_y: self.simd() && self.cond_for(true, bits(value, 37, 33)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ISA words as stored: `r1 = 0x1420` (`0f81 0000 1420`), `r2 = r1` style move is not needed.
    fn image(words: &[[u16; 3]]) -> Vec<u8> {
        let mut out = Vec::new();
        for [hi, mid, lo] in words {
            for p in [lo, mid, hi] {
                out.extend_from_slice(&p.to_le_bytes());
            }
        }
        out
    }

    #[test]
    fn words_load_least_significant_parcel_first_and_advance_by_one() {
        // R1 = 0x1420 (Type 17a), NOP, then a jump to 0x8C000 (Type 8a direct)
        let mut c = Core::new(&[]);
        c.load_isa(&image(&[
            [0x0f81, 0x0000, 0x1420],
            [0x0000, 0x0000, 0x0000],
            [0x063e, 0x0008, 0xc000],
        ]));
        c.pc = ISA_BASE;
        assert_eq!(c.isa_word(ISA_BASE), Some(0x0f81_0000_1420));
        assert_eq!(c.run(1), None);
        assert_eq!((c.r[1], c.pc), (0x1420, ISA_BASE + 1));
        assert_eq!(c.run(2), None);
        assert_eq!(c.pc, ISA_BASE, "the jump target is a word address");
    }

    #[test]
    fn running_off_the_image_faults_instead_of_executing_zeros() {
        let mut c = Core::new(&[]);
        c.load_isa(&image(&[[0x0000, 0x0000, 0x0000]]));
        c.pc = ISA_BASE;
        assert_eq!(c.run(1), None);
        assert!(c.run(1).is_some(), "word 1 is outside the image");
        assert_eq!(c.isa_word(ISA_BASE + 1), None);
        assert_eq!(c.isa_word(0), None);
    }

    #[test]
    fn do_loop_counts_words_in_isa_mode() {
        // LCNTR = 3, DO (pc, 1) UNTIL LCE with a one-word NOP body: the loop count is what matters,
        // observed as the pc after 1 + 3 steps.
        let mut c = Core::new(&[]);
        c.load_isa(&image(&[
            [0x0c00, 0x0300, 0x0001],
            [0x0000, 0x0000, 0x0000],
            [0x0000, 0x0000, 0x0000],
        ]));
        c.pc = ISA_BASE;
        assert_eq!(c.run(1 + 3), None);
        assert_eq!(
            c.pc,
            ISA_BASE + 2,
            "three passes of the one-word body, then on"
        );
    }

    #[test]
    fn resync_rebuilds_isa_words_from_dma_written_memory() {
        let mut c = Core::new(&[]);
        c.load_isa(&image(&vec![[0, 0, 0]; 256]));
        // ISA word 1 is bytes 6..12 of block 0 = normal words 0x92001 (bytes 4..8) and 0x92002
        c.dm_write(0x9_2001, 0x3333_0000).unwrap(); // bytes 4..8: 00 00 33 33
        c.dm_write(0x9_2002, 0x5555_4444).unwrap(); // bytes 8..12: 44 44 55 55
        c.isa_resync(0x9_2001);
        c.isa_resync(0x9_2002);
        // bytes 6..12 = 33 33 44 44 55 55: parcels lo = 0x3333, mid = 0x4444, hi = 0x5555
        assert_eq!(c.isa_word(ISA_BASE + 1), Some(0x5555_4444_3333));
        assert_eq!(c.isa_word(ISA_BASE), Some(0), "word 0 untouched");
        c.isa_resync(0x9_3000); // outside the 256-word table: ignored
    }

    #[test]
    fn pc_outside_the_isa_window_fetches_visa_and_vectors_use_the_isa_table() {
        let mut c = Core::new(&[]);
        let mut words = vec![[0u16, 0, 0]; 256];
        words[48] = [0x0f81, 0x0000, 0x1420]; // vector 12 (P1I): R1 = 0x1420, then RTI
        words[49] = [0x0b3e, 0, 0];
        c.load_isa(&image(&words));
        assert!(c.isa_at(ISA_BASE) && !c.isa_at(0x1C_0B2D));
        c.pc = ISA_BASE + 100;
        c.mode1 = 1 << 12;
        c.imask = 1 << 12;
        assert!(c.raise_interrupt(12));
        assert_eq!(c.run(3), None);
        assert_eq!(c.interrupts_taken(), 1);
        assert_eq!(c.r[1], 0x1420, "vector 12 is word 48 of the ISA table");
        assert_eq!(
            c.pc,
            ISA_BASE + 101,
            "RTI returned to word 100, which then ran"
        );
    }

    #[test]
    fn idle_in_a_delay_slot_wakes_on_a_pending_interrupt() {
        // JUMP (DB) to word 8, IDLE and NOP in the delay slots: the boot kernel's exit
        let mut c = Core::new(&[]);
        let mut words = vec![[0u16, 0, 0]; 256];
        words[0] = [0x063e, 0x0408, 0xc008];
        words[1] = [0x0080, 0, 0];
        words[48] = [0x0b3e, 0, 0];
        c.load_isa(&image(&words));
        c.pc = ISA_BASE;
        c.mode1 = 1 << 12;
        c.imask = 1 << 12;
        assert_eq!(c.run(2), None); // the jump and the IDLE
        assert!(c.is_idle());
        assert!(c.raise_interrupt(12));
        assert_eq!(c.run(20), None);
        assert!(!c.is_idle());
        assert_eq!(
            c.interrupts_taken(),
            1,
            "taken once the branch has completed"
        );
        assert!(c.pc >= ISA_BASE + 8, "execution continued past the jump");
    }
}
