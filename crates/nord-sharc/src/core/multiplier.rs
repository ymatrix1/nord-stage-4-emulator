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
            match op {
                0x14 => self.mr[0] = 0,
                0x16 => self.mr[1] = 0,
                _ => return false,
            }
            return true;
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
