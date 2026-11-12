//! ALU, float and short (16-bit) compute operations of [`Core`].
use super::{f32_of, Core};
use crate::visa::bits;

/// Compute unit and opcode of a Type 2c short op, and whether it is binary (`Rn = Rn op Rx`) or
/// takes only `Rx` as input.
struct ShortOp {
    unit: u64,
    opcode: u64,
    binary: bool,
}

/// The 16 short-compute operations (OP field, bits 11..8) as their full-compute equivalents.
fn short_op(op: u64) -> Option<ShortOp> {
    let (unit, opcode, binary) = match op {
        0 => (0, 0x01, true),   // Rn = Rn + Rx
        1 => (0, 0x02, true),   // Rn = Rn - Rx
        2 => (0, 0x21, false),  // Rn = PASS Rx
        3 => (0, 0x0A, true),   // COMP(Rn, Rx)
        4 => (0, 0x43, false),  // Rn = NOT Rx
        5 => (0, 0x29, false),  // Rn = Rx + 1
        6 => (0, 0x2A, false),  // Rn = Rx - 1
        8 => (0, 0x81, true),   // Fn = Fn + Fx
        9 => (0, 0x82, true),   // Fn = Fn - Fx
        10 => (0, 0xCA, false), // Fn = FLOAT Rx
        11 => (0, 0x8A, true),  // COMP(Fn, Fx)
        12 => (0, 0x40, true),  // Rn = Rn AND Rx
        13 => (0, 0x41, true),  // Rn = Rn OR Rx
        14 => (0, 0x42, true),  // Rn = Rn XOR Rx
        15 => (1, 0x30, true),  // Fn = Fn * Fx
        _ => return None,
    };
    Some(ShortOp {
        unit,
        opcode,
        binary,
    })
}

impl Core {
    pub(super) fn set_arith_flags(&mut self, result: u32, float: bool) {
        self.flags.az = if float {
            f32_of(result) == 0.0
        } else {
            result == 0
        };
        self.flags.an = if float {
            f32_of(result) < 0.0
        } else {
            (result as i32) < 0
        };
    }

    /// Writes a fixed-point result to `Rn` and updates AZ and AN.
    #[inline]
    pub(super) fn set_fixed(&mut self, n: usize, v: u32) {
        self.r[n] = v;
        self.set_arith_flags(v, false);
    }

    /// Writes a float result to `Fn` and updates AZ and AN.
    #[inline]
    pub(super) fn set_float(&mut self, n: usize, v: f32) {
        let bits = v.to_bits();
        self.r[n] = bits;
        self.set_arith_flags(bits, true);
    }

