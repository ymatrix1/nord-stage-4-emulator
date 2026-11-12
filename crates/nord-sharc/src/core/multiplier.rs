//! Fixed-point multiplier of [`Core`]: `Rn|MRF|MRB = [MRx +|-] Rx * Ry (mod)` and `MRx = 0`.
use super::Core;

impl Core {
    /// Executes a multiplier fixed-point op. Returns false (and changes nothing) for the forms
    /// not modelled (`SAT`, `RND` of MR, integer rounding).
    ///
    /// ASSUMED (programming reference 3-14..3-17, 11-49): the active result register is always
    /// MRF (the MODE1 SRCU swap is ignored), rounding is round-to-nearest-even at bit 32 and
    /// clears MR0, and `Rn` saturates on overflow.
    pub(super) fn fixed_mult(&mut self, op: u64, n: usize, x: usize, y: usize) -> bool {
        let cls = op >> 6;
        if cls == 0 {
            return match op {
                0x14 => {
                    self.mr[0] = 0;
                    true
                }
                0x16 => {
                    self.mr[1] = 0;
                    true
                }
                _ => self.saturate_or_round_mr(op, n),
            };
        }
        let (sy, sx) = ((op >> 5) & 1 == 1, (op >> 4) & 1 == 1);
        let (frac, sel, round) = ((op >> 3) & 1 == 1, (op >> 1) & 3, op & 1 == 1);
        if round && !frac {
            return false;
        }
        let operand = |v: u32, signed: bool| {
            if signed {
                v as i32 as i128
            } else {
                v as i128
            }
        };
        let mut p = operand(self.r[x], sx) * operand(self.r[y], sy);
        if frac {
            p <<= 1;
        }
        let reg = usize::from(sel == 1 || sel == 3);
        let mut v = match cls {
            1 => p,
            2 => self.mr[reg] + p,
            _ => self.mr[reg] - p,
        };
        if round {
            let low = v & 0xFFFF_FFFF;
            v += 0x8000_0000;
            if low == 0x8000_0000 {
                v &= !(1i128 << 32);
            }
            v &= !0xFFFF_FFFF;
        }
        v = (v << 48) >> 48;
        if sel < 2 {
            self.set_fixed(n, saturate_to_register(v, frac, sx || sy));
        } else {
            self.mr[reg] = v;
        }
        true
    }
}

impl Core {
    /// `Rn = MRxF|MRxB` and `MRxF|MRxB = Rn` (compute bit 22 set, bits 21..20 clear, operation
    /// code = bits 19..12: low two bits select MR0/MR1/MR2, bit 2 the background register, bit 4
    /// a write). VERIFIED by reading all 256 codes from ADI's disassembler (docs 5.36). A read of
    /// MR2 is sign-extended from its 16 bits; a write to MR1 sign-extends into MR2; a write to MR0
    /// leaves the upper bits alone (programming reference 3-14).
    pub(super) fn mr_transfer(&mut self, op: u64, n: usize) -> bool {
        let (field, background, write) =
            ((op & 3) as usize, (op >> 2) & 1 == 1, (op >> 4) & 1 == 1);
        if field == 3 || op & !0x17 != 0 {
            return false;
        }
        let reg = usize::from(background);
        let v = self.mr[reg];
        if !write {
            self.r[n] = match field {
                0 => v as u32,
                1 => (v >> 32) as u32,
                _ => (((v >> 64) as i16) as i32) as u32,
            };
            return true;
        }
        let r = i128::from(self.r[n]);
        let low = v & 0xFFFF_FFFF;
        let low64 = v & 0xFFFF_FFFF_FFFF_FFFF;
        let new = match field {
            0 => (v & !0xFFFF_FFFFi128) | r,
            1 => i128::from(self.r[n] as i32) << 32 | low,
            _ => i128::from(self.r[n] as u16 as i16) << 64 | low64,
        };
        self.mr[reg] = (new << 48) >> 48;
        true
    }

