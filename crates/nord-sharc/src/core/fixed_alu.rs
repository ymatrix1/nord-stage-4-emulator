//! Fixed-point ALU operations of [`Core`].
//!
//! VERIFIED against the CCES 2.12.1 ADSP-21489 simulator by `fixed_measured.rs`
//! (`docs/research/sharc-fixed/`, every pair of 12 operands through 26 operations, MODE1 ALUSAT
//! clear and set): AZ is a zero result and AN its sign; AV is a signed overflow and latches STKY
//! AOS (bit 2); AC is the carry out (for a subtraction of `x + !y + cin`, so `x - x` carries);
//! AS is set by `ABS` of a negative operand; compare, logic, `PASS`, `MIN`, `MAX` and `CLIP` clear
//! AV, AC and AS. With ALUSAT set an overflowing add, subtract, `ABS`, negate, increment or
//! decrement saturates to the largest or smallest value. `(x+y)/2` rounds half to even and its AC
//! is the carry of `x + y + 1` when the tie rounded up, of `x + y` otherwise.
use super::Core;

const STKY_AOS: u32 = 1 << 2;

impl Core {
    /// Writes `r` to `Rn` and sets the five fixed-point ALU flags.
    fn fixed_result(&mut self, n: usize, r: u32, ac: bool, av: bool, as_: bool) {
        self.r[n] = r;
        self.fx.clear(n);
        let f = &mut self.flags;
        (f.az, f.an, f.av, f.ac, f.as_) = (r == 0, (r as i32) < 0, av, ac, as_);
        // VERIFIED (census diff, 11 cases): a fixed-point ALU operation clears AI
        f.ai = false;
        if av {
            self.note_arith_exception(false, STKY_AOS);
        }
    }

    fn sat_result(&self, av: bool, exact: i64) -> Option<u32> {
        let limit = if exact > 0 { 0x7FFF_FFFF } else { 0x8000_0000 };
        (av && self.mode1 & super::float::MODE1_ALUSAT != 0).then_some(limit)
    }

    /// `Rn = Rx + Ry + cin` (`cin = 0` for a plain add).
    fn fixed_add(&mut self, n: usize, x: u32, y: u32, cin: u64) {
        let t = u64::from(x) + u64::from(y) + cin;
        let exact = i64::from(x as i32) + i64::from(y as i32) + cin as i64;
        let av = i32::try_from(exact).is_err();
        let r = self.sat_result(av, exact).unwrap_or(t as u32);
        self.fixed_result(n, r, t >> 32 != 0, av, false);
    }

    /// `Rn = Rx - Ry + cin - 1` (`cin = 1` for a plain subtract): `Rx + !Ry + cin`.
    fn fixed_sub(&mut self, n: usize, x: u32, y: u32, cin: u64) {
        let t = u64::from(x) + u64::from(!y) + cin;
        let exact = i64::from(x as i32) - i64::from(y as i32) + cin as i64 - 1;
        let av = i32::try_from(exact).is_err();
        let r = self.sat_result(av, exact).unwrap_or(t as u32);
        self.fixed_result(n, r, t >> 32 != 0, av, false);
    }

    /// Shifts the result of a compare into CACC (ASTAT bits 24..31, newest at bit 31).
    /// VERIFIED (simulator, 9 212 compare/other-op transitions): COMP and COMPU, fixed or float,
    /// shift in `Rx > Ry` (1 for a NaN operand); no other operation changes CACC.
    pub(super) fn shift_cacc(&mut self, greater: bool) {
        let extra = &mut self.sys.astat_extra[0];
        let cacc = (*extra >> 24 >> 1) | u32::from(greater) << 7;
        *extra = *extra & 0x00FF_FFFF | cacc << 24;
    }

    /// A result that only sets AZ and AN.
    fn fixed_plain(&mut self, n: usize, r: u32) {
        self.fixed_result(n, r, false, false, false);
    }

