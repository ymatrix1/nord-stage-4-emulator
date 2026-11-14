//! Register machine and memory of the firmware Leslie processor (`0x1256AD`, PNO core 1).
//!
//! The 1,440 + 445 instructions per call are generated into `fw_leslie_main_*.rs` and
//! `fw_leslie_callee.rs` (`tools/fx_leslie_rust_gen.py`); this file is what they run on. The machine
//! mirrors `tools/fx_pe.py` / `tools/fx_leslie_ops.py`: the routine runs in SIMD mode, so a data
//! register is a pair (`PEx`, `PEy`); a `DM` / `PM` access through `Ii` is the pair of the word
//! at the address and the next word, except a load through `I1` or `I9`, which `BDCST1` /
//! `BDCST9` broadcast; a register holds a 32-bit word or a float of the format `F`
//! ([`super::fw_ext40`]); circular buffers follow `Bi` / `Li`. The memory keeps the firmware's own
//! addresses in a few windows (the words of the boot state, `fw_leslie_data_a`, `_b` and `_c`);
//! a store outside every window is counted in tests, a read there gives 0.
//!
//! No allocation after [`Core::new`].

use super::fw_ext40::Fp;
use super::fw_leslie_data_a as data_a;
use super::fw_leslie_data_b as data_b;
use super::fw_leslie_data_c as data_c;
use super::fw_leslie_ring::RingAdvance;

/// A register element: a data word or a float.
#[derive(Clone, Copy, Debug)]
pub(super) enum V<F: Fp> {
    W(u32),
    F(F),
}

impl<F: Fp> V<F> {
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

/// Signal pair address of the call (VERIFIED: the oracle harness `tools/fx_leslie_run.py`).
pub(super) const IO: u32 = 0xB6000;
/// The state pointer `I1` of the processor: the row word `0xB2F2A`.
pub(super) const STATE: u32 = 0xB2F2A;
/// Rotor phase limit word and the three speed words.
pub(super) const LIMIT: u32 = 0xB2F21;
pub(super) const SPEED_WORDS: [u32; 3] = [0xB2F25, 0xB2F26, 0xB2FA1];
const MAX_WINDOWS: usize = 24;

/// `Ii += Mm` honouring `Li` / `Bi` (`core/dag.rs::dag_next`, VERIFIED by the oracle).
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

/// The processor's registers, DAG registers, flags and memory.
#[derive(Clone, Debug)]
pub(super) struct Core<F: Fp> {
    pub(super) rg: [P<F>; 16],
    pub(super) i: [u32; 16],
    pub(super) m: [u32; 16],
    pub(super) l: [u32; 16],
    pub(super) b: [u32; 16],
    pub(super) an: [bool; 2],
    pub(super) az: [bool; 2],
    pub(super) tf: [bool; 2],
    win: [(u32, u32, u32); MAX_WINDOWS],
    nwin: usize,
    mem: Box<[u32]>,
    ring: RingAdvance,
    calls: u64,
    #[cfg(test)]
    pcs: Vec<u32>,
    #[cfg(test)]
    stray: u32,
}

impl<F: Fp> Core<F> {
    /// The processor in the state of the boot log, the rotor running at the slow speed words:
    /// limit word `0x7FFFFFFF`, speed words as the ARM re-sends them.
    pub(super) fn new() -> Self {
        let mut win = [(0, 0, 0); MAX_WINDOWS];
        let (mut n, mut total) = (0, 0u32);
        for &(base, words) in data_a::WINDOWS
            .iter()
            .chain(data_b::WINDOWS.iter())
            .chain(data_c::WINDOWS.iter())
        {
            win[n] = (base, words.len() as u32, total);
            total += words.len() as u32;
            n += 1;
        }
        let mut c = Core {
            rg: [[V::W(0); 2]; 16],
            i: [0; 16],
            m: [0; 16],
            l: [0; 16],
            b: [0; 16],
            an: [false; 2],
            az: [false; 2],
            tf: [false; 2],
            win,
            nwin: n,
            mem: vec![0u32; total as usize].into_boxed_slice(),
            ring: RingAdvance::ADVANCE,
            calls: 0,
            #[cfg(test)]
            pcs: Vec::new(),
            #[cfg(test)]
            stray: 0,
        };
        for &(base, words) in data_a::WINDOWS
            .iter()
            .chain(data_b::WINDOWS.iter())
            .chain(data_c::WINDOWS.iter())
        {
            for (k, &w) in words.iter().enumerate() {
                c.wr(base + k as u32, w);
            }
        }
        for n in 0..16 {
            c.rg[n] = [V::W(data_a::ENTRY_R[n]), V::W(data_a::ENTRY_S[n])];
            c.i[n] = data_a::ENTRY_I[n];
            c.m[n] = data_a::ENTRY_M[n];
            c.l[n] = data_a::ENTRY_L[n];
            c.b[n] = data_a::ENTRY_B[n];
        }
        c.wr(LIMIT, 0x7FFF_FFFF);
        c.set_speed_words(data_a::SLOW_SPEED);
        c
    }

    /// A machine with zeroed memory windows `(base, words)` and no boot state: the multi-tap
    /// reverb (`fw_rvbm.rs`) lays out its own instance. Allocates once, here.
    pub(super) fn blank(windows: &[(u32, u32)]) -> Self {
        assert!(windows.len() <= MAX_WINDOWS, "too many memory windows");
        let mut win = [(0, 0, 0); MAX_WINDOWS];
        let mut total = 0u32;
        for (k, &(base, len)) in windows.iter().enumerate() {
            win[k] = (base, len, total);
            total += len;
        }
        Core {
            rg: [[V::W(0); 2]; 16],
            i: [0; 16],
            m: [0; 16],
            l: [0; 16],
            b: [0; 16],
            an: [false; 2],
            az: [false; 2],
            tf: [false; 2],
            win,
            nwin: windows.len(),
            mem: vec![0u32; total as usize].into_boxed_slice(),
            ring: RingAdvance::FROZEN,
            calls: 0,
            #[cfg(test)]
            pcs: Vec::new(),
            #[cfg(test)]
            stray: 0,
        }
    }