    /// `Rn|MRx = SAT MRx (SI|UI|SF|UF)` and `Rn|MRx = RND MRx (SF|UF)` (opcode class 0).
    ///
    /// ASSUMED: the saturated 80-bit maxima are the usual ones (fractional positive
    /// `0000:7FFFFFFF:FFFFFFFF`, negative `FFFF:80000000:00000000`; integer positive
    /// `0000:00000000:7FFFFFFF`, negative `FFFF:FFFFFFFF:80000000`; unsigned all ones in the
    /// 32 or 64 bit field), since the table of the reference is garbled in the text extraction.
    fn saturate_or_round_mr(&mut self, op: u64, n: usize) -> bool {
        let (sel, unsigned, frac) = ((op >> 1) & 3, op & 1 == 1, (op >> 3) & 1 == 1);
        let reg = usize::from(sel == 1 || sel == 3);
        let mut v = self.mr[reg];
        match op >> 4 {
            0 => v = saturate_mr(v, frac, !unsigned),
            1 if frac => {
                let low = v & 0xFFFF_FFFF;
                v += 0x8000_0000;
                if low == 0x8000_0000 {
                    v &= !(1i128 << 32);
                }
                v &= !0xFFFF_FFFF;
                v = (v << 48) >> 48;
            }
            _ => return false,
        }
        if sel < 2 {
            self.set_fixed(n, saturate_to_register(v, frac, !unsigned));
        } else {
            self.mr[reg] = v;
        }
        true
    }
}

/// An 80-bit accumulator value saturated to the maximum of its format when it overflows.
fn saturate_mr(v: i128, frac: bool, signed: bool) -> i128 {
    let fits = match (frac, signed) {
        (true, true) => (v >> 63) == 0 || (v >> 63) == -1,
        (false, true) => (v >> 31) == 0 || (v >> 31) == -1,
        (true, false) => (0..1i128 << 64).contains(&v),
        (false, false) => (0..1i128 << 32).contains(&v),
    };
    if fits {
        return v;
    }
    match (frac, signed, v < 0) {
        (true, true, false) => 0x7FFF_FFFF_FFFF_FFFF,
        (true, true, true) => -(1i128 << 63),
        (false, true, false) => 0x7FFF_FFFF,
        (false, true, true) => -(1i128 << 31),
        (true, false, false) => (1i128 << 64) - 1,
        (false, false, false) => (1i128 << 32) - 1,
        (_, false, true) => 0,
    }
}

