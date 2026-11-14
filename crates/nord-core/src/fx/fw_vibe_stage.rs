//! Frame-by-frame adapter around the firmware `VIBE` routine ([`super::fw_vibe`], `kFx2Type`
//! 6 and 7, `0x123C84`) with the words of the FX image and the laws of `kFx2Amt` / `kFx2Rate`
//! (opt-in, `FirmwareFx::mod2`).
//!
//! The routine runs once per pass on 2 blocks of two stereo frames, so the stage collects 4
//! frames and releases them one call later: **latency 4 frames (83 us at 48 kHz)**. The phase
//! increment word is scaled by `FIRMWARE_FRAME_RATE_HZ / sample_rate`; the delay words are in DSP
//! frames and are not rescaled (ASSUMED: the ring is 512 frames).
//!
//! Provenance (`docs/research/fx-mod2-types.md` s6): the routine is VERIFIED against the oracle;
//! the static words are the image row of the piano / synth unit (`0xC17A0`, `0xC16FE`, dump of
//! OS v1.66) and the words of the type switch (`fx-type-words.csv`, piano kind, used for every
//! layer kind: ASSUMED); type 7 (variation) writes only three words, so it uses the words of
//! type 6 (ASSUMED, OQ21); the laws are the closed forms of `fx-param-fits.csv` (11 samples
//! each: VERIFIED at the samples).

use super::fw_chorus::{CTX_I1, CTX_N, RING_BASE, RING_N};
use super::fw_mod2_run::{process, Routine};

/// Image row words (VERIFIED by dump, synth and piano unit rows agree): offset, word.
const IMAGE: [(i32, u32); 4] = [
    (-5, 0x3F80_0000),
    (-4, 0xBF80_0000),
    (-1, 0x3D80_0000),
    (0, 0x3D9F_FF79),
];
/// Words of the type switch, piano kind (`fx-type-words.csv`): offset, word. `c[-2]` (depth) and
/// `c[-23]` (phase increment) are the parameter targets and set by [`FwVibeStage::set_params`].
const TYPE_WORDS: [(i32, u32); 10] = [
    (-3, 0),
    (-8, 0xBF75_1DD9),
    (-9, 0x3CAE_2280),
    (-12, 0x3F53_D8A3),
    (-13, 0x3F69_EC52),
    (-18, 0xBF53_D8A3),
    (-19, 0x3F75_1DD9),
    (-20, 0x3DB0_9D72),
    (-21, 0x3F7A_8EEC),
    (3, 0x3FC9_006A),
];
/// Image words `c[2]` (`0x3FBE0029`), the delay `c[-0x20]` (`-600` words) of the image row.
const GAIN_WORD: u32 = 0x3FBE_0029;
const DELAY_WORD: u32 = (-600i32) as u32;

/// Firmware `VIBE` stage plus the 4-frame buffers.
#[derive(Debug, Clone)]
pub(super) struct FwVibeStage {
    ctx: [u32; CTX_N],
    ring: [u32; RING_N],
    scale: f64,
    input: [[f32; 2]; 4],
    output: [[f32; 2]; 4],
    pos: usize,
}

fn index(offset: i32) -> usize {
    (offset + 0x20) as usize
}

/// `c[-2]`, the depth: `v / 128`, the captured word `0x3F7FFFFE` at `v = 127` (VERIFIED at the 11
/// samples of `fx-param-fits.csv`).
fn depth_word(amount: u32) -> u32 {
    if amount >= 127 {
        0x3F7F_FFFE
    } else {
        (amount as f32 / 128.0).to_bits()
    }
}

/// `c[-23]`: `0x3200 + 512 floor(v^2 / 4)` (VERIFIED at the 11 samples, exact integers).
fn rate_word(rate: u32) -> u32 {
    0x3200 + 512 * ((rate * rate) >> 2)
}

