//! Pair-register machine for the instruction-level ports of the FX routines
//! (`fw_chorus4`, `fw_chorus5`; the Rust counterpart of `tools/fx_pe.py`).
//!
//! The FX routines run in SIMD mode: every data register is a pair, `PEx` (the `R` file) and
//! `PEy` (the `S` file). A port is a statement per executed instruction, built from [`Eff`]
//! effects that are all evaluated before any is applied (a compute reads the registers as they
//! were before the instruction). Registers hold raw 32-bit words; float operations read a word as
//! f32 (a zero exponent field flushes to zero) and write the rounded f32 word, so this port
//! differs from the 40-bit registers of the firmware by rounding only (the golden vectors of
//! `tools/fx_golden_mod2.py` hold to a relative `5e-6`).
//!
//! VERIFIED semantics (`tools/fx_pe.py` against the `sharc-fx` oracle): a load through `I1` is
//! broadcast to both elements, any other `DM` access is the pair `(word, word + 1)`, a write to
//! `Bn` also loads `In`, and `Ii += Mm` wraps by one buffer length `Ln` at `Bn`.

use super::fw_words::val;

/// Register pair: element `PEx`, element `PEy`.
pub(super) type P = [u32; 2];

/// Flat data memory of a routine (`address -> word`).
pub(super) trait Mem {
    fn rd(&self, addr: u32) -> u32;
    fn wr(&mut self, addr: u32, word: u32);
}

/// One effect of an instruction.
#[derive(Clone, Copy)]
pub(super) enum Eff {
    /// Data register pair.
    W(usize, P),
    /// `PEx` element only.
    Wx(usize, u32),
    /// `PEy` element only.
    Wy(usize, u32),
    /// Pair store (`addr`, `addr + 1`).
    St(u32, P),
    /// DAG registers `I`, `M`, `L`, `B` (`B` also loads `I`).
    Di(usize, u32),
    Dm(usize, u32),
    Dl(usize, u32),
    Db(usize, u32),
    /// Condition flags `(AN, AZ)` of a compute (per element).
    Fl2([bool; 2], [bool; 2]),
    /// Condition flags `(AN, AZ, AV, AC)` of a fixed add or subtract.
    Fl4([bool; 2], [bool; 2], [bool; 2], [bool; 2]),
}
pub(super) use Eff::{Db, Di, Dl, Dm, Fl2, Fl4, St, Wx, Wy, W};

/// Registers, DAG registers and the memory of one routine.
pub(super) struct Pe<M: Mem> {
    pub(super) x: [u32; 16],
    pub(super) y: [u32; 16],
    pub(super) i: [u32; 8],
    pub(super) m: [u32; 8],
    pub(super) l: [u32; 8],
    pub(super) b: [u32; 8],
    /// `ASTAT` flags per element: negative, zero, overflow, carry (`fw_pe_ops.rs` reads them).
    pub(super) an: [bool; 2],
    pub(super) az: [bool; 2],
    pub(super) av: [bool; 2],
    pub(super) ac: [bool; 2],
    pub(super) mem: M,
}

impl<M: Mem> Pe<M> {
    pub(super) fn new(mem: M, i0: u32, i1: u32) -> Self {
        let mut pe = Self {
            x: [0; 16],
            y: [0; 16],
            i: [0; 8],
            m: [0; 8],
            l: [0; 8],
            b: [0; 8],
            an: [false; 2],
            az: [false; 2],
            av: [false; 2],
            ac: [false; 2],
            mem,
        };
        (pe.i[0], pe.i[1]) = (i0, i1);
        (pe.m[1], pe.m[2]) = (1, 2);
        pe
    }

    pub(super) fn r(&self, n: usize) -> P {
        [self.x[n], self.y[n]]
    }

    pub(super) fn rd(&self, addr: u32) -> u32 {
        self.mem.rd(addr)
    }

    pub(super) fn ld(&self, addr: u32) -> P {
        [self.mem.rd(addr), self.mem.rd(addr.wrapping_add(1))]
    }

    /// Load through `I1`: `BDCST1` copies the `PEx` word to both elements.
    pub(super) fn ldb(&self, addr: u32) -> P {
        let w = self.mem.rd(addr);
        [w, w]
    }

    pub(super) fn a(&self, i: usize, off: i32) -> u32 {
        self.i[i].wrapping_add(off as u32)
    }

    /// `Ii` modified by `Mm`, wrapping inside the circular buffer `Li` / `Bi`.
    pub(super) fn dmod(&self, i: usize, m: usize) -> u32 {
        let (cur, step, len, base) = (self.i[i], self.m[m], self.l[i], self.b[i]);
        let next = cur.wrapping_add(step);
        if len == 0 {
            next
        } else if (step as i32) >= 0 {
            if next >= base.wrapping_add(len) {
                next.wrapping_sub(len)
            } else {
                next
            }
        } else if next < base {
            next.wrapping_add(len)
        } else {
            next
        }
    }

