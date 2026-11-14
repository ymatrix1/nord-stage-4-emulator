//! Frame-by-frame adapter around the firmware Mod 1 stages that are verified against the oracle:
//! the auto-pan ([`super::FwAutoPan`], routine `0x122D93`, `Fx1` types 0 and 1) and the tremolo
//! ([`super::FwAllpass`], routine `0x122C39`, types 2 and 3) with the instance words of the FX
//! image and the words `kFx1Amt` / `kFx1RateClkDiv` write (opt-in, `FirmwareFx::mod1`).
//!
//! The routines run once per pass on 2 blocks of two stereo frames, so the stage collects 4
//! frames, processes them and releases them one block later: **latency 4 frames (83 us at
//! 48 kHz)**, the first 4 output frames are silence. The phase increment word is scaled by
//! `FIRMWARE_FRAME_RATE_HZ / sample_rate` so the LFO frequency in Hz is the firmware's.
//!
//! Provenance of the words (`docs/research/fx-mod-types.md` s3): the static words are the initial
//! DM of the FX image (`c[-0xB..-9]` polynomial, `-31` shift, `k = 0.99078`, ...), VERIFIED by a
//! dump of the image (`tools/fx_mod_words.py`); the parameter laws are VERIFIED series words
//! (`docs/research/data/fx-param-fits.csv`); the role of types 2 and 3 as tremolo is EMPIRICAL
//! (the gain range `[1 - v / 127, 1]` follows from the words, the biquad mix `c[2]` stays 0).

use super::fw_allpass::{ctx as ap, FwAllpass};
use super::fw_autopan::{ctx as pan, FwAutoPan};
use super::fw_map;

/// Which routine the stage runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mod1Shape {
    /// `Fx1` types 0 (soft) and 1 (hard).
    AutoPan,
    /// `Fx1` types 2 (soft) and 3 (hard).
    Tremolo,
}

/// The firmware routine and its words.
#[derive(Debug, Clone, Copy)]
enum Core {
    Pan(FwAutoPan),
    Trem(FwAllpass),
}

/// Mod 1 firmware stage plus the 4-frame buffers.
#[derive(Debug, Clone, Copy)]
pub(super) struct FwMod1Stage {
    core: Core,
    hard: bool,
    scale: f64,
    input: [[f32; 2]; 4],
    output: [[f32; 2]; 4],
    pos: usize,
}

/// `kFx1Amt` word of the depth: soft types `v / 254` (tremolo) or `v / 508` (auto-pan); hard types
/// `v / 127` (auto-pan) or `8 v / 254` (tremolo). VERIFIED series, `v = 1` is the captured value
/// (`1 / 508` or `1 / 254`) and equals the linear law for the soft types.
fn depth_word(shape: Mod1Shape, hard: bool, v: u32) -> f32 {
    let x = v as f32;
    match (shape, hard) {
        (Mod1Shape::AutoPan, false) => x / 508.0,
        (Mod1Shape::AutoPan, true) if v == 1 => 1.0 / 508.0,
        (Mod1Shape::AutoPan, true) => x / 127.0,
        (Mod1Shape::Tremolo, false) => x / 254.0,
        (Mod1Shape::Tremolo, true) if v == 1 => 1.0 / 254.0,
        (Mod1Shape::Tremolo, true) => 8.0 * x / 254.0,
    }
}

impl FwMod1Stage {
    /// Stage for `shape` (`hard` = odd type), `kFx1RateClkDiv` and `kFx1Amt` in `0..=127`, at
    /// `sample_rate`. The smoothers start settled on the targets.
    pub(super) fn new(shape: Mod1Shape, hard: bool, rate: u32, amount: u32, fs: f32) -> Self {
        let scale = crate::organ::FIRMWARE_FRAME_RATE_HZ / f64::from(fs);
        let core = match shape {
            Mod1Shape::AutoPan => Core::Pan(Self::pan_base(hard)),
            Mod1Shape::Tremolo => Core::Trem(Self::trem_base(hard)),
        };
        let mut st = Self {
            core,
            hard,
            scale,
            input: [[0.0; 2]; 4],
            output: [[0.0; 2]; 4],
            pos: 0,
        };
        st.set_params(rate, amount);
        st.settle();
        st
    }

    /// The shape this stage runs.
    pub(super) fn shape(&self) -> Mod1Shape {
        match self.core {
            Core::Pan(_) => Mod1Shape::AutoPan,
            Core::Trem(_) => Mod1Shape::Tremolo,
        }
    }

    /// Whether the hard (variation) type is loaded.
    pub(super) fn hard(&self) -> bool {
        self.hard
    }

