//! Barrel shifter of [`Core`]: shifts, rotates, single-bit operations, field deposit and extract,
//! and the exponent and leading-bit counts (programming reference rev 2.4, section 11, "Shifter
//! and Shift Immediate Computations").
//!
//! Operands are the 32-bit fixed-point fields only (the 8-bit extension field is not modelled).
//! Not modelled: the bit FIFO operations (`BITDEP`, `BITEXT`, `BFFWRP`).
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

/// `FPACK`: an IEEE single to the 16-bit short float (sign 15, exponent 14..11, fraction 10..0).
/// Returns the packed value and whether it overflowed (programming reference 11-84).
///
/// Source exponent `e`: above 135 the largest magnitude; 121..=135 the exponent is the MSB of `e`
/// followed by its three LSBs and the fraction is the upper 11 bits, rounded; 110..=120 a
/// denormal `1.f * 2^(e - 110)` (gradual underflow, rounded); below 110 zero. ASSUMED: rounding
/// is to nearest with ties to even, and a rounding carry increments the exponent.
fn fpack(x: u32) -> (u32, bool) {
    let sign = (x >> 31) << 15;
    let e = (x >> 23) & 0xFF;
    let frac = x & 0x7F_FFFF;
    let round = |value: u32, drop: u32| -> u32 {
        // value >> drop with round to nearest even
        let kept = value >> drop;
        let rest = value & ((1u32 << drop) - 1);
        let half = 1u32 << (drop - 1);
        kept + u32::from(rest > half || (rest == half && kept & 1 == 1))
    };
    if e > 135 {
        return (sign | 0x7FFF, true);
    }
    let magnitude = if e > 120 {
        let packed = ((((e >> 7) & 1) << 3) | (e & 7)) << 11;
        packed + round(frac, 12) // a carry out of the fraction increments the exponent
    } else if e > 109 {
        round(0x80_0000 | frac, 133 - e)
    } else {
        0
    };
    if magnitude > 0x7FFF {
        return (sign | 0x7FFF, true);
    }
    (sign | magnitude, false)
}

