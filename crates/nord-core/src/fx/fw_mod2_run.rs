//! Driver of the generated Mod 2 routine ports ([`super::fw_vibe`], [`super::fw_flang`],
//! [`super::fw_phas0`], [`super::fw_phas1`], [`super::fw_ens`], [`super::fw_spin`]): the memory
//! map and one call on a frame block.
//!
//! The routines keep their state in the context words around `I1` (phase, filter states,
//! smoothers) and, for the delay routines, a circular ring of `0x400` words that the ARM
//! allocates (`SPIN` has a second ring of `0x26C` words and scratch words at `0xC2861..`, `ENS`
//! reads a waveform table at `0xC0BD4`). Here the context, the rings, the scratch words and one
//! small frame block are arrays at the addresses of the model scenarios
//! (`tools/fx_model_mod2.py`), so a context captured from the DSP loads unchanged and the
//! written-back words compare word for word with the golden vectors.
//!
//! VERIFIED: every routine against the `sharc-fx` oracle (`docs/research/fx-mod2-types.md`).

use super::fw_chorus::{CTX_I1, CTX_LO, MAX_BLOCKS, RING_BASE, RING_N};
use super::fw_mod2_tables::ENS_TABLE;
use super::fw_pe::{Mem, Pe};
use super::{
    fw_awah, fw_ens, fw_flang, fw_phas0, fw_phas1, fw_pump, fw_rm, fw_spin, fw_vibe, fw_wah,
};

/// Context words `c[-0x20] ..= c[0x2A]` (`SPIN` reads up to `c[0x29]`).
pub(super) const CTX_N: usize = 0x4B;
/// Second ring (`SPIN`, `L7 = 0x26C`) and its size in words (the ring plus the next pair).
pub(super) const RING2_BASE: u32 = 0xB5000;
pub(super) const RING2_N: usize = 0x300;
/// Scratch words of `SPIN` (`0xC2861 ..= 0xC286B`).
const SCRATCH_BASE: u32 = 0xC2860;
const SCRATCH_N: usize = 16;
/// `ENS` waveform table (DM `0xC0BD4`, 1025 words).
const ENS_BASE: u32 = 0xC0BD4;
/// Frame block (`I0`).
const IO_BASE: u32 = 0xB1000;

/// Which routine runs (`kFx2Type`: PHAS 0, 1, FLANG 2, 3, VIBE 6, 7, ENS 8, 9, SPIN 10, 11; and
/// the Mod 1 routines `kFx1Type` RM 4, 5, A-WAH 6, 7, WAH 8, 9, PUMP 10, 11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Routine {
    /// `kFx2Type` 6, 7: `0x123C84`.
    Vibe,
    /// `kFx2Type` 2, 3: `0x12348D`.
    Flang,
    /// `kFx2Type` 0: `0x12379B`.
    Phas0,
    /// `kFx2Type` 1: `0x1238FC`.
    Phas1,
    /// `kFx2Type` 8, 9: `0x123A7B`.
    Ens,
    /// `kFx2Type` 10, 11: `0x123557`.
    Spin,
    /// `kFx1Type` 4, 5 (`RM`): `0x1239D0`.
    Rm,
    /// `kFx1Type` 8, 9 (`WAH`): `0x123DD2`.
    Wah,
    /// `kFx1Type` 6, 7 (`A-WAH`): `0x123E9B`.
    Awah,
    /// `kFx1Type` 10, 11 (`PUMP`): `0x122D2C`.
    Pump,
}

impl Routine {
    /// Context offset of the block count word (`lcntr`).
    pub(super) const fn blocks_offset(self) -> i32 {
        match self {
            Self::Vibe => 0x14,
            Self::Flang => 2,
            Self::Phas0 => 0x1B,
            Self::Phas1 => 4,
            Self::Ens => 0x11,
            Self::Spin => 0x21,
            Self::Rm => -0x10,
            Self::Wah => 7,
            Self::Awah => 4,
            Self::Pump => -8,
        }
    }
}

/// The state a stage keeps between calls: context, ring, second ring, scratch words.
#[derive(Debug, Clone)]
pub(super) struct Mod2State {
    pub(super) ctx: [u32; CTX_N],
    pub(super) ring: [u32; RING_N],
    pub(super) ring2: [u32; RING2_N],
    scratch: [u32; SCRATCH_N],
}

impl Mod2State {
    pub(super) fn new() -> Self {
        Self {
            ctx: [0; CTX_N],
            ring: [0; RING_N],
            ring2: [0; RING2_N],
            scratch: [0; SCRATCH_N],
        }
    }
}