impl FwVibeStage {
    /// Stage for `kFx2Rate` and `kFx2Amt` in `0..=127`, at `sample_rate`.
    pub(super) fn new(rate: u32, amount: u32, fs: f32) -> Self {
        let mut ctx = [0u32; CTX_N];
        for (o, w) in IMAGE.into_iter().chain(TYPE_WORDS) {
            ctx[index(o)] = w;
        }
        ctx[index(2)] = GAIN_WORD;
        ctx[index(-0x20)] = DELAY_WORD;
        ctx[index(-0x1F)] = RING_BASE;
        ctx[index(-0x1E)] = RING_BASE;
        ctx[index(-0x16)] = CTX_I1 - 0x19;
        let mut st = Self {
            ctx,
            ring: [0; RING_N],
            scale: crate::organ::FIRMWARE_FRAME_RATE_HZ / f64::from(fs),
            input: [[0.0; 2]; 4],
            output: [[0.0; 2]; 4],
            pos: 0,
        };
        st.set_params(rate, amount);
        st
    }

    /// Writes the targets of `kFx2Rate` and `kFx2Amt` (the routine has no smoothers here).
    pub(super) fn set_params(&mut self, rate: u32, amount: u32) {
        let inc = (f64::from(rate_word(rate.min(127))) * self.scale).round() as u32;
        self.ctx[index(-0x17)] = inc;
        self.ctx[index(-2)] = depth_word(amount.min(127));
    }

    /// Pushes one frame and returns the frame of the previous block at the same position.
    #[inline]
    pub(super) fn process_frame(&mut self, frame: [f32; 2]) -> [f32; 2] {
        let out = self.output[self.pos];
        self.input[self.pos] = frame;
        self.pos += 1;
        if self.pos == 4 {
            self.pos = 0;
            let mut block: [f32; 8] = std::array::from_fn(|i| self.input[i / 2][i % 2]);
            process(Routine::Vibe, &mut self.ctx, &mut self.ring, &mut block);
            for (i, o) in self.output.iter_mut().enumerate() {
                *o = [block[2 * i], block[2 * i + 1]];
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(rate: u32, amount: u32, n: usize) -> Vec<[f32; 2]> {
        let mut st = FwVibeStage::new(rate, amount, 48_000.0);
        (0..n)
            .map(|i| st.process_frame([((i as f32) * 0.05).sin(), ((i as f32) * 0.031).cos()]))
            .collect()
    }

    #[test]
    fn the_laws_hit_the_captured_samples() {
        assert_eq!(rate_word(0), 0x3200);
        assert_eq!(rate_word(1), 0x3200);
        assert_eq!(rate_word(2), 0x3400);
        assert_eq!(rate_word(15), 0xA200);
        assert_eq!(rate_word(63), 0x7_F200);
        assert_eq!(rate_word(127), 0x1F_B200);
        assert_eq!(depth_word(0), 0);
        assert_eq!(depth_word(15), 0x3DF0_0000);
        assert_eq!(depth_word(63), 0x3EFC_0000);
        assert_eq!(depth_word(127), 0x3F7F_FFFE);
    }

    #[test]
    fn the_stage_runs_finite_with_a_four_frame_latency() {
        for (rate, amount) in [(0, 0), (60, 100), (127, 127)] {
            let out = run(rate, amount, 6000);
            assert!(out
                .iter()
                .all(|f| f.iter().all(|x| x.is_finite() && x.abs() < 8.0)));
            assert!(out[..4].iter().all(|f| *f == [0.0, 0.0]), "latency");
            assert!(out.iter().skip(4).any(|f| f[0] != 0.0), "{rate} {amount}");
        }
    }

    #[test]
    fn a_faster_rate_word_gives_a_faster_phase() {
        let phase = |rate| {
            let mut st = FwVibeStage::new(rate, 50, 48_000.0);
            for _ in 0..400 {
                st.process_frame([0.1, 0.1]);
            }
            st.ctx[index(-0x19)]
        };
        assert_ne!(phase(0), phase(100));
    }

    #[test]
    fn the_depth_changes_the_output() {
        let a = run(60, 10, 3000);
        let b = run(60, 120, 3000);
        assert_ne!(a, b);
    }
}
