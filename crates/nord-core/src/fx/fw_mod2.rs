//! Frame-by-frame adapter around the firmware Mod 2 chorus routines (`kFx2Type` 4 `CHOR` and 5
//! `CHOR` variation: [`super::fw_chorus4`], [`super::fw_chorus5`]) with the words of the FX image
//! and the laws of `kFx2Amt` / `kFx2Rate` (opt-in, `FirmwareFx::mod2`).
//!
//! The routines run once per pass on 2 blocks of two stereo frames, so the stage collects 4
//! frames and releases them one call later: **latency 4 frames (83 us at 48 kHz)**. The phase
//! increment word is scaled by `FIRMWARE_FRAME_RATE_HZ / sample_rate`; the delay words are in
//! DSP frames and are not rescaled (ASSUMED: the ring is 256 frames).
//!
//! Provenance (`docs/research/fx-mod2-types.md`): the routines are VERIFIED against the oracle;
//! the static words are the image row of the synth unit L0 (dump of OS v1.66); the laws are the
//! closed forms of `docs/research/data/fx-param-fits.csv` (11 samples each, VERIFIED at the
//! samples, EMPIRICAL between them). The words the ARM writes per layer kind (`piano` values)
//! are used for every layer: ASSUMED, the organ and synth rows differ in a few constants.

use super::fw_chorus::{ChorusKind, FwChorus, CTX_I1, CTX_N, RING_BASE, RING_N};

/// Smoothing pole `k = 0.99078` of the image (`0x3F7DA3C2`).
const K: u32 = 0x3F7D_A3C2;
/// Sine polynomial `1.57033, -0.641942, 0.0716143` of the image.
const POLY: [u32; 3] = [0x3FC9_0069, 0xBF24_5655, 0x3D92_AA8A];
/// Ramp slope and knee of the soft chorus mix words (`c[-3]`, `c[-2]`): `0x3C366ACA`, 0.7071.
const MIX_SLOPE: f32 = 0.011_129;
const MIX_MAX: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// Mod 2 chorus firmware stage plus the 4-frame buffers.
#[derive(Debug, Clone)]
pub(super) struct FwMod2Stage {
    core: FwChorus,
    scale: f64,
    input: [[f32; 2]; 4],
    output: [[f32; 2]; 4],
    pos: usize,
}

impl FwMod2Stage {
    /// Stage for `variation` (type 5), `kFx2Rate` and `kFx2Amt` in `0..=127`, at `sample_rate`.
    pub(super) fn new(variation: bool, rate: u32, amount: u32, fs: f32) -> Self {
        let kind = if variation {
            ChorusKind::Variation
        } else {
            ChorusKind::Soft
        };
        let mut core = FwChorus::new(kind, [0; CTX_N], [0; RING_N]);
        Self::base_words(&mut core);
        let mut st = Self {
            core,
            scale: crate::organ::FIRMWARE_FRAME_RATE_HZ / f64::from(fs),
            input: [[0.0; 2]; 4],
            output: [[0.0; 2]; 4],
            pos: 0,
        };
        st.set_params(rate, amount);
        st.settle();
        st
    }

    /// Whether the stage runs the variation routine.
    pub(super) fn variation(&self) -> bool {
        self.core.kind() == ChorusKind::Variation
    }

    /// The image row and the words the type switch writes (`piano` kind).
    fn base_words(c: &mut FwChorus) {
        let self_ptr = CTX_I1 - 0x18;
        c.set_word(-0x16, self_ptr);
        match c.kind() {
            ChorusKind::Soft => {
                c.set_word(-9, RING_BASE);
                c.set_word(-7, RING_BASE);
                c.set_value(-0x1F, 35.0);
                c.set_value(-0x19, 62.0);
                for (o, w) in [0xC, 0xD, 0xE].into_iter().zip(POLY) {
                    c.set_word(o, w);
                }
                for (o, v) in [(0xF, 1.0), (0x10, -1.0), (0x11, 0.5)] {
                    c.set_value(o, v);
                }
                c.set_word(0xA, 1);
                c.set_word(0x14, K);
            }
            ChorusKind::Variation => {
                c.set_word(-0xF, RING_BASE);
                c.set_word(-0xD, RING_BASE);
                c.set_value(-9, 1.0);
                c.set_value(-8, 1.0);
                for (o, v) in [(3, 1.0), (4, -1.0), (5, 0.5)] {
                    c.set_value(o, v);
                }
                c.set_word(6, K);
                // Per layer constants written by the type switch (piano row).
                for (o, w) in [
                    (7, 0x3F7A_8EEC),
                    (8, 0x3DB0_9D72),
                    (9, 0x3F75_1DD9),
                    (10, 0xBF53_D8A3),
                    (15, 0x3F69_EC52),
                    (16, 0x3F53_D8A3),
                    (19, 0x3CAE_2280),
                    (20, 0xBF75_1DD9),
                ] {
                    c.set_word(o, w);
                }
            }
        }
    }

