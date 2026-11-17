//! Strict state comparison of two cores for the AOT differential runs, and the test oracle that
//! runs lowered ops the way the generated code does.
use super::Core;

impl Core {
    /// First difference between the complete architectural state of `self` and `other`,
    /// including what `compare_full_state` cannot see: PEy flags and multiplier results, the
    /// effective float extensions, ASTAT extras, STKY, USTAT and the interrupt latches.
    pub fn state_mismatch(&self, other: &Core) -> Option<String> {
        fn ext(c: &Core, y: bool) -> [u64; 16] {
            let (r, fx) = if y { (&c.s, &c.fx_y) } else { (&c.r, &c.fx) };
            std::array::from_fn(|n| {
                let lo = if fx.tag[n] == r[n] { fx.lo[n] } else { 0 };
                u64::from(r[n]) << 8 | u64::from(lo)
            })
        }
        macro_rules! same {
            ($what:literal, $a:expr, $b:expr) => {
                if $a != $b {
                    return Some(format!("{}: {:?} vs {:?}", $what, $a, $b));
                }
            };
        }
        same!("pc", self.pc, other.pc);
        same!("steps", self.steps, other.steps);
        same!("r", self.r, other.r);
        same!("s", self.s, other.s);
        same!("i", self.i, other.i);
        same!("m", self.m, other.m);
        same!("l", self.l, other.l);
        same!("b", self.b, other.b);
        same!("mr", self.mr, other.mr);
        same!("mr_y", self.mr_y, other.mr_y);
        same!("flags", self.flags, other.flags);
        same!("flags_y", self.flags_y, other.flags_y);
        same!("mode1", self.mode1, other.mode1);
        same!("mode2", self.mode2, other.mode2);
        same!("ustat", self.ustat, other.ustat);
        same!("f40 x", ext(self, false), ext(other, false));
        same!("f40 y", ext(self, true), ext(other, true));
        same!("astat_extra", self.sys.astat_extra, other.sys.astat_extra);
        same!("stky", self.sys.stky, other.sys.stky);
        same!("irptl", self.irq.irptl, other.irq.irptl);
        same!("lirptl", self.irq.lirptl, other.irq.lirptl);
        same!("loops", self.loops.len(), other.loops.len());
        same!("call depth", self.call_stack.len(), other.call_stack.len());
        None
    }
}

#[cfg(test)]
impl Core {
    /// Test oracle for the lowering: runs `ops` PE-batched exactly like the generated code.
    pub(super) fn run_lowered(&mut self, ops: &[super::aot_lower::LoweredOp]) {
        use super::aot_lower::LoweredOp;
        for _ in 0..2 {
            for &op in ops {
                match op {
                    LoweredOp::Compute(c) => self.aot_op(c),
                    LoweredOp::LoadImm { reg, value } => self.write_ureg(reg, value),
                    LoweredOp::RegMove { src, dst, compute } => {
                        let m = self.aot_move_src(src);
                        if let Some(c) = compute {
                            self.aot_op(c);
                        }
                        self.aot_move_dst(dst, m);
                    }
                    LoweredOp::Shift { op, n, x, ry } => self.aot_shift(op, n, x, ry),
                }
            }
            self.aot_swap_pe();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::support::core_with;
    use super::*;

    /// A block mixing every lowered form: immediates, float add/sub/mul/clip/copysign/recips/
    /// FLOAT, fixed add, `Rn = Rm, compute` moves, a shift immediate, a multifunction and NOP.
    fn mixed_program() -> Vec<u16> {
        vec![
            0x0f0e, 0x3eb0, 0xf34d, // R14 = 0x3eb0f34d
            0x0180, 0x1ecc, // R14 = R12 + R12 (fixed ALU, 32-bit form)
            0x018d, 0xa4ce, // F4 = FLOAT R12 BY R14
            0x0188, 0x2be4, // F11 = F14 - F4
            0x018e, 0x3bbe, // F11 = CLIP F11 BY F14
            0xc8be, // F11 = F11 + F14 (16-bit form)
            0x0193, 0x0ebe, // F14 = F11 * F14
            0x703e, 0x030c, 0x4040, // R6 = R0, F0 = RECIPS F4
            0x01d9, 0x602e, // F6 = F0 * F6, F0 = F11 - F14
            0x703f, 0x863f, // R12 = R3
            0x023e, 0x0032, 0x1fa0, // R10 = BTGL R0 BY 31
            0x018e, 0x000a, // F0 = F0 COPYSIGN F10
            0x0001, // NOP
        ]
    }

    fn seeded(c: &mut Core, seed: u32) {
        let mut x = seed.wrapping_mul(0x9E37_79B9) | 1;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x
        };
        for k in 0..16 {
            let v = next();
            // keep exponents in a normal range so most ops produce ordinary floats
            c.r[k] = (v & 0x807F_FFFF) | ((120 + (next() % 16)) << 23);
            c.s[k] = (next() & 0x807F_FFFF) | ((118 + (next() % 20)) << 23);
        }
        c.mode1 |= 1 << 21;
    }

    #[test]
    fn lowered_ops_match_the_interpreter_on_random_simd_state() {
        let program = mixed_program();
        let probe = core_with(&program);
        let last = probe.pc + program.len() as u32 - 1;
        let low = match probe.lower_block(probe.pc, last) {
            Ok(l) => l,
            Err(e) => panic!("lowering failed: {e}"),
        };
        let ops = match &low.items[..] {
            [super::super::aot_lower::Item::Batch(ops)] => ops.clone(),
            other => panic!("{other:?}"),
        };
        for seed in 1..40 {
            let mut interp = core_with(&program);
            let mut aot = core_with(&program);
            seeded(&mut interp, seed);
            seeded(&mut aot, seed);
            assert_eq!(interp.run(u64::from(low.steps)), None);
            aot.run_lowered(&ops);
            aot.steps += u64::from(low.steps);
            aot.pc = low.exit;
            assert_eq!(interp.state_mismatch(&aot), None, "seed {seed}");
        }
    }

    #[test]
    fn state_mismatch_sees_pey_flags_and_float_extension() {
        let mut a = Core::new(&[]);
        let b = Core::new(&[]);
        assert_eq!(a.state_mismatch(&b), None);
        a.flags_y.az = true;
        assert!(a.state_mismatch(&b).unwrap().starts_with("flags_y"));
        a.flags_y.az = false;
        a.fx.tag[3] = a.r[3];
        a.fx.lo[3] = 0x80;
        assert!(a.state_mismatch(&b).unwrap().starts_with("f40 x"));
    }
}
