//! Multifunction compute of [`Core`]: a multiplier and an ALU operation (or two ALU operations)
//! in one instruction (programming reference rev 2.4, "Multifunction Computations").
//!
//! Both halves read the register values from before the instruction, so the ALU half is
//! evaluated first on the original registers, the state is restored, the multiplier half runs on
//! the original registers and only then the ALU result is written.
use super::{Core, Flags};
use crate::visa::bits;

/// Multiplier op codes of [`Core::fixed_mult`] used by the fixed-point multifunction forms:
/// `Rm = Rxm * Rym (SSFR)`, `MRF = MRF + ... (SSF)`, `Rm = MRF + ... (SSFR)`,
/// `MRF = MRF - ... (SSF)`, `Rm = MRF - ... (SSFR)`.
const FIXED_MULT_OPS: [u64; 5] = [0x79, 0xBC, 0xB9, 0xFC, 0xF9];
/// ALU op codes of the fixed-point forms: add, subtract, average.
const FIXED_ALU_OPS: [u64; 3] = [0x01, 0x02, 0x09];
/// ALU op codes of the float forms: add, subtract, `FLOAT`, `FIX`, average, `ABS`, `MAX`, `MIN`.
const FLOAT_ALU_OPS: [u64; 8] = [0x81, 0x82, 0xDA, 0xD9, 0x89, 0xB0, 0xE2, 0xE1];

impl Core {
    /// Runs `alu` (a compute word writing `Ra`) and `mult` (a closure) from the same original
    /// register values. Returns false, with the state restored, when either half is not modelled.
    fn parallel(&mut self, ra: usize, alu: u64, mult: impl Fn(&mut Core) -> bool) -> bool {
        let (orig, flags, fx) = (self.r, self.flags, self.fx);
        let alu_ok = self.compute(alu);
        let (alu_value, alu_flags, alu_fx) = (self.r[ra], self.flags, self.fx);
        self.r = orig;
        self.fx = fx;
        self.flags = flags;
        let mr = self.mr;
        if !alu_ok || !mult(self) {
            self.r = orig;
            self.fx = fx;
            self.flags = flags;
            self.mr = mr;
            return false;
        }
        self.r[ra] = alu_value;
        self.fx.take(&alu_fx, ra);
        self.flags = alu_flags;
        true
    }

    /// Multiplier plus ALU forms (compute bit 22 set), Tables 12-12 and 12-13.
    pub(super) fn multifunction(&mut self, c: u64) -> bool {
        // multifunction op6 0 or 1 (bits 21..17 clear; no multiplier+ALU form uses them): the
        // multiplier result register transfers `Rn = MRxF`, `MRxF = Rn`
        if bits(c, 21, 17) == 0 {
            return self.mr_transfer(bits(c, 19, 12), bits(c, 11, 8) as usize);
        }
        let (xm, ym) = (bits(c, 7, 6) as usize, 4 + bits(c, 5, 4) as usize);
        let (xa, ya) = (8 + bits(c, 3, 2), 12 + bits(c, 1, 0));
        let (rm, ra) = (bits(c, 15, 12) as usize, bits(c, 11, 8));
        let alu_word = |op: u64, a: u64| (op << 12) | (a << 8) | (xa << 4) | ya;
        if bits(c, 21, 21) == 1 {
            // Rm = Rxm * Rym, Ra = Rxa + Rya, Rs = Rxa - Rya (float when bit 20 is set)
            let float = bits(c, 20, 20) == 1;
            let rs = bits(c, 19, 16);
            let (add, sub) = if float { (0x81, 0x82) } else { (0x01, 0x02) };
            let (orig, flags, fx) = (self.r, self.flags, self.fx);
            let ok_add = self.compute(alu_word(add, ra));
            let (sum, sum_fx) = (self.r[ra as usize], self.fx);
            self.r = orig;
            self.fx = fx;
            let ok_sub = self.compute(alu_word(sub, rs));
            let (diff, diff_fx) = (self.r[rs as usize], self.fx);
            self.r = orig;
            self.fx = fx;
            let mult_ok = if float {
                self.compute(
                    (1 << 20) | (0x30 << 12) | ((rm as u64) << 8) | ((xm as u64) << 4) | ym as u64,
                )
            } else {
                self.fixed_mult(FIXED_MULT_OPS[0], rm, xm, ym)
            };
            if !(ok_add && ok_sub && mult_ok) {
                self.r = orig;
                self.fx = fx;
                self.flags = flags;
                return false;
            }
            self.r[ra as usize] = sum;
            self.r[rs as usize] = diff;
            self.fx.take(&sum_fx, ra as usize);
            self.fx.take(&diff_fx, rs as usize);
            return true;
        }
        let op6 = bits(c, 21, 16);
        let group = op6 >> 2;
        let low = (op6 & 3) as usize;
        if op6 >> 3 == 3 {
            // float multiplier with a float ALU operation (011xxx)
            let alu_op = FLOAT_ALU_OPS[(op6 & 7) as usize];
            let word =
                (1 << 20) | (0x30 << 12) | ((rm as u64) << 8) | ((xm as u64) << 4) | ym as u64;
            return self.parallel(ra as usize, alu_word(alu_op, ra), |core| core.compute(word));
        }
        let (Some(&mult_op), Some(&alu_op)) = (
            group
                .checked_sub(1)
                .and_then(|g| FIXED_MULT_OPS.get(g as usize)),
            FIXED_ALU_OPS.get(low),
        ) else {
            return false;
        };
        self.parallel(ra as usize, alu_word(alu_op, ra), |core| {
            core.fixed_mult(mult_op, rm, xm, ym)
        })
    }

