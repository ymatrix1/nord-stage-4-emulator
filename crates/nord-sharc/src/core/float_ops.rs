//! Float compute operations of [`Core`]: the glue between the register file and the arithmetic
//! of `float.rs` / `float_misc.rs`.
//!
//! A register holds 40 bits, but the file stores 32 (`r`, `s`). The extra 8 bits of a float result
//! live in [`ExtFile`] next to the register, tagged with the 32-bit value they belong to, so any
//! other write to the register (a load, a fixed-point result, a move) invalidates them without
//! every writer having to know about the extension. Storing a register to memory takes the upper
//! 32 bits, i.e. truncates, as the hardware does.
use super::float::{self, Mode, Out};
use super::float_conv as conv;
use super::float_misc as misc;
use super::Core;

/// Low 8 bits of the 40-bit float registers, valid while the register still holds `tag`.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct ExtFile {
    tag: [u32; 16],
    lo: [u8; 16],
}

impl ExtFile {
    /// Forgets the extension of register `n` (it was overwritten by a non-float write).
    pub fn clear(&mut self, n: usize) {
        self.lo[n] = 0;
    }

    /// Takes the extension of register `n` from another snapshot of the file.
    pub fn take(&mut self, from: &ExtFile, n: usize) {
        (self.tag[n], self.lo[n]) = (from.tag[n], from.lo[n]);
    }
}

/// The captured source of a register transfer (see [`Core::capture_move`]).
pub(super) struct MoveSrc {
    v: u32,
    ext: Option<(u32, u8)>,
    comp: u32,
    comp_ext: Option<(u32, u8)>,
}

impl Core {
    /// The 40-bit float in register `n`.
    pub(super) fn f40(&self, n: usize) -> u64 {
        let lo = if self.fx.tag[n] == self.r[n] {
            self.fx.lo[n]
        } else {
            0
        };
        (u64::from(self.r[n]) << 8) | u64::from(lo)
    }

    /// Extension of the data register (R: code 0..15, S: code 0x50..0x5F) `code`, if it is one.
    fn ext_slot(&self, code: u8) -> Option<(u32, u8)> {
        let n = usize::from(code & 15);
        match code >> 4 {
            0 => Some((self.fx.tag[n], self.fx.lo[n])),
            5 => Some((self.fx_y.tag[n], self.fx_y.lo[n])),
            _ => None,
        }
    }

    fn put_ext_slot(&mut self, code: u8, slot: (u32, u8)) {
        let n = usize::from(code & 15);
        match code >> 4 {
            0 => (self.fx.tag[n], self.fx.lo[n]) = slot,
            5 => (self.fx_y.tag[n], self.fx_y.lo[n]) = slot,
            _ => {}
        }
    }

    /// `Ureg = Ureg` transfer. VERIFIED on the simulator (`docs/research/sharc-round/`,
    /// `sharc-flags/`): a move between data registers, R or S, keeps the 40-bit extension; any other
    /// destination drops it.
    #[cfg(test)]
    pub(super) fn move_ureg(&mut self, dst: u8, src: u8) {
        let cap = self.capture_move(src);
        self.apply_move(dst, src, &cap);
    }

    /// Reads the source of a register transfer: the value, its extension, and the value of the
    /// PEy complement (read together because a compute in the same instruction sees the old
    /// registers).
    pub(super) fn capture_move(&self, src: u8) -> MoveSrc {
        let n = usize::from(src & 15);
        let (comp, comp_ext) = match src >> 4 {
            0 => (self.s[n], self.ext_slot(0x50 | src)),
            5 => (self.r[n], self.ext_slot(src & 15)),
            // ASSUMED for STKY (ASTAT measured): the complement is STKYy
            7 if src == 0x76 || src == 0x78 => (self.read_ureg(src + 1), None),
            _ => (self.read_ureg(src), None),
        };
        MoveSrc {
            v: self.read_ureg(src),
            ext: self.ext_slot(src),
            comp,
            comp_ext,
        }
    }