/// The memory regions of one call.
struct Mod2Mem<'a> {
    st: &'a mut Mod2State,
    io: [u32; 4 * MAX_BLOCKS],
}

fn slot(addr: u32, lo: u32, n: usize) -> Option<usize> {
    let k = addr.wrapping_sub(lo) as usize;
    (k < n).then_some(k)
}

impl Mem for Mod2Mem<'_> {
    fn rd(&self, a: u32) -> u32 {
        if let Some(k) = slot(a, CTX_LO, CTX_N) {
            self.st.ctx[k]
        } else if let Some(k) = slot(a, RING_BASE, RING_N) {
            self.st.ring[k]
        } else if let Some(k) = slot(a, IO_BASE, self.io.len()) {
            self.io[k]
        } else if let Some(k) = slot(a, RING2_BASE, RING2_N) {
            self.st.ring2[k]
        } else if let Some(k) = slot(a, ENS_BASE, ENS_TABLE.len()) {
            ENS_TABLE[k]
        } else if let Some(k) = slot(a, SCRATCH_BASE, SCRATCH_N) {
            self.st.scratch[k]
        } else {
            0
        }
    }

    fn wr(&mut self, a: u32, w: u32) {
        if let Some(k) = slot(a, CTX_LO, CTX_N) {
            self.st.ctx[k] = w;
        } else if let Some(k) = slot(a, RING_BASE, RING_N) {
            self.st.ring[k] = w;
        } else if let Some(k) = slot(a, IO_BASE, self.io.len()) {
            self.io[k] = w;
        } else if let Some(k) = slot(a, RING2_BASE, RING2_N) {
            self.st.ring2[k] = w;
        } else if let Some(k) = slot(a, SCRATCH_BASE, SCRATCH_N) {
            self.st.scratch[k] = w;
        }
    }
}

/// One call on `frames = [l0, r0, l1, r1, ...]` in place: whole blocks of two stereo frames (a
/// trailing partial block is left untouched). The routine's block count word is set to the number
/// of blocks.
pub(super) fn process(routine: Routine, st: &mut Mod2State, frames: &mut [f32]) {
    let n = (frames.len() / 4).min(MAX_BLOCKS);
    st.ctx[(routine.blocks_offset() + 0x20) as usize] = n as u32;
    let mut io = [0u32; 4 * MAX_BLOCKS];
    for (w, f) in io.iter_mut().zip(frames.iter()).take(4 * n) {
        *w = f.to_bits();
    }
    let mut pe = Pe::new(Mod2Mem { st, io }, IO_BASE, CTX_I1);
    match routine {
        Routine::Vibe => fw_vibe::run(&mut pe),
        Routine::Flang => fw_flang::run(&mut pe),
        Routine::Phas0 => fw_phas0::run(&mut pe),
        Routine::Phas1 => fw_phas1::run(&mut pe),
        Routine::Ens => fw_ens::run(&mut pe),
        Routine::Spin => fw_spin::run(&mut pe),
        Routine::Rm => fw_rm::run(&mut pe),
        Routine::Wah => fw_wah::run(&mut pe),
        Routine::Awah => fw_awah::run(&mut pe),
        Routine::Pump => fw_pump::run(&mut pe),
    }
    for (f, w) in frames.iter_mut().zip(pe.mem.io).take(4 * n) {
        *f = f32::from_bits(w);
    }
}

#[cfg(test)]
mod tests {
    use super::super::fw_words::assert_close;
    use super::super::tests_mod1_golden as g1;
    use super::super::tests_mod2_golden as g;
    use super::*;

    /// The deterministic ring of `tools/fx_golden_mod2.py::ring_word`.
    fn golden_ring<const N: usize>() -> [u32; N] {
        std::array::from_fn(|k| {
            let h = (k as u32).wrapping_mul(2_654_435_761).wrapping_add(12345);
            ((h >> 9) as f32 / 4_194_304.0 - 1.0).to_bits()
        })
    }

    /// Relative tolerance of the delay routines: f32 against the 40-bit registers of the firmware
    /// (measured differences up to 2e-5 after three calls with a feedback path).
    const LOOSE: f32 = 1.0e-4;

    /// The changed words of `ring` against the deterministic ring, as `(offset, word)`.
    fn changed<const N: usize>(ring: &[u32; N]) -> Vec<(u32, u32)> {
        let want = golden_ring::<N>();
        (0..N)
            .filter(|&k| ring[k] != want[k])
            .map(|k| (k as u32, ring[k]))
            .collect()
    }