    fn pan_base(hard: bool) -> FwAutoPan {
        let mut c = FwAutoPan::default();
        c.set_word(pan::SCALE, 1);
        // The type switch writes 0xC0000000 for type 1; the image holds 0x40000000 (type 0).
        c.set_word(pan::OFFSET, if hard { 0xC000_0000 } else { 0x4000_0000 });
        c.set_value(pan::B_TARGET, 1.0);
        for (o, w) in pan::POLY.into_iter().zip(POLY) {
            c.set_word(o, w);
        }
        c.set_word(pan::SHIFT, (-31i32) as u32);
        c.set_word(pan::SMOOTH, K_WORD);
        c.set_value(pan::PRE_GAIN, 4.0);
        c.set_value(pan::TRI_OFFSET, -1.0);
        c.set_value(pan::POST_GAIN, 0.5);
        c.set_word(pan::BLOCKS, 2);
        c
    }

    fn trem_base(hard: bool) -> FwAllpass {
        let _ = hard;
        let mut c = FwAllpass::default();
        c.set_word(ap::COUNTER_SCALE, 1);
        c.set_word(ap::COUNTER_OFFSET, 0xC000_0000);
        for (o, w) in ap::POLY.into_iter().zip(POLY) {
            c.set_word(o, w);
        }
        // Far (low-pass) and near (high-pass) one-pole sections of the image; unused at mix 0.
        for (o, v) in [(-0x10, 0.080_103), (-0xF, 0.080_103), (-0xD, 0.839_794)] {
            c.set_value(o, v);
        }
        for (o, v) in [(-7, 0.919_897), (-6, -0.919_897), (-4, 0.839_794)] {
            c.set_value(o, v);
        }
        c.set_word(ap::SMOOTH, K_WORD);
        c.set_value(ap::MIX, 0.0);
        c.set_word(4, 2);
        c
    }

    /// Writes the targets of `kFx1RateClkDiv` and `kFx1Amt`; the stage's smoothers glide there.
    pub(super) fn set_params(&mut self, rate: u32, amount: u32) {
        let (rate, amount) = (rate.min(127), amount.min(127));
        let word = (f64::from(fw_map::allpass_rate(rate)) * self.scale).round() as u32;
        let (hard, shape) = (self.hard, self.shape());
        let depth = depth_word(shape, hard, amount);
        let v = amount as f32;
        match &mut self.core {
            Core::Pan(c) => {
                c.set_word(pan::RATE, word);
                c.set_value(pan::A_TARGET, depth);
                c.set_value(pan::P_TARGET, 1.0 - v / 254.0);
                c.set_value(pan::B_TARGET, if hard { depth } else { 1.0 });
            }
            Core::Trem(c) => {
                c.set_word(ap::RATE, word);
                c.set_value(ap::A_TARGET, depth);
                c.set_value(ap::A_TARGET + 1, 1.0 - v / 254.0);
                c.set_value(ap::RIGHT_LFO, if hard { v / 254.0 } else { 0.5 });
            }
        }
    }

    /// Copies every target to its smoother state (a program load, no glide).
    fn settle(&mut self) {
        match &mut self.core {
            Core::Pan(c) => {
                let (a, p, b) = (
                    c.value(pan::A_TARGET),
                    c.value(pan::P_TARGET),
                    c.value(pan::B_TARGET),
                );
                for (o, v) in [(pan::A_STATE, a), (pan::A_USED, a), (pan::P_STATE, p)] {
                    c.set_value(o, v);
                }
                c.set_value(pan::P_USED, p);
                c.set_value(pan::B_STATE, b);
            }
            Core::Trem(c) => {
                let (al, ar, s) = (
                    c.value(ap::A_TARGET),
                    c.value(ap::A_TARGET + 1),
                    c.value(ap::RIGHT_LFO),
                );
                for (o, v) in [(-0x18, al), (-0x17, ar), (ap::S_STATE + 1, s)] {
                    c.set_value(o, v);
                }
            }
        }
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
            match &mut self.core {
                Core::Pan(c) => c.process(&mut block),
                Core::Trem(c) => {
                    // The pass pointer addresses the phase pair itself and the routine reads it
                    // after adding the rate.
                    let t = c.words()[(ap::PHASE + 0x20) as usize]
                        .wrapping_add(c.words()[(ap::RATE + 0x20) as usize]);
                    c.process([t, t], &mut block);
                }
            }
            self.output = std::array::from_fn(|k| [block[2 * k], block[2 * k + 1]]);
        }
        out
    }
}

/// VERIFIED: sine polynomial `a1 a3 a5` of the FX image (static DM, core 1 `0xC151D..0xC151F`
/// and `0xC15CB..`), 1.57033, -0.641942, 0.0716143.
const POLY: [u32; 3] = [0x3FC9_0069, 0xBF24_5655, 0x3D92_AA8A];
/// VERIFIED: smoothing coefficient 0.99078 = `0x3F7DA3C2`, the same word as the delay's `K`.
const K_WORD: u32 = 0x3F7D_A3C2;

#[cfg(test)]
mod tests {
    use super::*;

    fn run(shape: Mod1Shape, hard: bool, rate: u32, amt: u32, n: usize) -> Vec<[f32; 2]> {
        let mut st = FwMod1Stage::new(shape, hard, rate, amt, 48_000.0);
        (0..n).map(|_| st.process_frame([1.0, 1.0])).collect()
    }

