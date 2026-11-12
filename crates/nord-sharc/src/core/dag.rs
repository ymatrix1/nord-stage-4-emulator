//! Data address generators: index modify with circular buffers, bit-reversed addressing, the
//! DAG1/DAG2 bank offset and the immediate `MODIFY`/`BITREV` instructions (Type 19a).
use super::exec::Decoded;
use super::Core;
use crate::visa::bits;

/// MODE1 bit 0: bit-reversed addressing for `I8`.
const MODE1_BR8: u32 = 1;
/// MODE1 bit 1: bit-reversed addressing for `I0`.
const MODE1_BR0: u32 = 1 << 1;
/// MODE1 bit 24: circular buffer addressing enable.
const MODE1_CBUFEN: u32 = 1 << 24;

impl Core {
    /// Register offset of a DAG selector bit: DAG1 uses `I0..I7`/`M0..M7`, DAG2 uses `I8..I15`.
    #[inline]
    pub(super) fn dag_bank(bit: u64) -> usize {
        if bit == 1 {
            8
        } else {
            0
        }
    }

    /// Next DAG index after modifying `i` by `m` with circular-buffer length `l` and base `b`.
    #[inline]
    pub(super) fn dag_next(i: u32, m: u32, l: u32, b: u32) -> u32 {
        let next = i.wrapping_add(m);
        if l == 0 {
            return next;
        }
        if (m as i32) >= 0 {
            if next >= b.wrapping_add(l) {
                next.wrapping_sub(l)
            } else {
                next
            }
        } else if next < b {
            next.wrapping_add(l)
        } else {
            next
        }
    }

    /// Index register `ii` stepped by `delta`. With `circular` the buffer registers `L`/`B` apply.
    #[inline]
    fn stepped(&self, ii: usize, delta: u32, circular: bool) -> u32 {
        let (l, b) = if circular {
            (self.l[ii], self.b[ii])
        } else {
            (0, 0)
        };
        let next = Self::dag_next(self.i[ii], delta, l, b);
        if circular && l != 0 {
            self.note_cb_wrap(ii, self.i[ii].wrapping_add(delta), next);
        }
        next
    }

    /// Index register `ii` post-modified by `delta` in a data access: circular buffering applies
    /// only while `MODE1.CBUFEN` is set (PRM 6-11; the firmware sets the bit in its start-up
    /// code, `bit set mode1 0x1c004f8`).
    #[inline]
    pub(super) fn modified_by(&self, ii: usize, delta: u32) -> u32 {
        self.stepped(ii, delta, self.mode1 & MODE1_CBUFEN != 0)
    }

    /// Index register `ii` modified by modify register `mi` (honours its `L` and `B`).
    #[inline]
    pub(super) fn modified(&self, ii: usize, mi: usize) -> u32 {
        self.modified_by(ii, self.m[mi])
    }

    /// The `MODIFY` instructions always apply `L`/`B`, independent of `CBUFEN` (PRM 6-12).
    #[inline]
    pub(super) fn modified_always(&self, ii: usize, delta: u32) -> u32 {
        self.stepped(ii, delta, true)
    }

    /// The address a data access through index `ii` puts out: bit-reversed for `I0` with
    /// `MODE1.BR0` and for `I8` with `BR8`; the register itself is not reversed (PRM 6-25).
    #[inline]
    pub(super) fn dag_out(&self, ii: usize, addr: u32) -> u32 {
        let reversed = match ii {
            0 => self.mode1 & MODE1_BR0 != 0,
            8 => self.mode1 & MODE1_BR8 != 0,
            _ => false,
        };
        if reversed {
            addr.reverse_bits()
        } else {
            addr
        }
    }

