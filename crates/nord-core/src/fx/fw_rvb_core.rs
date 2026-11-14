//! Register machine and memory of the firmware reverb core `0x123F61` (types 0 and 1).
//!
//! The 230 instructions are generated into `fw_rvb_steps.rs` (`tools/fx_rvbcore_gen.py`); this file
//! is what they run on. The machine mirrors `tools/fx_pe.py`: the FX routines run in SIMD mode, so
//! a data register is a pair (`PEx`, `PEy`); a `DM` access through `Ii` is the pair `(word, word +
//! 1)`, except a load through `I1`, which `BDCST1` broadcasts; a register holds a 32-bit word
//! (loaded, or produced by an integer operation) or a float of the number format `F`
//! ([`super::fw_ext40`]); circular buffers follow `Bi` / `Li` (`dag_next`).
//!
//! Memory is one pre-allocated word array in a virtual address space laid out here (context,
//! state array, PM list, all-pass buffer, two delay lines): the firmware's absolute addresses
//! (SDRAM `0x0802F000` ...) are replaced by region tags (`fw_rvb_words.rs`), they never reach the
//! audio. ASSUMED layout, VERIFIED irrelevant: the model is checked word for word against the
//! oracle on the real addresses, and ring wrap-around depends on `B`, `L` only.
//!
//! No allocation after [`Core::new`].

use super::fw_ext40::Fp;
use super::fw_rvb_words::{
    INSTANCE, INSTANCE_PTRS, REGION_AP, REGION_LINE_A, REGION_LINE_B, REGION_PM, REGION_STATE,
    TYPE_WORDS,
};

/// A register element: a data word or a float.
#[derive(Clone, Copy, Debug)]
pub(super) enum V<F: Fp> {
    W(u32),
    F(F),
}

impl<F: Fp> V<F> {
    /// The element read as a float.
    fn fl(self) -> F {
        match self {
            V::W(w) => F::from_word(w),
            V::F(f) => f,
        }
    }

    /// The element read as a 32-bit word (a float keeps its upper 32 bits).
    pub(super) fn wd(self) -> u32 {
        match self {
            V::W(w) => w,
            V::F(f) => f.to_word(),
        }
    }
}

/// A register: `[PEx, PEy]`.
pub(super) type P<F> = [V<F>; 2];

/// Context `0xB27E8` in the virtual space: `c[n]` is `mem[CTX + n]`.
pub(super) const CTX: u32 = 0x100;
const STATE: u32 = CTX + 0x61;
#[cfg(test)]
const STATE_LEN: u32 = 0x200;
const PM: u32 = 0x600;
#[cfg(test)]
const PM_LEN: u32 = 0x100;
const AP: u32 = 0x700;
#[cfg(test)]
const AP_LEN: u32 = 3200;
const LINE_A: u32 = 0x1400;
const LINE_LEN: u32 = 5008;
const LINE_B: u32 = LINE_A + LINE_LEN;
const MEM_LEN: usize = (LINE_B + LINE_LEN) as usize;

#[cfg(test)]
const _: () = assert!(STATE + STATE_LEN <= PM && PM + PM_LEN <= AP && AP + AP_LEN <= LINE_A);

/// One call's ring step: `Ii += Mm` honouring `Li` / `Bi`.
///
/// VERIFIED (`core/dag.rs::dag_next`, oracle): the wrap is one buffer length; `L = 0` is a plain add.
fn dag_next(i: u32, m: u32, l: u32, b: u32) -> u32 {
    let nxt = i.wrapping_add(m);
    if l == 0 {
        nxt
    } else if (m as i32) >= 0 {
        if nxt >= b.wrapping_add(l) {
            nxt.wrapping_sub(l)
        } else {
            nxt
        }
    } else if nxt < b {
        nxt.wrapping_add(l)
    } else {
        nxt
    }
}

/// The core's registers, DAG registers and memory.
#[derive(Clone, Debug)]
pub(super) struct Core<F: Fp> {
    pub(super) rg: [P<F>; 16],
    pub(super) i: [u32; 16],
    pub(super) m: [u32; 16],
    pub(super) l: [u32; 16],
    pub(super) b: [u32; 16],
    mem: Box<[u32]>,
    #[cfg(test)]
    pcs: Vec<u32>,
}