    /// Executes a 23-bit compute field. Returns false when the operation is not modelled.
    pub(super) fn compute(&mut self, c: u64) -> bool {
        if c == 0 {
            return true;
        }
        if bits(c, 22, 22) == 1 {
            return false;
        }
        let cu = bits(c, 22, 20);
        let op = bits(c, 19, 12);
        let (n, x, y) = (
            bits(c, 11, 8) as usize,
            bits(c, 7, 4) as usize,
            bits(c, 3, 0) as usize,
        );
        let (rx, ry) = (self.r[x], self.r[y]);
        let (fx, fy) = (f32_of(rx), f32_of(ry));
        let scale = || 2f32.powi(ry as i32);
        match (cu, op) {
            (0, 0x01) => {
                self.flags.ac = rx.overflowing_add(ry).1;
                self.flags.av = (rx as i32).checked_add(ry as i32).is_none();
                self.set_fixed(n, rx.wrapping_add(ry));
            }
            (0, 0x02) => {
                self.flags.av = (rx as i32).checked_sub(ry as i32).is_none();
                self.set_fixed(n, rx.wrapping_sub(ry));
            }
            (0, 0x05) => {
                let carry_in = u32::from(self.flags.ac);
                self.flags.ac = (u64::from(rx) + u64::from(ry) + u64::from(carry_in)) > 0xFFFF_FFFF;
                self.set_fixed(n, rx.wrapping_add(ry).wrapping_add(carry_in));
            }
            (0, 0x06) => {
                let r = rx
                    .wrapping_sub(ry)
                    .wrapping_add(u32::from(self.flags.ac))
                    .wrapping_sub(1);
                self.set_fixed(n, r);
            }
            (0, 0x09) => self.set_fixed(
                n,
                ((i64::from(rx as i32) + i64::from(ry as i32)) >> 1) as u32,
            ),
            (0, 0x0A) => {
                self.flags.az = rx == ry;
                self.flags.an = (rx as i32) < (ry as i32);
            }
            (0, 0x0B) => {
                self.flags.az = rx == ry;
                self.flags.an = rx < ry;
            }
            (0, 0x21) => self.set_fixed(n, rx),
            (0, 0x22) => self.set_fixed(n, rx.wrapping_neg()),
            (0, 0x25) => self.set_fixed(n, rx.wrapping_add(u32::from(self.flags.ac))),
            (0, 0x26) => {
                self.set_fixed(n, rx.wrapping_add(u32::from(self.flags.ac)).wrapping_sub(1));
            }
            (0, 0x29) => self.set_fixed(n, rx.wrapping_add(1)),
            (0, 0x2A) => self.set_fixed(n, rx.wrapping_sub(1)),
            (0, 0x30) => self.set_fixed(n, (rx as i32).wrapping_abs() as u32),
            (0, 0x40) => self.set_fixed(n, rx & ry),
            (0, 0x41) => self.set_fixed(n, rx | ry),
            (0, 0x42) => self.set_fixed(n, rx ^ ry),
            (0, 0x43) => self.set_fixed(n, !rx),
            (0, 0x61) => self.set_fixed(n, (rx as i32).min(ry as i32) as u32),
            (0, 0x62) => self.set_fixed(n, (rx as i32).max(ry as i32) as u32),
            (0, 0x63) => {
                let lim = (ry as i32).wrapping_abs();
                self.set_fixed(n, (rx as i32).clamp(lim.wrapping_neg(), lim) as u32);
            }
            (0, 0x81) => self.set_float(n, fx + fy),
            (0, 0x82) => self.set_float(n, fx - fy),
            (0, 0x89) => self.set_float(n, (fx + fy) / 2.0),
            (0, 0x8A) => {
                self.flags.az = fx == fy;
                self.flags.an = fx < fy;
            }
            (0, 0x91) => self.set_float(n, (fx + fy).abs()),
            (0, 0x92) => self.set_float(n, (fx - fy).abs()),
            (0, 0xA1) | (0, 0xA5) => self.set_float(n, fx),
            (0, 0xA2) => self.set_float(n, -fx),
            (0, 0xB0) => self.set_float(n, fx.abs()),
            (0, 0xBD) => self.set_float(n, fx * scale()),
            (0, 0xC1) => {
                if fx == 0.0 || !fx.is_finite() {
                    return false;
                }
                self.set_fixed(n, (((rx >> 23) & 0xFF) as i32 - 127) as u32);
            }
            (0, 0xC9) => self.set_fixed(n, fx.round_ties_even() as i32 as u32),
            (0, 0xCA) => self.set_float(n, rx as i32 as f32),
            (0, 0xCD) => self.set_fixed(n, fx.trunc() as i32 as u32),
            (0, 0xD9) => self.set_fixed(n, (fx * scale()).round_ties_even() as i32 as u32),
            (0, 0xDA) => self.set_float(n, rx as i32 as f32 * scale()),
            (0, 0xDD) => self.set_fixed(n, (fx * scale()).trunc() as i32 as u32),
            (0, 0xE0) => self.set_float(n, fx.copysign(fy)),
            (0, 0xE1) => self.set_float(n, fx.min(fy)),
            (0, 0xE2) => self.set_float(n, fx.max(fy)),
            (0, 0xE3) => self.set_float(n, fx.clamp(-fy.abs(), fy.abs())),
            (1, 0x30) => self.set_float(n, fx * fy),
            (1, op) if self.fixed_mult(op, n, x, y) => {}
            _ => return false,
        }
        true
    }