    /// Applies the effects of one instruction (all values were computed before the call).
    pub(super) fn apply(&mut self, effs: &[Eff]) {
        for e in effs {
            match *e {
                W(n, p) => (self.x[n], self.y[n]) = (p[0], p[1]),
                Wx(n, v) => self.x[n] = v,
                Wy(n, v) => self.y[n] = v,
                St(a, p) => {
                    self.mem.wr(a, p[0]);
                    self.mem.wr(a.wrapping_add(1), p[1]);
                }
                Di(n, v) => self.i[n] = v,
                Dm(n, v) => self.m[n] = v,
                Dl(n, v) => self.l[n] = v,
                Db(n, v) => (self.b[n], self.i[n]) = (v, v),
                Fl2(an, az) => (self.an, self.az) = (an, az),
                Fl4(an, az, av, ac) => (self.an, self.az, self.av, self.ac) = (an, az, av, ac),
            }
        }
    }
}

fn lift2(a: P, b: P, f: impl Fn(f32, f32) -> f32) -> P {
    [0, 1].map(|c| f(val(a[c]), val(b[c])).to_bits())
}

pub(super) fn fadd(a: P, b: P) -> P {
    lift2(a, b, |x, y| x + y)
}

pub(super) fn fsub(a: P, b: P) -> P {
    lift2(a, b, |x, y| x - y)
}

pub(super) fn fmul(a: P, b: P) -> P {
    lift2(a, b, |x, y| x * y)
}

pub(super) fn fabs(a: P) -> P {
    a.map(|w| val(w).abs().to_bits())
}

pub(super) fn iadd(a: P, b: P) -> P {
    [0, 1].map(|c| a[c].wrapping_add(b[c]))
}

pub(super) fn ineg(a: P) -> P {
    a.map(u32::wrapping_neg)
}

/// `Rn = TRUNC Fx`: round toward zero to a signed word.
pub(super) fn ftrunc(a: P) -> P {
    a.map(|w| (val(w) as i32) as u32)
}

/// `Fn = FLOAT Rx BY Ry`: the signed integer `a` times `2^b`.
pub(super) fn ffloat(a: P, b: P) -> P {
    [0, 1].map(|c| ((f64::from(a[c] as i32) * 2f64.powi(b[c] as i32)) as f32).to_bits())
}

pub(super) fn imm(w: u32) -> P {
    [w, w]
}

/// Per element `AZ` of a fixed-point `pass`.
pub(super) fn azero(a: P) -> [bool; 2] {
    a.map(|w| w == 0)
}

/// `IF EQ Rn = Rm`: the new value where `AZ` was set, else the old one.
pub(super) fn ifeq(az: [bool; 2], new: P, old: P) -> P {
    [0, 1].map(|c| if az[c] { new[c] } else { old[c] })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Flat(Vec<u32>);

    impl Mem for Flat {
        fn rd(&self, addr: u32) -> u32 {
            self.0[addr as usize]
        }
        fn wr(&mut self, addr: u32, word: u32) {
            self.0[addr as usize] = word;
        }
    }

    fn pe() -> Pe<Flat> {
        Pe::new(Flat(vec![0; 64]), 0, 8)
    }

    #[test]
    fn a_load_through_i1_broadcasts_and_other_loads_are_pairs() {
        let mut p = pe();
        p.mem.0[8] = 7;
        p.mem.0[9] = 9;
        assert_eq!(p.ldb(p.a(1, 0)), [7, 7]);
        assert_eq!(p.ld(p.a(1, 0)), [7, 9]);
    }

    #[test]
    fn effects_read_the_old_registers_and_a_swap_exchanges_the_elements() {
        let mut p = pe();
        p.apply(&[W(1, [1, 2]), W(2, [3, 4])]);
        p.apply(&[Wx(2, p.y[1]), Wy(1, p.x[2])]);
        assert_eq!((p.r(1), p.r(2)), ([1, 3], [2, 4]));
    }

    #[test]
    fn a_circular_modify_wraps_by_one_buffer_length() {
        let mut p = pe();
        p.apply(&[Db(6, 16), Dl(6, 8), Dm(5, 10), Dm(4, 0xFFFF_FFFE)]);
        p.apply(&[Di(6, 20)]);
        assert_eq!(p.dmod(6, 5), 22, "20 + 10 = 30 >= 24: 30 - 8");
        p.apply(&[Di(6, 17)]);
        assert_eq!(p.dmod(6, 4), 23, "17 - 2 = 15 < 16: 15 + 8");
        assert_eq!(p.b[6], 16);
    }

    #[test]
    fn float_helpers_follow_the_routine_semantics() {
        let one = 1.0f32.to_bits();
        assert_eq!(fadd([one, one], [one, 0])[0], 2.0f32.to_bits());
        assert_eq!(ftrunc([(-2.7f32).to_bits(); 2]), [(-2i32) as u32; 2]);
        assert_eq!(
            ffloat([(-4i32) as u32; 2], imm((-1i32) as u32)),
            [(-2.0f32).to_bits(); 2]
        );
        assert_eq!(ifeq([true, false], [1, 1], [2, 2]), [1, 2]);
        assert_eq!(azero([0, 5]), [true, false]);
    }
}
