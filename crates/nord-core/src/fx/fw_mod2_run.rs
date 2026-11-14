//! Driver of the generated Mod 2 routine ports ([`super::fw_vibe`], [`super::fw_flang`],
//! [`super::fw_phas0`], [`super::fw_phas1`]): the memory map and one call on a frame block.
//!
//! The routines keep their state in the context words around `I1` (phase, filter states,
//! smoothers) and, for the delay routines, a circular ring of `0x400` words that the ARM
//! allocates. Here the context, the ring and one small frame block are three arrays at the
//! addresses of the model scenarios (`tools/fx_model_mod2.py`), so a context captured from the DSP
//! loads unchanged and the written-back words compare word for word with the golden vectors.
//!
//! VERIFIED: every routine against the `sharc-fx` oracle (`docs/research/fx-mod2-types.md`).

use super::fw_chorus::{CTX_I1, CTX_LO, CTX_N, MAX_BLOCKS, RING_BASE, RING_N};
use super::fw_pe::{Mem, Pe};
use super::{fw_flang, fw_phas0, fw_phas1, fw_vibe};

/// Frame block (`I0`).
const IO_BASE: u32 = 0xB1000;

/// Which routine runs. FLANG and PHAS are verified ports without a wired stage yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(super) enum Routine {
    /// `kFx2Type` 6, 7: `0x123C84`.
    Vibe,
    /// `kFx2Type` 2, 3: `0x12348D`.
    Flang,
    /// `kFx2Type` 0: `0x12379B`.
    Phas0,
    /// `kFx2Type` 1: `0x1238FC`.
    Phas1,
}

impl Routine {
    /// Context offset of the block count word (`lcntr`).
    pub(super) const fn blocks_offset(self) -> i32 {
        match self {
            Self::Vibe => 0x14,
            Self::Flang => 2,
            Self::Phas0 => 0x1B,
            Self::Phas1 => 4,
        }
    }
}

/// The three memory regions of one call.
struct Mod2Mem<'a> {
    ctx: &'a mut [u32; CTX_N],
    ring: &'a mut [u32; RING_N],
    io: [u32; 4 * MAX_BLOCKS],
}

fn slot(addr: u32, lo: u32, n: usize) -> Option<usize> {
    let k = addr.wrapping_sub(lo) as usize;
    (k < n).then_some(k)
}

impl Mem for Mod2Mem<'_> {
    fn rd(&self, a: u32) -> u32 {
        if let Some(k) = slot(a, CTX_LO, CTX_N) {
            self.ctx[k]
        } else if let Some(k) = slot(a, RING_BASE, RING_N) {
            self.ring[k]
        } else if let Some(k) = slot(a, IO_BASE, self.io.len()) {
            self.io[k]
        } else {
            0
        }
    }

    fn wr(&mut self, a: u32, w: u32) {
        if let Some(k) = slot(a, CTX_LO, CTX_N) {
            self.ctx[k] = w;
        } else if let Some(k) = slot(a, RING_BASE, RING_N) {
            self.ring[k] = w;
        } else if let Some(k) = slot(a, IO_BASE, self.io.len()) {
            self.io[k] = w;
        }
    }
}

/// One call on `frames = [l0, r0, l1, r1, ...]` in place: whole blocks of two stereo frames (a
/// trailing partial block is left untouched). The routine's block count word is set to the number
/// of blocks.
pub(super) fn process(
    routine: Routine,
    ctx: &mut [u32; CTX_N],
    ring: &mut [u32; RING_N],
    frames: &mut [f32],
) {
    let n = (frames.len() / 4).min(MAX_BLOCKS);
    ctx[(routine.blocks_offset() + 0x20) as usize] = n as u32;
    let mut io = [0u32; 4 * MAX_BLOCKS];
    for (w, f) in io.iter_mut().zip(frames.iter()).take(4 * n) {
        *w = f.to_bits();
    }
    let mut pe = Pe::new(Mod2Mem { ctx, ring, io }, IO_BASE, CTX_I1);
    match routine {
        Routine::Vibe => fw_vibe::run(&mut pe),
        Routine::Flang => fw_flang::run(&mut pe),
        Routine::Phas0 => fw_phas0::run(&mut pe),
        Routine::Phas1 => fw_phas1::run(&mut pe),
    }
    for (f, w) in frames.iter_mut().zip(pe.mem.io).take(4 * n) {
        *f = f32::from_bits(w);
    }
}

#[cfg(test)]
mod tests {
    use super::super::fw_words::assert_close;
    use super::super::tests_mod2_golden as g;
    use super::*;

    /// The deterministic ring of `tools/fx_golden_mod2.py::ring_word`.
    fn golden_ring() -> [u32; RING_N] {
        std::array::from_fn(|k| {
            let h = (k as u32).wrapping_mul(2_654_435_761).wrapping_add(12345);
            ((h >> 9) as f32 / 4_194_304.0 - 1.0).to_bits()
        })
    }

    /// Relative tolerance of the delay routines: f32 against the 40-bit registers of the firmware
    /// (measured differences up to 2e-5 after three calls with a feedback path).
    const LOOSE: f32 = 1.0e-4;

    /// Runs a golden: three calls of two blocks, then the final context and the changed ring words.
    fn check(routine: Routine, v: &g::Golden, tol: f32) {
        let mut ctx: [u32; CTX_N] = v.ctx_in.try_into().unwrap();
        let mut ring = golden_ring();
        for (n, (i, o)) in v
            .frames_in
            .chunks(8)
            .zip(v.frames_out.chunks(8))
            .enumerate()
        {
            let mut frames: Vec<f32> = i.iter().map(|w| f32::from_bits(*w)).collect();
            process(routine, &mut ctx, &mut ring, &mut frames);
            let got: Vec<u32> = frames.iter().map(|f| f.to_bits()).collect();
            assert_close(&got, o, tol, &format!("{routine:?} call {n}"));
        }
        assert_close(&ctx, v.ctx_out, tol, &format!("{routine:?} final context"));
        let want = golden_ring();
        let changed: Vec<usize> = (0..RING_N).filter(|&k| ring[k] != want[k]).collect();
        assert_eq!(
            changed,
            v.ring_diff.iter().map(|d| d.0 as usize).collect::<Vec<_>>(),
            "{routine:?}: the same ring words are written"
        );
        let got: Vec<u32> = v.ring_diff.iter().map(|d| ring[d.0 as usize]).collect();
        let exp: Vec<u32> = v.ring_diff.iter().map(|d| d.1).collect();
        assert_close(&got, &exp, tol, &format!("{routine:?} ring words"));
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
    fn the_golden_vectors_are_not_silent() {
        for v in [&g::VIBE, &g::FLANG, &g::PHAS0, &g::PHAS1] {
            assert!(v.frames_in != v.frames_out && v.ctx_in != v.ctx_out);
        }
        assert!(g::VIBE.ring_diff.len() > 1 && g::FLANG.ring_diff.len() > 1);
    }

    #[test]
    fn memory_outside_the_regions_reads_zero_and_ignores_writes() {
        let (mut ctx, mut ring) = ([0u32; CTX_N], [0u32; RING_N]);
        let mut m = Mod2Mem {
            ctx: &mut ctx,
            ring: &mut ring,
            io: [0; 4 * MAX_BLOCKS],
        };
        m.wr(0x1234, 7);
        assert_eq!(m.rd(0x1234), 0);
        m.wr(IO_BASE + 1, 5);
        assert_eq!(m.rd(IO_BASE + 1), 5);
    }
}
