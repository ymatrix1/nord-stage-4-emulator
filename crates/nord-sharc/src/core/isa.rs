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
            cond_y: self.simd() && self.cond_for(self.flags_y, bits(value, 37, 33)),
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
}