    /// Zeroes every memory window (no allocation).
    pub(super) fn clear_memory(&mut self) {
        self.mem.fill(0);
    }

    /// Sets the three rotor speed words (`0xB2F25`, `0xB2F26`, `0xB2FA1`).
    pub(super) fn set_speed_words(&mut self, w: [u32; 3]) {
        for (a, v) in SPEED_WORDS.into_iter().zip(w) {
            self.wr(a, v);
        }
    }

    /// Sets how the history ring pointers move between calls (default: the firmware's).
    pub(super) fn set_ring(&mut self, ring: RingAdvance) {
        self.ring = ring;
    }

    /// The history ring behaviour in use.
    pub(super) fn ring(&self) -> RingAdvance {
        self.ring
    }

    /// One call: the signal pair `io = [L0, R0, L1, R1]` is processed in place; returns the pair.
    pub(super) fn call(&mut self, io: [f32; 4]) -> [f32; 4] {
        for (k, v) in io.iter().enumerate() {
            self.wr(IO + k as u32, v.to_bits());
        }
        if self.ring.stride != 0 {
            for (a, v) in self.ring.pointers(self.calls) {
                self.wr(a, v);
            }
        }
        self.calls = self.calls.wrapping_add(1);
        self.i[0] = IO;
        self.i[1] = STATE;
        self.m[0] = 0;
        self.m[1] = 1;
        self.m[2] = 2;
        self.m[9] = 1;
        #[cfg(test)]
        self.pcs.clear();
        self.run_main();
        let mut out = [0.0f32; 4];
        for (k, o) in out.iter_mut().enumerate() {
            *o = f32::from_bits(self.rd(IO + k as u32));
        }
        out
    }

    /// The processor: its generated parts in order.
    fn run_main(&mut self) {
        self.main_a();
        self.main_b();
        self.main_c();
        self.main_d();
        self.main_e();
    }

    // --- memory and register access of the generated steps ---------------------------------
    fn slot(&self, a: u32) -> Option<usize> {
        for &(base, len, off) in &self.win[..self.nwin] {
            if a >= base && a - base < len {
                return Some((off + a - base) as usize);
            }
        }
        None
    }

    pub(super) fn rd(&self, a: u32) -> u32 {
        self.slot(a).map_or(0, |s| self.mem[s])
    }

    pub(super) fn wr(&mut self, a: u32, w: u32) {
        if let Some(s) = self.slot(a) {
            self.mem[s] = w;
        } else {
            #[cfg(test)]
            {
                self.stray += 1;
            }
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

    /// Load through `I1` or `I9`: the `PEx` word in both elements.
    pub(super) fn ldb(&self, a: u32) -> P<F> {
        [V::W(self.rd(a)); 2]
    }

    pub(super) fn a(&self, k: usize, off: i32) -> u32 {
        self.i[k].wrapping_add(off as u32)
    }

    /// `Ii` after `Ii += Mm` (circular when `Li != 0`).
    pub(super) fn modi(&self, i: usize, mm: usize) -> u32 {
        dag_next(self.i[i], self.m[mm], self.l[i], self.b[i])
    }

    /// `Ii` after `Ii += imm`.
    pub(super) fn modimm(&self, i: usize, imm: u32) -> u32 {
        dag_next(self.i[i], imm, self.l[i], self.b[i])
    }

    /// `Ii + Mm` without wrap or write-back (the register pre-modify form).
    pub(super) fn plain(&self, i: usize, j: usize) -> u32 {
        self.i[i].wrapping_add(self.m[j])
    }

    /// Instruction marker (the pc log exists in tests only).
    #[inline(always)]
    pub(super) fn tick(&mut self, _pc: u32) {
        #[cfg(test)]
        self.pcs.push(_pc);
    }

    #[cfg(test)]
    pub(super) fn ticks(&self) -> usize {
        self.pcs.len()
    }

    #[cfg(test)]
    pub(super) fn stray(&self) -> u32 {
        self.stray
    }

    pub(super) fn f(v: V<F>) -> F {
        v.fl()
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
    }

    #[test]
    fn windows_map_addresses_and_unmapped_words_read_zero() {
        let mut c = Core::<f32>::new();
        assert_eq!(c.rd(LIMIT), 0x7FFF_FFFF);
        assert_eq!(c.rd(0x1), 0, "outside every window");
        c.wr(IO, 0x1234);
        assert_eq!(c.rd(IO), 0x1234);
        assert_eq!(c.stray(), 0);
        c.wr(0x1, 5);
        assert_eq!(c.stray(), 1);
        c.set_speed_words([1, 2, 3]);
        assert_eq!((c.rd(SPEED_WORDS[0]), c.rd(SPEED_WORDS[2])), (1, 3));
    }

    #[test]
    fn pair_loads_broadcast_only_through_the_ldb_form() {
        let mut c = Core::<f32>::new();
        c.wr(IO, 7);
        c.wr(IO + 1, 9);
        assert_eq!(c.ld(IO)[1].wd(), 9);
        assert_eq!(c.ldb(IO)[1].wd(), 7);
    }
}
