//! ALU, float and short (16-bit) compute operations of [`Core`].
use super::Core;
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
    /// Executes a 23-bit compute field. Returns false when the operation is not modelled.
    pub(super) fn compute(&mut self, c: u64) -> bool {
        if c == 0 {
            return true;
        }
        if bits(c, 22, 22) == 1 {
            return self.multifunction(c);
        }
        let cu = bits(c, 22, 20);
        let op = bits(c, 19, 12);
        let (n, x, y) = (
            bits(c, 11, 8) as usize,
            bits(c, 7, 4) as usize,
            bits(c, 3, 0) as usize,
        );
        let (rx, ry) = (self.r[x], self.r[y]);
        match (cu, op) {
            (0, op) if op < 0x80 && self.fixed_alu(op, n, rx, ry) => {}
            (1, 0x30) => self.float_mul(n, x, y),
            (0, op) if op >= 0x80 && self.float_alu(op, n, x, y) => {}
            (0, op) if matches!(op >> 4, 0x7 | 0xF) && self.dual_add_sub(c) => {}
            (1, op) if self.fixed_mult(op, n, x, y) => {}
            (2, op) if self.shifter(op, n, x, y) => {}
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
        assert!(!c.compute(0x40_3000), "there is no MR3 transfer");
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
