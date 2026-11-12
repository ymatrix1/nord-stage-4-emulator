//! Barrel shifter of [`Core`]: shifts, rotates, single-bit operations, field deposit and extract,
//! and the exponent and leading-bit counts (programming reference rev 2.4, section 11, "Shifter
//! and Shift Immediate Computations").
//!
//! Operands are the 32-bit fixed-point fields only (the 8-bit extension field is not modelled).
//! The bit FIFO operations (`BITDEP`, `BITEXT`, `BFFWRP`) live in `bitfifo.rs`. Not modelled:
//! `FPACK`/`FUNPACK`.
use super::bitfifo::{BFFWRP_WRITE, BITDEP, BITEXT, BITEXT_NU};
use super::exec::Decoded;
use super::{sign_extend, Core, Fault};
use crate::visa::bits;

/// Mask of the low `len` bits (`len` of 32 or more selects all 32).
#[inline]
fn low_mask(len: u32) -> u32 {
    if len >= 32 {
        u32::MAX
    } else {
        (1u32 << len) - 1
    }
}

/// `v` shifted left by `s` (right when negative); an off-scale shift gives 0, or the sign
/// fill for an arithmetic right shift.
fn shift(v: u32, s: i32, arithmetic: bool) -> u32 {
    if s >= 0 {
        if s >= 32 {
            0
        } else {
            v << s
        }
    } else if -s >= 32 {
        if arithmetic {
            ((v as i32) >> 31) as u32
        } else {
            0
        }
    } else if arithmetic {
        ((v as i32) >> -s) as u32
    } else {
        v >> -s
    }
}

/// Field deposit: the low `len` bits of `x` placed at bit `pos`; returns the 32-bit result and
/// whether bits fell off the left end (`len + pos > 32`). With `sign_extend` the bits above the
/// field are filled with the field's MSB unless that MSB is off-scale left.
fn deposit(x: u32, len: u32, pos: u32, sign_extend: bool) -> (u32, bool) {
    let len = len.min(32);
    let field = u64::from(x & low_mask(len));
    let mut wide = field << pos;
    let msb_pos = pos + len; // one above the field's MSB
    if sign_extend && len > 0 && msb_pos <= 32 && (field >> (len - 1)) & 1 == 1 {
        wide |= (u64::MAX << msb_pos) & u64::from(u32::MAX);
    }
    (wide as u32, len + pos > 32)
}

/// Field extract: `len` bits of `x` starting at bit `pos`, right aligned; returns the result and
/// whether bits came from beyond the left end (`len + pos > 32`).
fn extract(x: u32, len: u32, pos: u32, sign_extend: bool) -> (u32, bool) {
    let len = len.min(32);
    let src = if pos >= 32 { 0 } else { x >> pos };
    let mut v = src & low_mask(len);
    if sign_extend && len > 0 && pos + len <= 32 && (v >> (len - 1)) & 1 == 1 {
        v |= !low_mask(len);
    }
    (v, len + pos > 32)
}

/// Register-form shifter opcode of a shift-immediate operation code (bits 21..16 of the `0x02`
/// word), VERIFIED against ADI's disassembler for all 64 codes (docs 5.34); `bitext`, `bitdep`
/// and `bffwrp` (bit FIFO) are not modelled.
fn immediate_to_register_op(op6: u64) -> Option<u64> {
    Some(match op6 {
        0 => 0x00,
        1 => 0x04,
        2 => 0x08,
        8 => 0x20,
        9 => 0x24,
        16 => 0x40,
        17 => 0x44,
        18 => 0x48,
        19 => 0x4C,
        20 => BITEXT,
        22 => BITEXT_NU,
        25 => 0x64,
        27 => 0x6C,
        29 => BITDEP,
        31 => BFFWRP_WRITE,
        48 => 0xC0,
        49 => 0xC4,
        50 => 0xC8,
        51 => 0xCC,
        _ => return None,
    })
}