    /// `Ra = Rx + Ry, Rs = Rx - Ry` (fixed or float), compute unit 0 with opcode `0x7?`/`0xF?`.
    pub(super) fn dual_add_sub(&mut self, c: u64) -> bool {
        let float = bits(c, 19, 16) == 0xF;
        let (rs, ra) = (bits(c, 15, 12), bits(c, 11, 8));
        let (x, y) = (bits(c, 7, 4), bits(c, 3, 0));
        let (add, sub) = if float { (0x81, 0x82) } else { (0x01, 0x02) };
        let word = |op: u64, n: u64| (op << 12) | (n << 8) | (x << 4) | y;
        let (orig, flags, fx) = (self.r, self.flags, self.fx);
        let ok = self.compute(word(add, ra));
        let (sum, sum_flags, sum_fx) = (self.r[ra as usize], self.flags, self.fx);
        self.r = orig;
        self.fx = fx;
        let ok = ok && self.compute(word(sub, rs));
        let (diff, diff_fx, diff_flags) = (self.r[rs as usize], self.fx, self.flags);
        self.r = orig;
        self.fx = fx;
        if !ok {
            self.flags = flags;
            return false;
        }
        self.r[ra as usize] = sum;
        self.r[rs as usize] = diff;
        self.fx.take(&sum_fx, ra as usize);
        self.fx.take(&diff_fx, rs as usize);
        // VERIFIED (`simd_measured.rs`, 64 operand pairs of the fixed dual form on PEx, ASTAT bits
        // AZ AV AN AC): the flags are the OR of the sum's and the difference's. ASSUMED for AS and
        // AI and for the float form (AZ AN AV AC checked there too; the SIMD probe covers both).
        self.flags = Flags {
            az: sum_flags.az || diff_flags.az,
            an: sum_flags.an || diff_flags.an,
            av: sum_flags.av || diff_flags.av,
            ac: sum_flags.ac || diff_flags.ac,
            as_: sum_flags.as_ || diff_flags.as_,
            ai: sum_flags.ai || diff_flags.ai,
            ..sum_flags
        };
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A multifunction compute word.
    fn word(op6: u64, rm: u64, ra: u64, xm: u64, ym: u64, xa: u64, ya: u64) -> u64 {
        (1 << 22) | (op6 << 16) | (rm << 12) | (ra << 8) | (xm << 6) | (ym << 4) | (xa << 2) | ya
    }

    fn f(x: f32) -> u32 {
        x.to_bits()
    }

    #[test]
    fn float_multiply_with_add_reads_the_original_registers() {
        // F8 = F1 * F4, F9 = F8 + F12 (Ra = F9 reads the old F8)
        let mut c = Core::new(&[]);
        (c.r[1], c.r[4]) = (f(3.0), f(2.0));
        (c.r[8], c.r[12]) = (f(10.0), f(0.5));
        assert!(c.compute(word(0b011000, 8, 9, 1, 0, 0, 0)));
        assert_eq!(f32::from_bits(c.r[8]), 6.0);
        assert_eq!(f32::from_bits(c.r[9]), 10.5);
    }

    #[test]
    fn float_alu_variants_with_a_multiply() {
        let mut c = Core::new(&[]);
        (c.r[0], c.r[4], c.r[8], c.r[12]) = (f(2.0), f(4.0), f(7.0), f(5.0));
        // Fm = F0 * F4, Fa = MAX(F8, F12)
        assert!(c.compute(word(0b011110, 1, 2, 0, 0, 0, 0)));
        assert_eq!((f32::from_bits(c.r[1]), f32::from_bits(c.r[2])), (8.0, 7.0));
        // Fa = ABS F8
        c.r[8] = f(-3.0);
        assert!(c.compute(word(0b011101, 1, 2, 0, 0, 0, 0)));
        assert_eq!(f32::from_bits(c.r[2]), 3.0);
    }

    #[test]
    fn fixed_multiply_accumulate_with_an_add() {
        // MRF = MRF + R0 * R4 (SSF), R9 = R8 + R12
        let mut c = Core::new(&[]);
        (c.r[0], c.r[4], c.r[8], c.r[12]) = (0x4000_0000, 0x4000_0000, 5, 6);
        assert!(c.compute(word(0b001000, 0, 9, 0, 0, 0, 0)));
        assert_eq!(c.r[9], 11);
        assert_eq!(c.mr[0] >> 32, 0x2000_0000, "0.5 * 0.5 accumulated in MRF");
        // Rm = Rxm * Rym (SSFR), Ra = Rxa - Rya
        assert!(c.compute(word(0b000101, 1, 2, 0, 0, 0, 0)));
        assert_eq!((c.r[1], c.r[2]), (0x2000_0000, (5u32).wrapping_sub(6)));
        // opcode groups that do not exist are not modelled
        assert!(
            !c.compute(word(0b000000, 3, 2, 0, 0, 0, 0)),
            "op8 3 is no MR transfer"
        );
    }

    #[test]
    fn triple_operation_form_and_dual_add_subtract() {
        // Fm = F0 * F4, Fa = F8 + F12, Fs = F8 - F12 (bit 21 set, bit 20 = float, Rs = 3)
        let mut c = Core::new(&[]);
        (c.r[0], c.r[4], c.r[8], c.r[12]) = (f(2.0), f(3.0), f(9.0), f(4.0));
        let triple = (1 << 22) | (1 << 21) | (1 << 20) | (3 << 16) | (5 << 12) | (6 << 8);
        assert!(c.compute(triple));
        assert_eq!(f32::from_bits(c.r[5]), 6.0);
        assert_eq!(f32::from_bits(c.r[6]), 13.0);
        assert_eq!(f32::from_bits(c.r[3]), 5.0);
        // dual add/subtract: top nibble 7 (fixed): R2 = R0 - R1 in bits 15..12, R3 = R0 + R1
        let mut c = Core::new(&[]);
        (c.r[0], c.r[1]) = (10, 3);
        let dual = (0x7 << 16) | (2 << 12) | (3 << 8) | 1;
        assert!(c.compute(dual));
        assert_eq!((c.r[2], c.r[3]), (7, 13));
    }

    #[test]
    fn dual_add_sub_flags_are_the_or_of_both_results() {
        // R4 = R9 + R13, R8 = R9 - R13 with 0x3FC00000 and 0x40000000: the sum is positive, the
        // difference negative -> AN; measured on the simulator (`simd_measured.rs`)
        let mut c = Core::new(&[]);
        (c.r[9], c.r[13]) = (0x3FC0_0000, 0x4000_0000);
        assert!(c.compute((0x7 << 16) | (8 << 12) | (4 << 8) | (9 << 4) | 13));
        assert_eq!((c.r[4], c.r[8]), (0x7FC0_0000, 0xFFC0_0000));
        assert!(c.flags.an);
    }
}