    fn check_ring(got: &[(u32, u32)], want: &[(u32, u32)], tol: f32, what: &str) {
        let offs = |v: &[(u32, u32)]| v.iter().map(|d| d.0).collect::<Vec<_>>();
        assert_eq!(offs(got), offs(want), "{what}: the same words are written");
        let words = |v: &[(u32, u32)]| v.iter().map(|d| d.1).collect::<Vec<_>>();
        assert_close(&words(got), &words(want), tol, what);
    }

    /// Runs a golden: three calls of two blocks, then the final context and the changed ring words.
    fn check(routine: Routine, v: &g::Golden, tol: f32) {
        let mut st = Mod2State::new();
        st.ctx = v.ctx_in.try_into().unwrap();
        st.ring = golden_ring();
        st.ring2 = golden_ring();
        for (n, (i, o)) in v
            .frames_in
            .chunks(8)
            .zip(v.frames_out.chunks(8))
            .enumerate()
        {
            let mut frames: Vec<f32> = i.iter().map(|w| f32::from_bits(*w)).collect();
            process(routine, &mut st, &mut frames);
            let got: Vec<u32> = frames.iter().map(|f| f.to_bits()).collect();
            assert_close(&got, o, tol, &format!("{routine:?} call {n}"));
        }
        assert_close(
            &st.ctx,
            v.ctx_out,
            tol,
            &format!("{routine:?} final context"),
        );
        check_ring(
            &changed(&st.ring),
            v.ring_diff,
            tol,
            &format!("{routine:?} ring"),
        );
        check_ring(
            &changed(&st.ring2),
            v.ring2_diff,
            tol,
            &format!("{routine:?} ring 2"),
        );
    }

    #[test]
    fn the_vibe_routine_matches_the_firmware_model() {
        check(Routine::Vibe, &g::VIBE, LOOSE);
    }

    #[test]
    fn the_flanger_routine_matches_the_firmware_model() {
        check(Routine::Flang, &g::FLANG, LOOSE);
    }

    #[test]
    fn the_first_phaser_routine_matches_the_firmware_model() {
        check(Routine::Phas0, &g::PHAS0, 5.0e-6);
    }

    #[test]
    fn the_second_phaser_routine_matches_the_firmware_model() {
        check(Routine::Phas1, &g::PHAS1, 5.0e-6);
    }

    #[test]
    fn the_ensemble_routine_matches_the_firmware_model() {
        check(Routine::Ens, &g::ENS, LOOSE);
    }

    #[test]
    fn the_spin_routine_matches_the_firmware_model() {
        check(Routine::Spin, &g::SPIN, LOOSE);
    }

    #[test]
    fn the_ring_modulator_routine_matches_the_firmware_model() {
        check(Routine::Rm, &g1::RM, 5.0e-6);
    }

    #[test]
    fn the_wah_routine_matches_the_firmware_model() {
        check(Routine::Wah, &g1::WAH, LOOSE);
    }

    #[test]
    fn the_auto_wah_routine_matches_the_firmware_model() {
        check(Routine::Awah, &g1::AWAH, LOOSE);
    }

    #[test]
    fn the_pump_routine_matches_the_firmware_model() {
        check(Routine::Pump, &g1::PUMP, 5.0e-6);
    }

    #[test]
    fn the_golden_vectors_are_not_silent() {
        for v in [&g::VIBE, &g::FLANG, &g::PHAS0, &g::PHAS1, &g::ENS, &g::SPIN] {
            assert!(v.frames_in != v.frames_out && v.ctx_in != v.ctx_out);
        }
        assert!(g::VIBE.ring_diff.len() > 1 && g::FLANG.ring_diff.len() > 1);
        assert!(g::ENS.ring_diff.len() > 1 && g::SPIN.ring_diff.len() > 1);
        assert!(!g::SPIN.ring2_diff.is_empty(), "the second ring is written");
    }

    #[test]
    fn memory_outside_the_regions_reads_zero_and_ignores_writes() {
        let mut st = Mod2State::new();
        let mut m = Mod2Mem {
            st: &mut st,
            io: [0; 4 * MAX_BLOCKS],
        };
        m.wr(0x1234, 7);
        assert_eq!(m.rd(0x1234), 0);
        m.wr(IO_BASE + 1, 5);
        assert_eq!(m.rd(IO_BASE + 1), 5);
        m.wr(SCRATCH_BASE + 1, 9);
        assert_eq!(m.rd(SCRATCH_BASE + 1), 9);
        assert_eq!(m.rd(ENS_BASE), ENS_TABLE[0], "the table is read only data");
        m.wr(ENS_BASE, 1);
        assert_eq!(m.rd(ENS_BASE), ENS_TABLE[0]);
    }
}