/// `FUNPACK`: the 16-bit short float in the low half of `x` to an IEEE single.
fn funpack(x: u32) -> u32 {
    let sign = ((x >> 15) & 1) << 31;
    let exp = (x >> 11) & 0xF;
    let frac = x & 0x7FF;
    if exp != 0 {
        let msb = exp >> 3;
        let e8 = (msb << 7) | ((1 - msb) * 0xF) << 3 | (exp & 7);
        return sign | (e8 << 23) | (frac << 12);
    }
    if frac == 0 {
        return sign;
    }
    // denormal: exponent 120 - N for N leading zeros of the 11-bit fraction, hidden 1 stripped
    let leading = frac.leading_zeros() - 21;
    let normalised = (frac << (leading + 1)) & 0x7FF;
    sign | ((120 - leading) << 23) | (normalised << 12)
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
        25 => 0x64,
        27 => 0x6C,
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

    /// Type 6a: `[IF cond] <shift immediate>, DM|PM(Ii, Mm) <-> Rn` with the shift fields of the
    /// `0x02` form and a post-modify data move: PM select bit 32 (DAG2), index 43..41, modify
    /// 40..38, direction bit 31 (1 = store), register 26..23. VERIFIED against ADI's `elfdump`
    /// on 24 shift-and-move instructions (docs 5.36). The store reads its register before the
    /// shift writes.
    pub(super) fn exec_shift_move(&mut self, d: &Decoded) -> Result<(), Fault> {
        let w = d.w;
        let bank = Self::dag_bank(bits(w, 32, 32));
        let ii = bits(w, 43, 41) as usize + bank;
        let mi = bits(w, 40, 38) as usize + bank;
        let (addr, next_i) = (self.i[ii], self.modified(ii, mi));
        let ureg = bits(w, 26, 23) as u8;
        let captured = self.capture(ureg);
        self.exec_shift_imm(d)?;
        if d.cond_ok {
            self.mem_transfer_with(d, addr, ureg, bits(w, 31, 31) == 1, captured)?;
            self.i[ii] = next_i;
        }
        Ok(())
    }

    fn set_shifter_flags(&mut self, sz: bool, sv: bool, ss: bool) {
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
            0x90 => {
                let (packed, overflow) = fpack(rx);
                self.set_shifter_flags(false, overflow, false);
                packed
            }
            0x94 => {
                self.set_shifter_flags(false, false, false);
                funpack(rx)
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
            _ => return false,
        };
        self.r[n] = result;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ADI: `8b7e 3011 4650` is `r5=fdep r0 by 0x6:0x19, r0=dm(i5,m5)`: the shift reads the old R0
    /// and the load then replaces it; I5 is post-modified by M5.
    #[test]
    fn shift_immediate_with_a_data_move() {
        use super::super::support::core_at_zero;
        let mut c = core_at_zero(&[0x8b7e, 0x3011, 0x4650]);
        (c.i[5], c.m[5]) = (0x9_0100, 4);
        c.r[0] = 0x0000_0003;
        c.dm_write(0x9_0100, 0x55).unwrap();
        assert_eq!(c.step(), Ok(()));
        assert_eq!(
            c.r[5],
            3 << 6,
            "R5 = FDEP R0 BY 6:25 (the low 25 bits of 3 at bit 6)"
        );
        assert_eq!(c.r[0], 0x55, "R0 = DM(I5, M5)");
        assert_eq!((c.i[5], c.pc), (0x9_0104, 3));
        // a failing condition skips the whole instruction: `if eq` with AZ clear (cond 0)
        let mut c = core_at_zero(&[0x8b00, 0x3011, 0x4650]);
        (c.i[5], c.m[5]) = (0x9_0100, 4);
        c.r[0] = 3;
        c.dm_write(0x9_0100, 0x55).unwrap();
        assert_eq!(c.step(), Ok(()));
        assert_eq!((c.r[5], c.r[0], c.i[5]), (0, 3, 0x9_0100));
    }

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
        // bit FIFO forms are not modelled
        let mut c = core_at_zero(&[0x023e, 0x001d, 0x0502]);
        assert!(c.step().is_err());
    }

    /// Values worked out from the reference's description of the short float format.
    #[test]
    fn short_float_pack_and_unpack() {
        let f = |x: f32| x.to_bits();
        assert_eq!(
            fpack(f(1.0)),
            (0x3800, false),
            "exponent 127 -> MSB 0, LSBs 7"
        );
        assert_eq!(
            fpack(f(2.0)),
            (0x4000, false),
            "exponent 128 -> MSB 1, LSBs 0"
        );
        assert_eq!(fpack(f(-1.0)).0, 0xB800);
        assert_eq!(funpack(0x3800), f(1.0));
        assert_eq!(funpack(0x4000), f(2.0));
        assert_eq!(funpack(0xB800), f(-1.0));
        // a fraction survives with 11 bits: 1.5 and 1 + 2^-11
        assert_eq!(funpack(fpack(f(1.5)).0), f(1.5));
        assert_eq!(
            funpack(fpack(f(1.0 + 1.0 / 2048.0)).0),
            f(1.0 + 1.0 / 2048.0)
        );
        // rounding carries into the exponent: the largest fraction below 2.0 rounds up to 2.0
        assert_eq!(fpack(0x3FFF_FFFF), (0x4000, false));
        // overflow gives the largest magnitude and sets SV, zero stays zero
        assert_eq!(fpack(f(1.0e10)), (0x7FFF, true));
        assert_eq!(fpack(f(-1.0e10)), (0xFFFF, true));
        assert_eq!(fpack(0), (0, false));
        // gradual underflow: 2^-8 packs as the denormal 0x0200 and unpacks exactly
        assert_eq!(fpack(f(2f32.powi(-8))), (0x0200, false));
        assert_eq!(funpack(0x0200), f(2f32.powi(-8)));
        assert_eq!(funpack(0x0001), f(2f32.powi(-17)), "the smallest denormal");
        assert_eq!(fpack(f(2f32.powi(-40))), (0, false), "too small");
        // the exponent range maps back for every packed value
        for packed in (0u32..0x8000).step_by(7) {
            let (again, overflow) = fpack(funpack(packed));
            assert!(!overflow);
            assert_eq!(again, packed, "round trip of {packed:#x}");
        }
    }

    #[test]
    fn fpack_funpack_through_the_compute_unit() {
        let mut c = Core::new(&[]);
        c.r[1] = 1.0f32.to_bits();
        assert!(c.compute(word(0x90, 3, 1, 0)));
        assert_eq!(c.r[3], 0x3800);
        assert!(c.compute(word(0x94, 4, 3, 0)));
        assert_eq!(f32::from_bits(c.r[4]), 1.0);
        c.r[1] = 1.0e10f32.to_bits();
        assert!(c.compute(word(0x90, 3, 1, 0)));
        assert!(c.flags.sv, "overflow sets SV");
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
    }
}