    /// Executes a fixed-point ALU opcode. Returns false for anything else.
    pub(super) fn fixed_alu(&mut self, op: u64, n: usize, rx: u32, ry: u32) -> bool {
        let ci = u64::from(self.flags.ac);
        match op {
            0x01 => self.fixed_add(n, rx, ry, 0),
            0x02 => self.fixed_sub(n, rx, ry, 1),
            0x05 => self.fixed_add(n, rx, ry, ci),
            0x06 => self.fixed_sub(n, rx, ry, ci),
            0x09 => {
                let a = i64::from(rx as i32) + i64::from(ry as i32);
                let rnd = i64::from(a & 1 == 1 && (a >> 1) & 1 == 1);
                let t = u64::from(rx) + u64::from(ry) + rnd as u64;
                self.fixed_result(n, ((a + rnd) >> 1) as u32, t >> 32 != 0, false, false);
            }
            0x0A | 0x0B => {
                let (less, greater) = if op == 0x0A {
                    ((rx as i32) < (ry as i32), (rx as i32) > (ry as i32))
                } else {
                    (rx < ry, rx > ry)
                };
                self.shift_cacc(greater);
                let f = &mut self.flags;
                (f.az, f.an, f.av, f.ac, f.as_) = (rx == ry, less, false, false, false);
                f.ai = false; // VERIFIED (census diff, 67 cases): a fixed compare clears AI
            }
            0x21 => self.fixed_plain(n, rx),
            0x22 => self.fixed_sub(n, 0, rx, 1),
            0x25 => self.fixed_add(n, rx, 0, ci),
            0x26 => self.fixed_sub(n, rx, 0, ci),
            0x29 => self.fixed_add(n, rx, 1, 0),
            0x2A => self.fixed_sub(n, rx, 1, 1),
            0x30 => {
                let exact = i64::from(rx as i32).abs();
                let av = exact >= 1 << 31;
                let r = self.sat_result(av, exact).unwrap_or(exact as u32);
                self.fixed_result(n, r, false, av, (rx as i32) < 0);
            }
            0x40 => self.fixed_plain(n, rx & ry),
            0x41 => self.fixed_plain(n, rx | ry),
            0x42 => self.fixed_plain(n, rx ^ ry),
            0x43 => self.fixed_plain(n, !rx),
            0x61 => self.fixed_plain(n, (rx as i32).min(ry as i32) as u32),
            0x62 => self.fixed_plain(n, (rx as i32).max(ry as i32) as u32),
            0x63 => {
                let lim = i64::from(ry as i32).abs();
                let r = i64::from(rx as i32).clamp(-lim, lim);
                self.fixed_plain(n, r as u32);
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(op: u64, n: u64, x: u64, y: u64) -> u64 {
        (op << 12) | (n << 8) | (x << 4) | y
    }

    #[test]
    fn add_sub_flags_carry_overflow_and_saturation() {
        let mut c = Core::new(&[]);
        c.r[1] = 0x7FFF_FFFF;
        c.r[2] = 1;
        assert!(c.compute(word(0x01, 3, 1, 2)));
        assert_eq!(c.r[3], 0x8000_0000);
        assert!(c.flags.av && c.flags.an && !c.flags.ac);
        assert_ne!(c.sys.stky[0] & STKY_AOS, 0, "AV latches AOS");
        c.mode1 = super::super::float::MODE1_ALUSAT;
        assert!(c.compute(word(0x01, 3, 1, 2)));
        assert_eq!(c.r[3], 0x7FFF_FFFF, "ALUSAT saturates");
        c.r[1] = 5;
        c.r[2] = 5;
        assert!(c.compute(word(0x02, 3, 1, 2)));
        assert!(c.flags.az && c.flags.ac, "x - x carries");
    }

    #[test]
    fn average_rounds_half_to_even_with_the_rounding_carry() {
        let mut c = Core::new(&[]);
        c.r[1] = 0;
        c.r[2] = 1;
        assert!(c.compute(word(0x09, 3, 1, 2)));
        assert_eq!(c.r[3], 0, "0.5 rounds to 0");
        c.r[2] = 0xFFFF_FFFF;
        assert!(c.compute(word(0x09, 3, 1, 2)));
        assert!(c.flags.az && c.flags.ac, "-0.5 rounds up to 0 with a carry");
        c.r[2] = 0x7FFF_FFFF;
        assert!(c.compute(word(0x09, 3, 1, 2)));
        assert_eq!(c.r[3], 0x4000_0000);
    }

    #[test]
    fn compares_shift_into_cacc() {
        let mut c = Core::new(&[]);
        c.r[1] = 5;
        c.r[2] = 3;
        assert!(c.compute(word(0x0A, 0, 1, 2))); // 5 > 3
        assert_eq!(c.astat(0) >> 24, 0x80);
        assert!(c.compute(word(0x0A, 0, 2, 1))); // 3 > 5 is false: the history moves down
        assert_eq!(c.astat(0) >> 24, 0x40);
        assert!(c.compute(word(0x0B, 0, 1, 2))); // unsigned
        assert_eq!(c.astat(0) >> 24, 0xA0);
        assert!(c.compute(word(0x41, 3, 1, 2))); // other operations leave it alone
        assert_eq!(c.astat(0) >> 24, 0xA0);
    }

    #[test]
    fn abs_neg_and_logic_flags() {
        let mut c = Core::new(&[]);
        c.r[1] = 0x8000_0000;
        assert!(c.compute(word(0x30, 3, 1, 1)));
        assert!(c.flags.av && c.flags.as_ && c.flags.an);
        c.r[1] = 0;
        assert!(c.compute(word(0x22, 3, 1, 1)));
        assert!(c.flags.az && c.flags.ac, "0 - 0 carries");
        c.r[1] = 0xF0;
        c.r[2] = 0x0F;
        assert!(c.compute(word(0x41, 3, 1, 2)));
        assert!(!c.flags.ac && !c.flags.av && !c.flags.as_);
        assert!(c.compute(word(0x0A, 0, 1, 2)));
        assert!(!c.flags.az && !c.flags.an);
    }

    #[test]
    fn a_fixed_alu_operation_clears_ai() {
        // census diff: `R12 = R12 AND R13` and `PASS` with a random ASTAT that had AI set
        let mut c = Core::new(&[]);
        c.flags.ai = true;
        (c.r[1], c.r[2]) = (6, 3);
        assert!(c.compute((0x40 << 12) | (3 << 8) | (1 << 4) | 2)); // R3 = R1 AND R2
        assert_eq!(c.r[3], 2);
        assert!(!c.flags.ai);
    }

    #[test]
    fn a_fixed_compare_clears_ai() {
        // census diff: `COMP(R0, R1)` with AI set from an earlier float operation
        let mut c = Core::new(&[]);
        c.flags.ai = true;
        (c.r[1], c.r[2]) = (6, 3);
        assert!(c.compute((0x0A << 12) | (1 << 4) | 2));
        assert!(!c.flags.ai);
    }
}