    fn range(v: &[[f32; 2]], ch: usize) -> (f32, f32) {
        let it = v.iter().map(|f| f[ch]);
        (
            it.clone().fold(f32::MAX, f32::min),
            it.fold(f32::MIN, f32::max),
        )
    }

    #[test]
    fn the_output_is_delayed_by_one_block() {
        let mut st = FwMod1Stage::new(Mod1Shape::AutoPan, false, 64, 127, 48_000.0);
        let outs: Vec<[f32; 2]> = (0..8).map(|_| st.process_frame([1.0, 1.0])).collect();
        assert!(outs[..4].iter().all(|f| *f == [0.0, 0.0]));
        assert!(outs[4..].iter().all(|f| f[0] > 0.0 || f[1] > 0.0));
    }

    #[test]
    fn auto_pan_gains_stay_in_the_one_minus_amount_to_one_range() {
        for hard in [false, true] {
            // Full depth, fast rate: both channels sweep 0..1 against each other.
            let y = &run(Mod1Shape::AutoPan, hard, 127, 127, 48_000)[2_000..];
            let (lo, hi) = range(y, 0);
            assert!(lo >= -0.01 && hi <= 1.01, "hard {hard}: {lo} {hi}");
            assert!(lo < 0.1 && hi > 0.9, "hard {hard} sweeps fully: {lo} {hi}");
            // g_L + g_R = 2 P = 1 at full depth.
            assert!(y.iter().all(|f| (f[0] + f[1] - 1.0).abs() < 1.0e-3));
        }
        // Zero depth is unity on both channels.
        let y = &run(Mod1Shape::AutoPan, false, 64, 0, 2_000)[100..];
        assert!(y
            .iter()
            .all(|f| (f[0] - 1.0).abs() < 1.0e-5 && (f[1] - 1.0).abs() < 1.0e-5));
    }

    #[test]
    fn the_hard_pan_is_closer_to_a_square_than_the_soft_one() {
        let at_extremes = |hard| {
            let y = &run(Mod1Shape::AutoPan, hard, 100, 127, 96_000)[2_000..];
            let near = y.iter().filter(|f| f[0] < 0.1 || f[0] > 0.9).count();
            near as f32 / y.len() as f32
        };
        assert!(at_extremes(true) > at_extremes(false) + 0.2);
    }

    #[test]
    fn tremolo_has_maximum_level_one_and_dips_to_one_minus_amount() {
        for hard in [false, true] {
            let y = &run(Mod1Shape::Tremolo, hard, 127, 127, 48_000)[2_000..];
            let (lo, hi) = range(y, 0);
            assert!(hi <= 1.01 && hi > 0.95, "hard {hard}: {hi}");
            assert!((-0.01..0.1).contains(&lo), "hard {hard}: {lo}");
            assert!(y.iter().all(|f| f[0] == f[1]), "the tremolo gain is mono");
        }
        let y = &run(Mod1Shape::Tremolo, false, 64, 0, 2_000)[100..];
        assert!(y.iter().all(|f| (f[0] - 1.0).abs() < 1.0e-5));
    }

    #[test]
    fn the_lfo_frequency_follows_the_phase_increment_word() {
        // rate 127: 2,093,056 counts per call of 4 frames at the 43,402.34 Hz frame rate.
        let want = 2_093_056.0 / 4_294_967_296.0 * 43_402.343_75 / 4.0;
        let y = run(Mod1Shape::Tremolo, false, 127, 127, 96_000);
        let mid = 0.5;
        let up = y
            .windows(2)
            .skip(4_000)
            .filter(|w| w[0][0] < mid && w[1][0] >= mid)
            .count() as f64;
        let measured = up / (92_000.0 / 48_000.0);
        assert!(
            (measured - want).abs() < 0.05 * want,
            "{measured} vs {want}"
        );
    }

    #[test]
    fn depth_words_follow_the_verified_series() {
        assert_eq!(depth_word(Mod1Shape::AutoPan, false, 127), 0.25);
        assert_eq!(depth_word(Mod1Shape::AutoPan, true, 127), 1.0);
        assert_eq!(
            depth_word(Mod1Shape::AutoPan, true, 1).to_bits(),
            0x3b01_0204
        );
        assert_eq!(depth_word(Mod1Shape::Tremolo, false, 127), 0.5);
        assert_eq!(depth_word(Mod1Shape::Tremolo, true, 127), 4.0);
        assert_eq!(
            depth_word(Mod1Shape::Tremolo, true, 1).to_bits(),
            0x3b81_0204
        );
    }

    #[test]
    fn the_stage_reports_its_shape_and_type() {
        let st = FwMod1Stage::new(Mod1Shape::Tremolo, true, 10, 10, 44_100.0);
        assert!(st.hard() && st.shape() == Mod1Shape::Tremolo);
    }
}