    /// Type 19a: `Id = MODIFY(Is, <data32>)` (circular, always) or `Id = BITREV(Is, <data32>)`
    /// (add, then reverse all 32 bits). Fields as printed by ADI's disassembler (VERIFIED on the
    /// firmware word `1600 0000 0028` = `i0 = modify(i0, 0x28)`): G bit 38, source 34..32,
    /// destination `(37..35) XOR (34..32)`, bit 39 selects BITREV.
    pub(super) fn exec_modify_imm(&mut self, d: &Decoded) {
        let w = d.w;
        let bank = Self::dag_bank(bits(w, 38, 38));
        let src = bits(w, 34, 32) as usize + bank;
        let dst = (bits(w, 37, 35) ^ bits(w, 34, 32)) as usize + bank;
        let data = bits(w, 31, 0) as u32;
        self.i[dst] = if bits(w, 39, 39) == 1 {
            self.i[src].wrapping_add(data).reverse_bits()
        } else {
            self.modified_always(src, data)
        };
    }
}

#[cfg(test)]
mod tests {
    use super::super::support::core_at_zero;
    use super::*;

    #[test]
    fn dag_addressing_modes_and_circular_buffers() {
        assert_eq!(Core::dag_next(10, 1, 0, 0), 11);
        assert_eq!(Core::dag_next(10, 0xFFFF_FFFF, 0, 0), 9);
        // circular buffer base 100 length 4: 103 + 1 wraps to 100, 100 - 1 wraps to 103
        assert_eq!(Core::dag_next(103, 1, 4, 100), 100);
        assert_eq!(Core::dag_next(100, 0xFFFF_FFFF, 4, 100), 103);
    }

    #[test]
    fn bank_offset_and_register_modify() {
        assert_eq!((Core::dag_bank(0), Core::dag_bank(1)), (0, 8));
        let mut c = Core::new(&[]);
        (c.i[9], c.m[10], c.l[9], c.b[9]) = (103, 1, 4, 100);
        assert_eq!(c.modified(9, 10), 104, "linear while CBUFEN is clear");
        c.mode1 = MODE1_CBUFEN;
        assert_eq!(c.modified(9, 10), 100);
        c.mode1 = 0;
        assert_eq!(c.modified_always(9, 1), 100, "MODIFY ignores CBUFEN");
    }

    #[test]
    fn bit_reversed_address_output_for_i0_and_i8_only() {
        let mut c = Core::new(&[]);
        assert_eq!(c.dag_out(0, 0x83000), 0x83000);
        c.mode1 = MODE1_BR0;
        assert_eq!(c.dag_out(0, 0x83000), 0x000C_1000, "PRM listing 6-7");
        assert_eq!(c.dag_out(1, 0x83000), 0x83000);
        assert_eq!(c.dag_out(8, 0x83000), 0x83000);
        c.mode1 = MODE1_BR8;
        assert_eq!(c.dag_out(8, 1), 0x8000_0000);
    }

    /// ADI: `1600 0000 0028` is `i0 = modify(i0, 0x28)`; the circular wrap applies.
    #[test]
    fn immediate_modify_and_bitrev_instructions() {
        let mut c = core_at_zero(&[0x1600, 0x0000, 0x0028]);
        c.i[0] = 0x100;
        assert_eq!(c.step(), Ok(()));
        assert_eq!((c.i[0], c.pc), (0x128, 3));
        // circular: base 0x100, length 0x20: 0x110 + 0x28 wraps to 0x118
        let mut c = core_at_zero(&[0x1600, 0x0000, 0x0028]);
        (c.i[0], c.b[0], c.l[0]) = (0x110, 0x100, 0x20);
        assert_eq!(c.step(), Ok(()));
        assert_eq!(c.i[0], 0x118);
        // BITREV (bit 39): I1 = BITREV(I0, 4); destination field 001 XOR source 000
        let mut c = core_at_zero(&[0x1688, 0x0000, 0x0004]);
        c.i[0] = 0x0000_0004;
        assert_eq!(c.step(), Ok(()));
        assert_eq!(c.i[1], 8u32.reverse_bits());
    }
}