/// The 32-bit register value of an 80-bit result: bits 63..32 (fractional) or 31..0 (integer),
/// saturated to the format's maximum when the result does not fit.
fn saturate_to_register(v: i128, frac: bool, signed: bool) -> u32 {
    match (frac, signed) {
        (true, true) if (v >> 63) == 0 || (v >> 63) == -1 => (v >> 32) as u32,
        (false, true) if (v >> 31) == 0 || (v >> 31) == -1 => v as u32,
        (_, true) if v < 0 => 0x8000_0000,
        (_, true) => 0x7FFF_FFFF,
        (true, false) if (0..1i128 << 64).contains(&v) => (v >> 32) as u32,
        (false, false) if (0..1i128 << 32).contains(&v) => v as u32,
        (_, false) if v < 0 => 0,
        (_, false) => 0xFFFF_FFFF,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mult_word(op: u64, n: u64, x: u64, y: u64) -> u64 {
        (1 << 20) | (op << 12) | (n << 8) | (x << 4) | y
    }

    #[test]
    fn fixed_multiplier_fractional_integer_saturate_and_accumulate() {
        let mut c = Core::new(&[]);
        // SSF: 0.5 * 0.5 = 0.25
        c.r[1] = 0x4000_0000;
        c.r[2] = 0x4000_0000;
        assert!(c.compute(mult_word(0x78, 3, 1, 2)));
        assert_eq!(c.r[3], 0x2000_0000);
        // SSF: -1 * -1 overflows and saturates
        c.r[1] = 0x8000_0000;
        c.r[2] = 0x8000_0000;
        assert!(c.compute(mult_word(0x78, 3, 1, 2)));
        assert_eq!(c.r[3], 0x7FFF_FFFF);
        // SSI: -3 * 7 = -21
        c.r[1] = (-3i32) as u32;
        c.r[2] = 7;
        assert!(c.compute(mult_word(0x70, 3, 1, 2)));
        assert_eq!(c.r[3] as i32, -21);
        // MRF = SSF product, then Rn = MRF + product, MRF = 0
        c.r[1] = 0x4000_0000;
        c.r[2] = 0x2000_0000;
        assert!(c.compute(mult_word(0x78 | 4, 0, 1, 2)));
        assert!(c.compute(mult_word(0x80 | 0x38, 4, 1, 2)));
        assert_eq!(c.r[4], 0x2000_0000);
        assert!(c.compute(mult_word(0x14, 0, 0, 0)));
        assert_eq!(c.mr[0], 0);
        // SSFR rounds half to even at bit 32
        c.r[1] = 0x0000_0001;
        c.r[2] = 0x4000_0000;
        assert!(c.compute(mult_word(0x79, 3, 1, 2)));
        assert_eq!(c.r[3], 0);
        assert!(
            !c.compute(mult_word(0x71, 3, 1, 2)),
            "integer rounding is not modelled"
        );
    }

    #[test]
    fn saturate_and_round_the_accumulator() {
        let mut c = Core::new(&[]);
        // overflowing fractional signed value: R3 = SAT MRF (SF) -> 0x7FFFFFFF
        c.mr[0] = 1i128 << 70;
        assert!(c.compute(mult_word(0x08, 3, 0, 0)));
        assert_eq!(c.r[3], 0x7FFF_FFFF);
        // MRF = SAT MRF (SF) stores the saturated maximum
        assert!(c.compute(mult_word(0x08 | 4, 0, 0, 0)));
        assert_eq!(c.mr[0], 0x7FFF_FFFF_FFFF_FFFF);
        // in range: unchanged, integer view in the register
        c.mr[0] = 0x1234;
        assert!(c.compute(mult_word(0x00, 4, 0, 0)));
        assert_eq!(c.r[4], 0x1234);
        // R5 = RND MRF (SF): half rounds to even at bit 32
        c.mr[0] = 0x3 << 31; // 1.5 in MR1
        assert!(c.compute(mult_word(0x18 | 8, 5, 0, 0)));
        assert_eq!(c.r[5], 2);
        assert!(
            !c.compute(mult_word(0x12, 5, 0, 0)),
            "integer RND is not an operation"
        );
    }

    /// ADI: compute `mr1f=r3` (op 0x11), `r3=mr0f` (0x00), `r3=mr2f` (0x02), `mr0b=r3` (0x14).
    #[test]
    fn mr_registers_transfer_to_and_from_the_register_file() {
        let word = |op: u64, n: u64| (4 << 20) | (op << 12) | (n << 8) | (n << 4) | n;
        let mut c = Core::new(&[]);
        c.mr[0] = (0x1234i128 << 64) | (0xAAAA_BBBBi128 << 32) | 0xCCCC_DDDD;
        assert!(c.compute(word(0x00, 3)));
        assert_eq!(c.r[3], 0xCCCC_DDDD, "MR0F is the low 32 bits");
        assert!(c.compute(word(0x01, 3)));
        assert_eq!(c.r[3], 0xAAAA_BBBB);
        assert!(c.compute(word(0x02, 3)));
        assert_eq!(c.r[3], 0x1234, "MR2F is 16 bits");
        // MR2 is sign-extended when read
        c.mr[0] = -1;
        assert!(c.compute(word(0x02, 4)));
        assert_eq!(c.r[4], 0xFFFF_FFFF);
        // writing MR1 sign-extends into MR2 and keeps MR0
        c.mr[1] = 0x55;
        c.r[3] = 0x8000_0001;
        assert!(c.compute(word(0x15, 3)));
        assert_eq!(c.mr[1] >> 32, -(0x7FFF_FFFFi128), "sign-extended MR2:MR1");
        assert_eq!(c.mr[1] & 0xFFFF_FFFF, 0x55);
        // writing MR0 leaves the rest
        c.mr[0] = 0x7777i128 << 32;
        c.r[3] = 9;
        assert!(c.compute(word(0x10, 3)));
        assert_eq!(c.mr[0], (0x7777i128 << 32) | 9);
        assert!(!c.compute(word(0x03, 3)), "there is no MR3");
    }

    #[test]
    fn unsigned_results_saturate_to_the_format_maximum() {
        assert_eq!(saturate_to_register(1i128 << 64, true, false), 0xFFFF_FFFF);
        assert_eq!(saturate_to_register(-5, false, false), 0);
        assert_eq!(saturate_to_register(1i128 << 40, false, true), 0x7FFF_FFFF);
        assert_eq!(
            saturate_to_register(-(1i128 << 40), false, true),
            0x8000_0000
        );
    }
}
