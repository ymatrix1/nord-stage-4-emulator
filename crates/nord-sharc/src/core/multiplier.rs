//! Fixed-point multiplier of [`Core`]: `Rn|MRF|MRB = [MRx +|-] Rx * Ry (mod)` and `MRx = 0`.
//!
//! VERIFIED against the CCES 2.12.1 ADSP-21489 simulator by `fixed_measured.rs`
//! (`docs/research/sharc-fixed/`, 4 096 multiplier rows incl. every format, accumulate/subtract
//! with extreme accumulator presets, SAT, RND and the transfers):
//! * the product of the two operands (signed or unsigned per the format letters) is shifted left
//!   by one only for the fractional **signed x signed** format, the MR is 80 bits and wraps;
//! * the `R` formats round half to even at bit 32 (the low 32 bits become zero);
//! * `Rn = ...` stores MR0 (integer formats) or MR1 (fractional formats) **without saturation**
//!   and leaves MRF/MRB unchanged; the multiplier never touches AZ/AN;
//! * MN is the sign of the 80-bit result; MV is set when it does not fit 32 bits (integer) or 64
//!   bits (fractional), signed or unsigned per the format, and latches STKY MOS (bit 6); MU is set
//!   for a nonzero fractional result whose MR1 is all zeros or all ones; MI is never set.
use super::Core;

const STKY_MOS: u32 = 1 << 6;
const STKY_MVS: u32 = 1 << 7;
const STKY_MUS: u32 = 1 << 8;

fn wrap80(v: i128) -> i128 {
    (v << 48) >> 48
}

/// Round half to even at bit 32 and clear the low 32 bits.
fn round_mr(v: i128) -> i128 {
    let low = v & 0xFFFF_FFFF;
    let mut r = v + 0x8000_0000;
    if low == 0x8000_0000 {
        r &= !(1i128 << 32);
    }
    wrap80(r & !0xFFFF_FFFF)
}

/// Whether `v` fits the 32-bit (integer) or 64-bit (fractional) window of the format.
fn fits(v: i128, frac: bool, unsigned: bool) -> bool {
    match (frac, unsigned) {
        (false, false) => (-(1i128 << 31)..1i128 << 31).contains(&v),
        (false, true) => (0..1i128 << 32).contains(&v),
        (true, false) => (-(1i128 << 63)..1i128 << 63).contains(&v),
        (true, true) => (0..1i128 << 64).contains(&v),
    }
}

impl Core {
    /// MN, MV, MU, MI of the multiplier (ASTAT bits 6..9) and the sticky bits they latch.
    fn set_mult_flags(&mut self, mn: bool, mv: bool, mu: bool, sticky: u32) {
        let nibble = u32::from(mn) | u32::from(mv) << 1 | u32::from(mu) << 2;
        let extra = &mut self.sys.astat_extra[0];
        *extra = *extra & !(0xF << 6) | nibble << 6;
        if sticky != 0 {
            self.note_arith_exception(false, sticky);
        }
    }

    /// Executes a multiplier fixed-point op. Returns false (and changes nothing) for the forms
    /// not modelled (integer rounding, SAT/RND forms that do not exist).
    ///
    /// ASSUMED: the active result register is always MRF (the MODE1 SRCU swap is ignored).
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
        if frac && sx && sy {
            p <<= 1;
        }
        let reg = usize::from(sel == 1 || sel == 3);
        let mut v = wrap80(match cls {
            1 => p,
            2 => self.mr[reg] + p,
            _ => self.mr[reg] - p,
        });
        if round {
            v = round_mr(v);
        }
        let unsigned = !sx && !sy;
        let mv = !fits(v, frac, unsigned);
        let mu = frac && v != 0 && matches!(v >> 32, 0 | -1) && v & 0xFFFF_FFFF != 0;
        self.set_mult_flags(v < 0, mv, mu, if mv { STKY_MOS } else { 0 });
        if sel < 2 {
            self.r[n] = if frac { (v >> 32) as u32 } else { v as u32 };
            self.fx.clear(n);
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
        self.mr[reg] = wrap80(new);
        true
    }