impl Core {
    /// Opcode `0x02`: `[IF cond] Rn = <shifter op> Rx BY <immediate>`. The 12-bit immediate is the
    /// signed shift amount, the bit position, or `len6:bit6` packed exactly like the register form.
    pub(super) fn exec_shift_imm(&mut self, d: &Decoded) -> Result<(), Fault> {
        let w = d.w;
        let data = ((bits(w, 30, 27) << 8) | bits(w, 15, 8)) as u32;
        let (n, x) = (bits(w, 7, 4) as usize, bits(w, 3, 0) as usize);
        let Some(op) = immediate_to_register_op(bits(w, 21, 16)) else {
            return self.unmodelled(d.pc, &d.insn);
        };
        let ry = if matches!(op, 0x00 | 0x04 | 0x08 | 0x20 | 0x24) {
            sign_extend(data, 12)
        } else {
            data
        };
        self.on_each_pe(d, d.cond_ok, d.cond_y, |core| {
            core.shifter_op(op, n, core.r[x], ry)
        })
    }

    pub(super) fn set_shifter_flags(&mut self, sz: bool, sv: bool, ss: bool) {
        self.flags.sz = sz;
        self.flags.sv = sv;
        self.flags.ss = ss;
    }

    /// Executes a shifter operation (compute unit 2, Table 12-11 opcodes). Returns false for the
    /// forms not modelled, leaving the state unchanged.
    pub(super) fn shifter(&mut self, op: u64, n: usize, x: usize, y: usize) -> bool {
        self.shifter_op(op, n, self.r[x], self.r[y])
    }