impl<F: Fp> Core<F> {
    /// The reverb instance of type `rvb_type` (0 or 1), buffers cleared, send and dry words
    /// `c[0x12..0x15]` set to `(1, 0)` (`kRvbAmt` 127), settled.
    pub(super) fn new(rvb_type: usize) -> Self {
        let mut c = Core {
            rg: [[V::W(0); 2]; 16],
            i: [0; 16],
            m: [0; 16],
            l: [0; 16],
            b: [0; 16],
            mem: vec![0u32; MEM_LEN].into_boxed_slice(),
            #[cfg(test)]
            pcs: Vec::new(),
        };
        for &(o, w) in &INSTANCE {
            c.set_ctx(i32::from(o), w);
        }
        for &(o, region) in &INSTANCE_PTRS {
            let base = match region {
                REGION_LINE_A => LINE_A,
                REGION_LINE_B => LINE_B,
                REGION_AP => AP,
                REGION_STATE => STATE,
                REGION_PM => PM,
                _ => 0,
            };
            c.set_ctx(i32::from(o), base);
        }
        for &(o, w) in &TYPE_WORDS[rvb_type.min(1)] {
            c.set_ctx(i32::from(o), w);
        }
        c.set_amount_words(1.0, 0.0);
        c
    }

    /// Sets the context word `c[o]`.
    pub(super) fn set_ctx(&mut self, o: i32, w: u32) {
        self.wr((CTX as i32 + o) as u32, w);
    }

    /// Reads the context word `c[o]`.
    pub(super) fn ctx(&self, o: i32) -> u32 {
        self.rd((CTX as i32 + o) as u32)
    }

    /// The `kRvbAmt` words: send `c[0x12]`, dry `c[0x13]`, and their smoothed copies `c[0x14]`,
    /// `c[0x15]` set to the same (a settled state; the core smooths later changes itself).
    pub(super) fn set_amount_words(&mut self, send: f32, dry: f32) {
        for (o, v) in [(0x12, send), (0x13, dry), (0x14, send), (0x15, dry)] {
            self.set_ctx(o, v.to_bits());
        }
    }

    /// One call: `f0`, `f2` are frames 0 and 2 of the low-passed block (`R0`, `R4`); returns the
    /// two frames of `R12`, `R14`.
    pub(super) fn call(&mut self, f0: [f32; 2], f2: [f32; 2]) -> ([f32; 2], [f32; 2]) {
        self.rg = [[V::W(0); 2]; 16];
        self.i = [0; 16];
        self.m = [0; 16];
        self.l = [0; 16];
        self.b = [0; 16];
        self.i[1] = CTX;
        // VERIFIED (oracle, `fx_rvbreal.model_machine`): the modify registers at the call
        for (k, v) in [
            (1, 1),
            (2, 2),
            (7, u32::MAX),
            (9, 1),
            (10, 2),
            (15, u32::MAX),
        ] {
            self.m[k] = v;
        }
        self.rg[0] = [V::W(f0[0].to_bits()), V::W(f0[1].to_bits())];
        self.rg[4] = [V::W(f2[0].to_bits()), V::W(f2[1].to_bits())];
        #[cfg(test)]
        self.pcs.clear();
        self.run_steps();
        let out = |n: usize| {
            [
                f32::from_bits(self.rg[n][0].wd()),
                f32::from_bits(self.rg[n][1].wd()),
            ]
        };
        (out(12), out(14))
    }

    // --- memory and register access of the generated steps ---------------------------------
    pub(super) fn rd(&self, a: u32) -> u32 {
        self.mem.get(a as usize).copied().unwrap_or(0)
    }

    fn wr(&mut self, a: u32, w: u32) {
        if let Some(c) = self.mem.get_mut(a as usize) {
            *c = w;
        }
    }

    /// Store of a pair: `PEx` at `a`, `PEy` at `a + 1`.
    pub(super) fn wr2(&mut self, a: u32, p: P<F>) {
        self.wr(a, p[0].wd());
        self.wr(a.wrapping_add(1), p[1].wd());
    }

    pub(super) fn r(&self, n: usize) -> P<F> {
        self.rg[n]
    }

    pub(super) fn ld(&self, a: u32) -> P<F> {
        [V::W(self.rd(a)), V::W(self.rd(a.wrapping_add(1)))]
    }

    /// Load through `I1`: the `PEx` word in both elements (`BDCST1`).
    pub(super) fn ldb(&self, a: u32) -> P<F> {
        [V::W(self.rd(a)); 2]
    }

    /// `Ik + off`.
    pub(super) fn a(&self, k: usize, off: i32) -> u32 {
        self.i[k].wrapping_add(off as u32)
    }

    /// `Ii` after `Ii += Mm` (circular when `Li != 0`).
    pub(super) fn modi(&self, i: usize, mm: usize) -> u32 {
        dag_next(self.i[i], self.m[mm], self.l[i], self.b[i])
    }

    /// `Ii + Mm` without wrap or write-back (the register pre-modify form).
    pub(super) fn plain(&self, i: usize, j: usize) -> u32 {
        self.i[i].wrapping_add(self.m[j])
    }