    /// Writes a captured transfer to `dst`. In SIMD mode the PEy side moves too:
    ///
    /// VERIFIED (`simd_measured.rs`, census diff): `Rn = Rm` also copies `Sm` to `Sn`, `Sn = Rm`
    /// also copies `Sm` to `Rn` (every data register has an opposite-element twin), `Rn = ASTAT`
    /// loads `Sn` with ASTATy and `ASTAT = Rn` loads ASTATy from `Sn`. ASSUMED: STKY likewise
    /// pairs with STKYy, and a source that is no data register is broadcast (`Sn = Rn`, as for the
    /// immediate loads).
    #[cfg(test)]
    pub(super) fn apply_move(&mut self, dst: u8, src: u8, cap: &MoveSrc) {
        self.apply_move_if(dst, src, cap, true, true);
    }

    /// [`Core::apply_move`] with a condition per element (VERIFIED, census diff: in SIMD mode
    /// `IF cc Sn = Rm` follows PEy's condition and `IF cc Rn = Rm` PEx's).
    pub(super) fn apply_move_if(
        &mut self,
        dst: u8,
        src: u8,
        cap: &MoveSrc,
        x_ok: bool,
        y_ok: bool,
    ) {
        if x_ok {
            self.write_ureg(dst, cap.v);
            if let (Some(e), true) = (cap.ext, self.ext_slot(dst).is_some()) {
                self.put_ext_slot(dst, e);
            }
        }
        if !self.simd() || !y_ok {
            return;
        }
        let twin = match dst >> 4 {
            0 => Some(0x50 | dst),
            5 => Some(dst & 15),
            7 if (dst == 0x76 || dst == 0x78) && src < 0x60 && src & 0x80 == 0 => Some(dst + 1),
            _ => None,
        };
        if let Some(t) = twin {
            if t >> 4 == 7 && src >> 4 != 0 && src >> 4 != 5 {
                return; // only a data register carries a PEy value into ASTATy/STKYy
            }
            self.write_ureg(t, cap.comp);
            if let (Some(e), true) = (cap.comp_ext, self.ext_slot(t).is_some()) {
                self.put_ext_slot(t, e);
            }
        }
    }

    /// `Rx <-> Sy`: swaps the registers with their extensions (VERIFIED: the extension survives).
    pub(super) fn swap_rs(&mut self, r: usize, s: usize) {
        let (er, es) = (self.ext_slot(r as u8), self.ext_slot(0x50 | s as u8));
        std::mem::swap(&mut self.r[r], &mut self.s[s]);
        if let (Some(er), Some(es)) = (er, es) {
            self.put_ext_slot(r as u8, es);
            self.put_ext_slot(0x50 | s as u8, er);
        }
    }

    fn set_f40(&mut self, n: usize, v: u64) {
        self.r[n] = (v >> 8) as u32;
        self.fx.tag[n] = self.r[n];
        self.fx.lo[n] = v as u8;
    }

    fn apply(&mut self, o: Out) {
        let f = &mut self.flags;
        (f.az, f.an, f.av, f.ac, f.as_, f.ai) = (o.az, o.an, o.av, false, o.as_, o.ai);
        // VERIFIED (simulator): AV, AI and a flushed underflow set STKY AVS (bit 1), AIS (5), AUS (0)
        self.note_float_exceptions(u32::from(o.au) | u32::from(o.av) << 1 | u32::from(o.ai) << 5);
    }

    fn note_float_exceptions(&mut self, sticky: u32) {
        if sticky != 0 {
            self.note_arith_exception(false, sticky);
        }
    }

    fn float_result(&mut self, n: usize, o: Out) {
        self.set_f40(n, o.v);
        self.apply(o);
    }

    fn int_result(&mut self, n: usize, o: Out) {
        self.r[n] = o.v as u32;
        self.fx.clear(n);
        self.apply(o);
    }

    /// `Fn = Fx * Fy` (multiplier opcode `0x30`).
    pub(super) fn float_mul(&mut self, n: usize, x: usize, y: usize) {
        let mode = Mode::from_mode1(self.mode1);
        let o = float::mul(self.f40(x), self.f40(y), mode);
        self.set_f40(n, o.v);
        // VERIFIED (simulator): the multiplier sets MN, MV, MU, MI (ASTAT bits 6..9) and leaves
        // the ALU flags alone.
        let mult =
            u32::from(o.mn) | u32::from(o.mv) << 1 | u32::from(o.mu) << 2 | u32::from(o.mi) << 3;
        let extra = &mut self.sys.astat_extra[0];
        *extra = *extra & !(0xF << 6) | mult << 6;
        // MV, MU, MI set STKY MVS (bit 7), MUS (8), MIS (9)
        self.note_float_exceptions(
            u32::from(o.mv) << 7 | u32::from(o.mu) << 8 | u32::from(o.mi) << 9,
        );
    }