    /// [`Core::shifter`] with the operand values given (register or immediate form).
    fn shifter_op(&mut self, op: u64, n: usize, rx: u32, ry: u32) -> bool {
        let old = self.r[n];
        let (len, pos) = ((ry >> 6) & 63, ry & 63);
        let bit = ry as i32;
        let bit_ok = (0..32).contains(&bit);
        let result = match op {
            // LSHIFT / ASHIFT / ROT by Ry (positive left), with the OR variants
            0x00 | 0x20 | 0x04 | 0x24 => {
                let r = shift(rx, ry as i32, op & 4 != 0) | if op & 0x20 != 0 { old } else { 0 };
                self.set_shifter_flags(r == 0, (ry as i32) > 0, false);
                r
            }
            0x08 => {
                let r = rx.rotate_left((ry as i32).rem_euclid(32) as u32);
                self.set_shifter_flags(r == 0, false, false);
                r
            }
            // BSET, BCLR, BTGL: one bit by position Ry; out of range leaves the operand
            0xC0 | 0xC4 | 0xC8 => {
                let r = match (bit_ok, op) {
                    (false, _) => rx,
                    (true, 0xC0) => rx | (1 << bit),
                    (true, 0xC4) => rx & !(1 << bit),
                    (true, _) => rx ^ (1 << bit),
                };
                self.set_shifter_flags(r == 0, bit > 31, false);
                r
            }
            0xCC => {
                // BTST: SZ set when the bit is 0 or the position is off-scale; no result
                let zero = !bit_ok || (rx >> bit) & 1 == 0;
                self.set_shifter_flags(zero, bit > 31, false);
                return true;
            }
            // FDEP / FEXT, with (SE) = bit 3 and OR = bit 5
            0x44 | 0x4C | 0x64 | 0x6C => {
                let (v, over) = deposit(rx, len, pos, op & 8 != 0);
                let r = v | if op & 0x20 != 0 { old } else { 0 };
                self.set_shifter_flags(r == 0, over, false);
                r
            }
            0x40 | 0x48 => {
                let (r, over) = extract(rx, len, pos, op & 8 != 0);
                self.set_shifter_flags(r == 0, over, false);
                r
            }
            // EXP, EXP (EX): the two's complement of (leading sign bits - 1)
            0x80 | 0x84 => {
                let sign = rx >> 31 == 1;
                let leading = if sign {
                    rx.leading_ones()
                } else {
                    rx.leading_zeros()
                };
                let extended = op == 0x84 && self.flags.av;
                let exp = if extended { 1 } else { 1 - leading as i32 };
                let ss = if op == 0x84 {
                    self.flags.av ^ sign
                } else {
                    sign
                };
                self.set_shifter_flags(exp == 0, false, ss);
                exp as u32
            }
            0x88 | 0x8C => {
                let count = if op == 0x88 {
                    rx.leading_zeros()
                } else {
                    rx.leading_ones()
                };
                let msb = rx >> 31 == 1;
                self.set_shifter_flags(msb == (op == 0x88), count == 32, false);
                count
            }
            // bit FIFO: BITEXT (NU), BFFWRP read, BITDEP, BFFWRP write
            0x50 | 0x58 | 0x70 | 0x74 | 0x7C => return self.bitfifo_op(op, n, rx, ry),
            _ => return false,
        };
        self.r[n] = result;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compute word of a shifter operation `Rn = op Rx BY Ry`.
    fn word(op: u64, n: u64, x: u64, y: u64) -> u64 {
        (2 << 20) | (op << 12) | (n << 8) | (x << 4) | y
    }

    fn run(op: u64, rx: u32, ry: u32, old: u32) -> (Core, u32) {
        let mut c = Core::new(&[]);
        (c.r[1], c.r[2], c.r[3]) = (rx, ry, old);
        assert!(c.compute(word(op, 3, 1, 2)), "op {op:#x}");
        let r = c.r[3];
        (c, r)
    }

    #[test]
    fn logical_and_arithmetic_shifts_by_a_signed_amount() {
        assert_eq!(run(0x00, 0x8000_0001, 4, 0).1, 0x0000_0010);
        assert_eq!(run(0x00, 0x8000_0000, (-4i32) as u32, 0).1, 0x0800_0000);
        assert_eq!(run(0x04, 0x8000_0000, (-4i32) as u32, 0).1, 0xF800_0000);
        assert_eq!(run(0x00, 1, 40, 0).1, 0, "off-scale left");
        assert_eq!(run(0x04, 0x8000_0000, (-40i32) as u32, 0).1, 0xFFFF_FFFF);
        assert_eq!(run(0x20, 1, 4, 0x100).1, 0x110, "OR variant");
        let (c, _) = run(0x00, 1, 0, 0);
        assert!(!c.flags.sz && !c.flags.sv);
        let (c, _) = run(0x00, 1, 40, 0);
        assert!(c.flags.sz && c.flags.sv);
    }

    #[test]
    fn rotate_and_single_bit_operations() {
        assert_eq!(run(0x08, 0x8000_0001, 1, 0).1, 0x0000_0003);
        assert_eq!(run(0x08, 0x8000_0001, (-1i32) as u32, 0).1, 0xC000_0000);
        assert_eq!(run(0xC0, 0, 5, 0).1, 0x20);
        assert_eq!(run(0xC4, 0xFF, 0, 0).1, 0xFE);
        assert_eq!(run(0xC8, 0x10, 4, 0).1, 0);
        assert_eq!(run(0xC0, 7, 32, 0).1, 7, "position off-scale: no change");
        assert!(run(0xC0, 7, 32, 0).0.flags.sv);
        // BTST sets SZ for a 0 bit and clears it for a 1 bit
        assert!(run(0xCC, 0b10, 0, 99).0.flags.sz);
        assert!(!run(0xCC, 0b10, 1, 99).0.flags.sz);
        assert_eq!(run(0xCC, 0b10, 1, 99).1, 99, "BTST writes no register");
    }

    #[test]
    fn field_deposit_matches_the_manual_example() {
        // len6 = 14, bit6 = 13: 14 bits of Rx land in Rn bits 26..13 (manual figure 11-1)
        let ry = (14 << 6) | 13;
        let rx = 0x0000_3FFF | 0xFFFF_0000;
        let (c, r) = run(0x44, rx, ry, 0);
        assert_eq!(r, 0x3FFF << 13);
        assert!(!c.flags.sv);
        // deposit running off the left end sets SV
        assert!(run(0x44, 0xFF, (16 << 6) | 20, 0).0.flags.sv);
        // sign extension: field 0b100 (3 bits) at bit 4 fills the bits above
        assert_eq!(run(0x4C, 0b100, (3 << 6) | 4, 0).1, 0xFFFF_FFC0);
        // OR variant keeps the other bits of Rn
        assert_eq!(run(0x64, 0b11, (2 << 6) | 4, 0x1).1, 0x31);
    }

    #[test]
    fn field_extract_and_sign_extend() {
        let ry = (8 << 6) | 4; // 8 bits from bit 4
        assert_eq!(run(0x40, 0x0000_0ABC, ry, 0).1, 0xAB);
        assert_eq!(
            run(0x48, 0x0000_0F8C, ry, 0).1,
            0xFFFF_FFF8,
            "0xF8 sign-extends"
        );
        assert!(run(0x40, 0xFFFF_FFFF, (16 << 6) | 20, 0).0.flags.sv);
    }

    /// ADI `elfdump` ground truth: `023e 1010 1810` is `r1=fext r0 by 0x18:0x8`, `023e 3011 0700`
    /// is `r0=fdep r0 by 0x7:0x18`, `023e 7801 ff11` is `r1=ashift r1 by 0xffffffff`.
    #[test]
    fn shift_immediate_words_match_the_adi_disassembly() {
        use super::super::support::core_at_zero;
        // r1 = fext r0 by 0x18:0x8: top byte of R0
        let mut c = core_at_zero(&[0x023e, 0x1010, 0x1810]);
        c.r[0] = 0xAB12_3456;
        assert_eq!(c.step(), Ok(()));
        assert_eq!((c.r[1], c.pc), (0xAB, 3));
        // r0 = fdep r0 by 0x7:0x18: 24 bits of R0 deposited at bit 7
        let mut c = core_at_zero(&[0x023e, 0x3011, 0x0700]);
        c.r[0] = 0x00AB_CDEF;
        assert_eq!(c.step(), Ok(()));
        assert_eq!(c.r[0], 0x00AB_CDEF << 7);
        // r1 = ashift r1 by -1 (data12 = 0xfff)
        let mut c = core_at_zero(&[0x023e, 0x7801, 0xff11]);
        c.r[1] = 0x8000_0002;
        assert_eq!(c.step(), Ok(()));
        assert_eq!(c.r[1], 0xC000_0001);
        // r4 = btgl r4 by 0
        let mut c = core_at_zero(&[0x023e, 0x0032, 0x0044]);
        c.r[4] = 0x10;
        assert_eq!(c.step(), Ok(()));
        assert_eq!(c.r[4], 0x11);
        // r3 = bitext 0x5 is a bit FIFO form (see bitfifo.rs); FPACK stays unmodelled
        let mut c = core_at_zero(&[0x023e, 0x0014, 0x0535]);
        assert_eq!(c.step(), Ok(()));
        assert_eq!(c.r[3], 0);
    }

    #[test]
    fn exponent_and_leading_bit_counts() {
        assert_eq!(run(0x80, 0x0000_0001, 0, 0).1 as i32, -30);
        assert_eq!(run(0x80, 0x4000_0000, 0, 0).1 as i32, 0);
        assert!(run(0x80, 0x4000_0000, 0, 0).0.flags.sz);
        assert_eq!(run(0x80, 0xFFFF_FFFF, 0, 0).1 as i32, -31);
        assert!(run(0x80, 0xFFFF_FFFF, 0, 0).0.flags.ss);
        assert_eq!(run(0x88, 0x0000_FFFF, 0, 0).1, 16);
        assert_eq!(run(0x8C, 0xFFF0_0000, 0, 0).1, 12);
        assert!(run(0x88, 0, 0, 0).0.flags.sv, "LEFTZ of 0 is 32");
        let mut c = Core::new(&[]);
        c.flags.av = true;
        c.r[1] = 0x4000_0000;
        assert!(c.compute(word(0x84, 3, 1, 2)));
        assert_eq!(c.r[3], 1, "EXP (EX) reports the ALU overflow bit");
        let mut c = Core::new(&[]);
        assert!(!c.compute(word(0x90, 3, 1, 2)), "FPACK is not modelled");
    }
}