    /// Short (16-bit) compute of Type 2c: OP bits 11..8, Rn 7..4, Rx 3..0. Each op is the
    /// full compute with the same name, so flags and results match the long forms.
    pub(super) fn short_compute(&mut self, c: u64) -> bool {
        let (n, x) = (bits(c, 7, 4), bits(c, 3, 0));
        let Some(op) = short_op(bits(c, 11, 8)) else {
            return false;
        };
        let (xs, ys) = if op.binary { (n, x) } else { (x, 0) };
        self.compute((op.unit << 20) | (op.opcode << 12) | (n << 8) | (xs << 4) | ys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_compute_and_flags() {
        let mut c = Core::new(&[]);
        c.r[1] = 1.5f32.to_bits();
        c.r[2] = 2.0f32.to_bits();
        // F3 = F1 * F2 via the Type 2b compute 0x130 + regs; use direct compute() calls
        assert!(c.compute((1 << 20) | (0x30 << 12) | (3 << 8) | (1 << 4) | 2));
        assert_eq!(f32::from_bits(c.r[3]), 3.0);
        assert!(c.compute((0x81 << 12) | (4 << 8) | (1 << 4) | 2));
        assert_eq!(f32::from_bits(c.r[4]), 3.5);
        assert!(c.compute((0x82 << 12) | (5 << 8) | (2 << 4) | 2));
        assert!(c.flags.az && !c.flags.an);
        // fixed-point and compare operations
        c.r[1] = 7;
        c.r[2] = (-3i32) as u32;
        assert!(c.compute((0x62 << 12) | (6 << 8) | (1 << 4) | 2));
        assert_eq!(c.r[6], 7);
        assert!(c.compute((0x61 << 12) | (6 << 8) | (1 << 4) | 2));
        assert_eq!(c.r[6] as i32, -3);
        assert!(c.compute((0x0A << 12) | (1 << 4) | 2));
        assert!(!c.flags.az && !c.flags.an);
        assert!(c.compute((0x63 << 12) | (6 << 8) | (1 << 4) | 2));
        assert_eq!(c.r[6], 3, "CLIP 7 by 3 saturates at the magnitude of Ry");
        assert!(c.compute((0xBD << 12) | (7 << 8) | (3 << 4) | 1)); // SCALB F3 by R1 (7)
        assert_eq!(f32::from_bits(c.r[7]), 3.0 * 128.0);
        assert!(!c.compute(0x40_0000), "multifunction is not modelled yet");
        assert!(
            !c.compute((0xC4 << 12) | (1 << 8)),
            "RECIPS seed table is unknown"
        );
    }

    #[test]
    fn short_pass_sets_zero_flag_for_mailbox_polls() {
        let mut c = Core::new(&[]);
        c.r[0] = 5;
        assert!(c.short_compute(0x200));
        assert!(!c.flags.az);
        c.r[0] = 0;
        assert!(c.short_compute(0x200));
        assert!(c.flags.az);
    }

    #[test]
    fn short_ops_match_the_long_forms() {
        let mut c = Core::new(&[]);
        (c.r[1], c.r[2]) = (10, 3);
        assert!(c.short_compute(0x112)); // R1 = R1 - R2
        assert_eq!(c.r[1], 7);
        assert!(c.short_compute(0x512)); // R1 = R2 + 1
        assert_eq!(c.r[1], 4);
        (c.r[1], c.r[2]) = (1.5f32.to_bits(), 2.0f32.to_bits());
        assert!(c.short_compute(0xF12)); // F1 = F1 * F2
        assert_eq!(f32::from_bits(c.r[1]), 3.0);
        assert!(!c.short_compute(0x712), "op 7 is not a short compute");
    }
}