    /// Floating-point ALU operations (`op >= 0x80`). Returns false for an opcode that is not one.
    pub(super) fn float_alu(&mut self, op: u64, n: usize, x: usize, y: usize) -> bool {
        let mode = Mode::from_mode1(self.mode1);
        let (a, b) = (self.f40(x), self.f40(y));
        let (ix, iy) = (self.r[x] as i32, self.r[y] as i32);
        let abs = |o: Out| Out {
            v: o.v & !(1 << 39),
            an: false,
            ..o
        };
        let o = match op {
            0x81 => float::add(a, b, false, false, mode),
            0x82 => float::add(a, b, true, false, mode),
            0x89 => float::add(a, b, false, true, mode),
            0x91 => abs(float::add(a, b, false, false, mode)),
            0x92 => abs(float::add(a, b, true, false, mode)),
            0x8A => {
                self.shift_cacc(misc::greater(a, b, mode));
                self.apply(misc::compare(a, b, mode));
                return true;
            }
            0xA1 => misc::sign_op(a, 0, mode),
            0xA2 => misc::sign_op(a, 1, mode),
            0xB0 => misc::sign_op(a, 2, mode),
            0xA5 => conv::rnd(a, mode),
            0xBD => conv::scalb(a, iy, mode),
            0xCA => conv::from_int(ix, 0, mode),
            0xDA => conv::from_int(ix, iy, mode),
            0xE0..=0xE3 => misc::select_op(a, b, (op & 3) as u8, mode),
            0xAD | 0xC1 | 0xC4 | 0xC5 | 0xC9 | 0xCD | 0xD9 | 0xDD => {
                return self.float_to_int(op, n, a, iy, mode)
            }
            _ => return false,
        };
        self.float_result(n, o);
        true
    }