    /// `Rn|MRx = SAT MRx (SI|UI|SF|UF)` and `Rn|MRx = RND MRx (SF|UF)` (opcode class 0).
    ///
    /// VERIFIED (simulator): SAT clamps to the window of the format (signed or unsigned, 32 bits
    /// for the integer and 64 bits for the fractional formats; an out-of-range value in an
    /// unsigned format gives the maximum whatever its sign) and sets MN for a negative result; RND
    /// rounds half to even at bit 32, sets MN for a negative and MV for an out-of-window result
    /// and latches STKY MVS (bit 7). `Rn` receives MR0 (integer) or MR1 (fractional) of the
    /// result, the MR form stores the result.
    fn saturate_or_round_mr(&mut self, op: u64, n: usize) -> bool {
        let (sel, unsigned, frac) = ((op >> 1) & 3, op & 1 == 0, (op >> 3) & 1 == 1);
        let reg = usize::from(sel == 1 || sel == 3);
        let v = self.mr[reg];
        let (out, mv) = match op >> 4 {
            0 => {
                let (lo, hi) = match (frac, unsigned) {
                    (false, false) => (-(1i128 << 31), (1i128 << 31) - 1),
                    (false, true) => (0, (1i128 << 32) - 1),
                    (true, false) => (-(1i128 << 63), (1i128 << 63) - 1),
                    (true, true) => (0, (1i128 << 64) - 1),
                };
                let c = if (lo..=hi).contains(&v) {
                    v
                } else if !unsigned && v < 0 {
                    lo
                } else {
                    hi
                };
                (c, false)
            }
            1 if frac => {
                let r = round_mr(v);
                // VERIFIED: MV also reports an input outside the window (-1 in an unsigned format)
                (
                    r,
                    !fits(r, true, unsigned)
                        || !fits(v, true, unsigned)
                        || (v < 0 && v + 0x8000_0000 >= 0),
                )
            }
            _ => return false,
        };
        // MU as for the multiplier, but here it latches STKY MUS (bit 8)
        let mu = frac && out != 0 && matches!(out >> 32, 0 | -1) && out & 0xFFFF_FFFF != 0;
        let sticky = if mv { STKY_MVS } else { 0 } | if mu { STKY_MUS } else { 0 };
        self.set_mult_flags(out < 0, mv, mu, sticky);
        if sel < 2 {
            self.r[n] = if frac { (out >> 32) as u32 } else { out as u32 };
            self.fx.clear(n);
        } else {
            self.mr[reg] = out;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mult_word(op: u64, n: u64, x: u64, y: u64) -> u64 {
        (1 << 20) | (op << 12) | (n << 8) | (x << 4) | y
    }

    #[test]
    fn fixed_multiplier_fractional_integer_and_accumulate() {
        let mut c = Core::new(&[]);
        // SSF: 0.5 * 0.5 = 0.25
        c.r[1] = 0x4000_0000;
        c.r[2] = 0x4000_0000;
        assert!(c.compute(mult_word(0x78, 3, 1, 2)));
        assert_eq!(c.r[3], 0x2000_0000);
        // SSF: -1 * -1 overflows the 64-bit window: MV, MOS, and no saturation (VERIFIED)
        c.r[1] = 0x8000_0000;
        c.r[2] = 0x8000_0000;
        assert!(c.compute(mult_word(0x78, 3, 1, 2)));
        assert_eq!(c.r[3], 0x8000_0000);
        assert_eq!(c.sys.astat_extra[0] >> 6 & 0xF, 0b010);
        assert_ne!(c.sys.stky[0] & STKY_MOS, 0);
        // SSI: -3 * 7 = -21, MN
        c.r[1] = (-3i32) as u32;
        c.r[2] = 7;
        assert!(c.compute(mult_word(0x70, 3, 1, 2)));
        assert_eq!(c.r[3] as i32, -21);
        assert_eq!(c.sys.astat_extra[0] >> 6 & 0xF, 0b001);
        // MRF = SSF product, then Rn = MRF + product leaves MRF alone, MRF = 0
        c.r[1] = 0x4000_0000;
        c.r[2] = 0x2000_0000;
        assert!(c.compute(mult_word(0x78 | 4, 0, 1, 2)));
        let mrf = c.mr[0];
        assert!(c.compute(mult_word(0x80 | 0x38, 4, 1, 2)));
        assert_eq!(c.r[4], 0x2000_0000);
        assert_eq!(c.mr[0], mrf);
        assert!(c.compute(mult_word(0x14, 0, 0, 0)));
        assert_eq!(c.mr[0], 0);
        // only the signed x signed fractional format shifts the product left
        c.r[1] = 1;
        c.r[2] = 1;
        assert!(c.compute(mult_word(0x78 | 4, 0, 1, 2)));
        assert_eq!(c.mr[0], 2);
        assert!(c.compute(mult_word(0x68 | 4, 0, 1, 2))); // SUF
        assert_eq!(c.mr[0], 1);
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
        assert!(c.compute(mult_word(0x09, 3, 0, 0)));
        assert_eq!(c.r[3], 0x7FFF_FFFF);
        // MRF = SAT MRF (SF) stores the saturated maximum
        assert!(c.compute(mult_word(0x09 | 4, 0, 0, 0)));
        assert_eq!(c.mr[0], 0x7FFF_FFFF_FFFF_FFFF);
        // in range: unchanged, integer view in the register
        c.mr[0] = 0x1234;
        assert!(c.compute(mult_word(0x01, 4, 0, 0)));
        assert_eq!(c.r[4], 0x1234);
        // an out-of-range negative value in an unsigned format gives the maximum (VERIFIED)
        c.mr[0] = -(1i128 << 70);
        assert!(c.compute(mult_word(0x00, 4, 0, 0)));
        assert_eq!(c.r[4], 0xFFFF_FFFF);
        // R5 = RND MRF (SF): half rounds to even at bit 32
        c.mr[0] = 0x3 << 31; // 1.5 in MR1
        assert!(c.compute(mult_word(0x19, 5, 0, 0)));
        assert_eq!(c.r[5], 2);
        assert!(
            !c.compute(mult_word(0x13, 5, 0, 0)),
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
    fn windows_of_the_formats() {
        assert!(fits(-(1i128 << 31), false, false) && !fits(1i128 << 31, false, false));
        assert!(fits((1i128 << 64) - 1, true, true) && !fits(-1, true, true));
        assert_eq!(round_mr(0x1_8000_0000), 0x2_0000_0000);
        assert_eq!(round_mr(0x2_8000_0000), 0x2_0000_0000, "ties go to even");
    }
}