    fn lanes(a: P<F>, b: P<F>, f: impl Fn(F, F) -> F) -> P<F> {
        [V::F(f(a[0].fl(), b[0].fl())), V::F(f(a[1].fl(), b[1].fl()))]
    }

    pub(super) fn fadd(&self, a: P<F>, b: P<F>) -> P<F> {
        Self::lanes(a, b, F::add)
    }
    pub(super) fn fsub(&self, a: P<F>, b: P<F>) -> P<F> {
        Self::lanes(a, b, F::sub)
    }
    pub(super) fn fmul(&self, a: P<F>, b: P<F>) -> P<F> {
        Self::lanes(a, b, F::mul)
    }
    pub(super) fn favg(&self, a: P<F>, b: P<F>) -> P<F> {
        Self::lanes(a, b, F::avg)
    }
    /// `Fn = PASS Fx`: the value as a float, no rounding.
    pub(super) fn fpass(&self, a: P<F>) -> P<F> {
        [V::F(a[0].fl()), V::F(a[1].fl())]
    }
    /// `Rn = Sm`: the pair with its elements exchanged.
    pub(super) fn swap(&self, a: P<F>) -> P<F> {
        [a[1], a[0]]
    }

    /// Instruction marker (the pc log exists in tests only).
    #[inline(always)]
    pub(super) fn tick(&mut self, _pc: u32) {
        #[cfg(test)]
        self.pcs.push(_pc);
    }
}

#[cfg(test)]
impl<F: Fp> Core<F> {
    pub(super) fn new_for_test() -> Self {
        Self::new(0)
    }
    pub(super) fn ticks_for_test(&self) -> usize {
        self.pcs.len()
    }
    pub(super) fn pass_count_for_test(&self) -> usize {
        self.pcs.iter().filter(|&&p| p == 0x124033).count()
    }
    pub(super) fn first_last_pc_for_test(&self) -> (u32, u32) {
        (self.pcs[0], *self.pcs.last().expect("pcs"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dag_wraps_by_one_buffer_length() {
        assert_eq!(dag_next(10, 2, 0, 0), 12, "L = 0 is a plain add");
        assert_eq!(dag_next(108, 2, 10, 100), 100, "reaches B + L: subtract L");
        assert_eq!(dag_next(100, u32::MAX, 10, 100), 109, "below B: add L");
        assert_eq!(dag_next(105, u32::MAX, 10, 100), 104);
    }

    #[test]
    fn the_instance_layout_points_into_the_regions() {
        let c = Core::<f32>::new(0);
        assert_eq!(c.ctx(39), LINE_A);
        assert_eq!(c.ctx(44), LINE_B);
        assert_eq!(c.ctx(50), AP);
        assert_eq!(c.ctx(-13), STATE);
        assert_eq!(c.ctx(-12), PM);
        assert_eq!(c.ctx(0x20), 1570);
        assert_eq!(f32::from_bits(c.ctx(0x12)), 1.0);
    }

    #[test]
    fn a_silent_call_returns_silence_and_runs_the_documented_instruction_count() {
        let mut c = Core::<f32>::new(1);
        let (a, b) = c.call([0.0; 2], [0.0; 2]);
        assert_eq!((a, b), ([0.0; 2], [0.0; 2]));
        assert!(c.ticks_for_test() > 21 * 16 + 150);
    }

    #[test]
    fn the_padding_between_the_regions_stays_zero_under_noise() {
        let mut c = Core::<f32>::new(0);
        let mut s = 7u32;
        for _ in 0..40 {
            let mut fr = [[0.0f32; 2]; 2];
            for v in fr.iter_mut().flatten() {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                *v = (s >> 8) as f32 / 16_777_216.0 - 0.5;
            }
            c.call(fr[0], fr[1]);
        }
        let zero = |lo: u32, hi: u32| (lo..hi).all(|a| c.rd(a) == 0);
        assert!(zero(STATE + STATE_LEN, PM), "state array overflow canary");
        assert!(zero(AP + AP_LEN, LINE_A), "all-pass buffer overflow canary");
        assert!(zero(PM, PM + PM_LEN), "the PM list is never written");
    }

    #[test]
    fn pair_loads_broadcast_only_through_the_ldb_form() {
        let mut c = Core::<f32>::new(0);
        c.wr(0x50, 7);
        c.wr(0x51, 9);
        assert_eq!(c.ld(0x50)[1].wd(), 9);
        assert_eq!(c.ldb(0x50)[1].wd(), 7);
        let p = c.fadd([V::F(1.0), V::F(2.0)], [V::W(0x3F80_0000), V::F(0.5)]);
        assert_eq!((p[0].wd(), p[1].wd()), (2.0f32.to_bits(), 2.5f32.to_bits()));
        assert_eq!(c.swap(p)[0].wd(), 2.5f32.to_bits());
    }
}