    /// The operations with a float input and a fixed-point result, plus the seeds.
    fn float_to_int(&mut self, op: u64, n: usize, a: u64, iy: i32, mode: Mode) -> bool {
        match op {
            0xAD => self.int_result(n, misc::mant(a, mode)),
            0xC1 => self.int_result(n, misc::logb(a, mode)),
            0xC4 => self.float_result(n, misc::recips(a, mode)),
            0xC5 => self.float_result(n, misc::rsqrts(a, mode)),
            0xC9 | 0xCD => self.int_result(n, conv::to_int(a, 0, op == 0xCD, mode)),
            _ => self.int_result(n, conv::to_int(a, iy, op == 0xDD, mode)),
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODE1_RND32: u32 = 1 << 16;

    fn word(op: u64, n: u64, x: u64, y: u64) -> u64 {
        (op << 12) | (n << 8) | (x << 4) | y
    }

    fn setf(c: &mut Core, n: usize, v: f32) {
        c.r[n] = v.to_bits();
    }

    #[test]
    fn extended_precision_survives_in_the_register_but_not_a_store() {
        let mut c = Core::new(&[]);
        setf(&mut c, 1, 1.0);
        setf(&mut c, 2, f32::from_bits(0x3380_0000)); // 2^-24
        assert!(c.compute(word(0x81, 3, 1, 2))); // F3 = F1 + F2
        assert_eq!(
            c.r[3],
            1.0f32.to_bits(),
            "the memory view truncates to 32 bits"
        );
        assert_eq!(c.f40(3) & 0xFF, 0x80, "the extension keeps 2^-24");
        // F4 = F3 - F1 recovers the tiny increment exactly
        assert!(c.compute(word(0x82, 4, 3, 1)));
        assert_eq!(f32::from_bits(c.r[4]), f32::from_bits(0x3380_0000));
        // with RND32 the increment is lost
        c.mode1 = MODE1_RND32;
        assert!(c.compute(word(0x81, 3, 1, 2)));
        assert_eq!(c.f40(3) & 0xFF, 0);
    }

    #[test]
    fn swaps_and_moves_through_the_s_file_keep_the_extension() {
        // VERIFIED rows: docs/research/sharc-flags/flags_measured.csv, kind `swap`
        const CSV: &str = include_str!("../../../../docs/research/sharc-flags/flags_measured.csv");
        for line in CSV.lines().filter(|l| l.starts_with("swap")) {
            let f: Vec<&str> = line.split(',').collect();
            let hex = |s: &str| u32::from_str_radix(s, 16).unwrap_or(0);
            let mut c = Core::new(&[]);
            c.mode1 = hex(f[1]);
            setf(&mut c, 1, 1.0);
            setf(&mut c, 2, f32::from_bits(0x3080_0000)); // 2^-30
            assert!(c.compute(word(0x81, 5, 1, 2))); // F5 = F1 + F2
            c.swap_rs(5, 2);
            c.swap_rs(5, 2);
            assert!(c.compute(word(0x82, 3, 5, 1)));
            let a = c.r[3];
            c.move_ureg(0x53, 5); // S3 = R5
            c.swap_rs(6, 3); // R6 <-> S3
            assert!(c.compute(word(0x82, 3, 6, 1)));
            let b = c.r[3];
            c.move_ureg(0x53, 5);
            c.move_ureg(9, 0x53); // R9 = S3
            assert!(c.compute(word(0x82, 3, 9, 1)));
            assert_eq!([a, b, c.r[3]], [hex(f[5]), hex(f[6]), hex(f[7])], "{line}");
        }
    }

    #[test]
    fn register_move_keeps_the_extension() {
        let mut c = Core::new(&[]);
        setf(&mut c, 1, 1.0);
        setf(&mut c, 2, f32::from_bits(0x3380_0000));
        assert!(c.compute(word(0x81, 3, 1, 2)));
        c.move_ureg(7, 3); // R7 = R3
        assert_eq!(c.f40(7), c.f40(3));
        assert_eq!(c.f40(7) & 0xFF, 0x80);
        c.move_ureg(0x10, 3); // I0 = R3 is not a data register
        c.move_ureg(7, 0x10); // R7 = I0 has no extension
        assert_eq!(c.f40(7) & 0xFF, 0);
    }

    #[test]
    fn overwriting_a_register_drops_its_extension() {
        let mut c = Core::new(&[]);
        setf(&mut c, 1, 1.0);
        setf(&mut c, 2, f32::from_bits(0x3380_0000));
        assert!(c.compute(word(0x81, 3, 1, 2)));
        c.write_ureg(3, 1.0f32.to_bits()); // R3 = same upper 32 bits loaded from memory
        assert_eq!(
            c.f40(3) & 0xFF,
            0,
            "a load clears the extension even for an equal value"
        );
        assert!(c.compute(word(0x81, 3, 1, 2)));
        c.r[3] ^= 1; // changed by anything else
        assert_eq!(c.f40(3) & 0xFF, 0);
    }

    #[test]
    fn flags_follow_the_reference_tables() {
        let mut c = Core::new(&[]);
        setf(&mut c, 1, f32::MAX);
        assert!(c.compute(word(0x81, 3, 1, 1))); // overflow to +infinity
        assert!(c.flags.av && !c.flags.az && !c.flags.an);
        assert_eq!(c.r[3], f32::INFINITY.to_bits());
        setf(&mut c, 1, f32::INFINITY);
        setf(&mut c, 2, f32::NEG_INFINITY);
        assert!(c.compute(word(0x81, 3, 1, 2)));
        assert!(c.flags.ai && c.r[3] == u32::MAX, "inf + -inf is invalid");
        assert!(c.compute(word(0x81, 3, 0, 0)), "0 + 0");
        assert!(c.flags.az && !c.flags.ai && !c.flags.av);
        c.mode1 = 1 << 15;
        setf(&mut c, 1, f32::MAX);
        assert!(c.compute(word(0x81, 3, 1, 1)));
        assert_eq!(c.r[3], 0x7F7F_FFFF, "TRUNC overflows to the largest normal");
        assert!(
            c.compute(word(0x8A, 0, 0, 0)),
            "COMP sets AZ for equal operands"
        );
        assert!(c.flags.az && !c.flags.av);
    }

    #[test]
    fn conversions_use_alusat_and_the_trunc_bit() {
        let mut c = Core::new(&[]);
        setf(&mut c, 1, 3.0e9);
        assert!(c.compute(word(0xC9, 2, 1, 0))); // R2 = FIX F1
        assert_eq!(c.r[2], u32::MAX);
        assert!(c.flags.ai && c.flags.av);
        c.mode1 = 1 << 13;
        assert!(c.compute(word(0xC9, 2, 1, 0)));
        assert_eq!(c.r[2], 0x7FFF_FFFF);
        assert!(!c.flags.ai);
        setf(&mut c, 1, -2.5);
        c.mode1 = 0;
        assert!(c.compute(word(0xC9, 2, 1, 0)));
        assert_eq!(c.r[2] as i32, -2, "ties to even");
        assert!(c.compute(word(0xCD, 2, 1, 0)));
        assert_eq!(c.r[2] as i32, -2, "TRUNC toward zero");
        assert!(c.flags.an);
        c.r[3] = 2; // scale by 2^2
        assert!(c.compute(word(0xD9, 2, 1, 3)));
        assert_eq!(c.r[2] as i32, -10);
    }

    #[test]
    fn logb_mant_and_seeds_are_modelled() {
        let mut c = Core::new(&[]);
        setf(&mut c, 1, 0.0);
        assert!(
            c.compute(word(0xC1, 2, 1, 0)),
            "LOGB of zero is no longer unmodelled"
        );
        assert_eq!(c.r[2], 0xFF80_0000);
        assert!(c.flags.av);
        setf(&mut c, 1, 12.0);
        assert!(c.compute(word(0xAD, 2, 1, 0)));
        assert_eq!(c.r[2], 0xC000_0000);
        assert!(c.compute(word(0xC4, 3, 1, 0)), "RECIPS");
        let seed = f64::from(f32::from_bits(c.r[3]));
        assert!((seed * 12.0 - 1.0).abs() <= 0.005);
        assert!(c.compute(word(0xC5, 3, 1, 0)), "RSQRTS");
        let seed = f64::from(f32::from_bits(c.r[3]));
        assert!((seed * 12.0f64.sqrt() - 1.0).abs() <= 1.0 / 16.0);
        assert!(
            !c.float_alu(0xF3, 3, 1, 0),
            "0xF3 is a dual add/subtract, not a float op"
        );
    }

    #[test]
    fn fdiv_iteration_of_the_runtime_library_gives_a_correctly_rounded_quotient() {
        // Same recurrence as CCES lib_fdiv.asm: q = n, d = den, r = RECIPS d;
        // 4 times: q *= r; d *= r; r = 2 - d (all in 40-bit mode).
        let mut c = Core::new(&[]);
        for (n, d) in [
            (1.0f32, 3.0f32),
            (355.0, 113.0),
            (-7.5, 0.3),
            (1.0e-5, 7.0e3),
        ] {
            (c.r[0], c.r[1], c.r[2]) = (n.to_bits(), d.to_bits(), 2.0f32.to_bits());
            assert!(c.compute(word(0xC4, 3, 1, 0))); // F3 = RECIPS F1
            for _ in 0..4 {
                assert!(c.compute((1 << 20) | (0x30 << 12) | 3)); // F0 *= F3
                assert!(c.compute((1 << 20) | (0x30 << 12) | (1 << 8) | (1 << 4) | 3)); // F1 *= F3
                assert!(c.compute(word(0x82, 3, 2, 1))); // F3 = 2 - F1
            }
            let want = n / d;
            let got = f32::from_bits(c.r[0]);
            assert!(
                ((got - want) / want).abs() <= f32::EPSILON,
                "{n}/{d}: {got} vs {want}"
            );
        }
    }

    #[test]
    fn simd_register_moves_carry_the_pey_complement() {
        let mut c = Core::new(&[]);
        c.mode1 = 1 << 21;
        (c.r[1], c.s[1]) = (11, 22);
        c.move_ureg(9, 1); // R9 = R1 also copies S1 to S9
        assert_eq!((c.r[9], c.s[9]), (11, 22));
        c.set_astat(0, 0x400);
        c.set_astat(1, 0x404);
        c.move_ureg(7, 0x76); // R7 = ASTAT: S7 = ASTATy
        assert_eq!((c.r[7], c.s[7]), (0x400, 0x404));
        (c.r[6], c.s[6]) = (0, 0);
        c.set_astat(1, 0xFF);
        c.move_ureg(0x76, 6); // ASTAT = R6 clears ASTATy from S6
        assert_eq!((c.astat(0), c.astat(1)), (0, 0));
    }
}