    /// Writes the targets of `kFx2Rate` and `kFx2Amt`; the stage's smoothers glide there.
    pub(super) fn set_params(&mut self, rate: u32, amount: u32) {
        let (rate, amount) = (rate.min(127), amount.min(127));
        let vf = amount as f32;
        let scaled = |word: f64| (word * self.scale).round() as u32;
        match self.core.kind() {
            ChorusKind::Soft => {
                // VERIFIED series words: rate `12800 + 4096 v`; mix ramps and delay ramps are
                // EMPIRICAL between the 11 captured amounts (knee at 63.5).
                self.core
                    .set_word(-0x20, scaled(f64::from(12_800 + 4096 * rate)));
                let ramp = ((vf - 63.5) / 63.5).clamp(0.0, 1.0);
                self.core.set_value(-0x1E, 117.0 + 39.0 * ramp);
                self.core.set_value(-0x1D, 171.0 + 9.0 * ramp);
                self.core.set_value(-3, (vf * MIX_SLOPE).min(MIX_MAX));
                self.core.set_value(
                    -2,
                    ((vf - MIX_MAX / MIX_SLOPE) * MIX_SLOPE).clamp(0.0, MIX_MAX),
                );
                self.core.set_value(1, 1.0 - 0.002_307_086_5 * vf);
            }
            ChorusKind::Variation => {
                let r = rate as f64;
                self.core.set_word(
                    -0x1D,
                    scaled(98_956.913_8 - 0.018_364_105_8 * r + 92.030_350_1 * r * r),
                );
                let (hi, lo) = (f32::from_bits(0x3F7F_F2E5), f32::from_bits(0x3F7F_3B64));
                self.core
                    .set_value(-0x1E, hi + (lo - hi) * rate as f32 / 127.0);
                self.core.set_value(-0x1C, 116.023_62 - 0.636_121_3 * vf);
                self.core.set_value(-0x1B, 17.952_756 + 1.272_242_5 * vf);
            }
        }
    }

    /// Copies every target to its smoother state (a program load, no glide).
    fn settle(&mut self) {
        let c = &mut self.core;
        let copies: &[(i32, i32)] = match c.kind() {
            ChorusKind::Soft => &[
                (-0x1B, -0x1E),
                (-0x1A, -0x1D),
                (-1, -3),
                (0, -2),
                (2, 1),
                (3, 1),
            ],
            ChorusKind::Variation => &[(-0x1A, -0x1C), (-0x19, -0x1B), (-5, -6), (-4, -6)],
        };
        for &(dst, src) in copies {
            c.set_word(dst, c.word(src));
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
            self.core.process(&mut block);
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

    fn run(variation: bool, rate: u32, amount: u32, n: usize) -> Vec<[f32; 2]> {
        let mut st = FwMod2Stage::new(variation, rate, amount, 48_000.0);
        (0..n)
            .map(|i| st.process_frame([((i as f32) * 0.05).sin(), ((i as f32) * 0.031).cos()]))
            .collect()
    }

    #[test]
    fn both_routines_run_finite_and_the_first_block_is_silent() {
        for variation in [false, true] {
            let out = run(variation, 60, 100, 4000);
            assert!(out
                .iter()
                .all(|f| f.iter().all(|x| x.is_finite() && x.abs() < 8.0)));
            assert!(
                out[..4].iter().all(|f| *f == [0.0, 0.0]),
                "latency of one call"
            );
            assert!(
                out.iter().skip(4).any(|f| f[0] != 0.0),
                "variation {variation}"
            );
        }
    }

    #[test]
    fn the_mix_ramps_reach_the_captured_end_values() {
        let st = FwMod2Stage::new(false, 0, 127, 48_000.0);
        assert!(
            (st.core.word(-3) as i32 - 0x3F34_FDF4).abs() < 0x1000,
            "c[-3] end value"
        );
        assert!((f32::from_bits(st.core.word(-2)) - MIX_MAX).abs() < 1.0e-3);
        assert!((f32::from_bits(st.core.word(-0x1E)) - 156.0).abs() < 1.0e-3);
        let v = FwMod2Stage::new(true, 127, 127, 48_000.0);
        assert!(
            (f32::from_bits(v.core.word(-0x1C)) - 35.2).abs() < 0.1,
            "0x420cf1e4"
        );
        assert!(v.variation() && !st.variation());
    }

    #[test]
    fn a_faster_rate_word_gives_a_faster_phase() {
        let phase = |rate| {
            let mut st = FwMod2Stage::new(false, rate, 50, 48_000.0);
            for _ in 0..400 {
                st.process_frame([0.1, 0.1]);
            }
            st.core.word(-0x18)
        };
        assert_ne!(phase(0), phase(100));
    }
}
