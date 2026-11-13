//! Firmware compressor gain computer (FX routine `0x120630`) as an opt-in stage.
//!
//! The routine runs once per block of 4 stereo frames. Per element (left / right) it measures the
//! block peak, links the two peaks, keeps a release-smoothed level, takes `log2` of the level with
//! the `e + frac` estimate, subtracts a threshold, smooths the excess with an attack / release
//! pair, scales and links the reduction, interpolates two float bit patterns by the reduction
//! fraction (a piecewise `2^-x`) and multiplies the 4 frames by `makeup * gain`.
//!
//! VERIFIED: `tools/fx_routine_models.py` reproduces the routine word for word against the
//! `sharc-fx` oracle (900 random scenarios, 0 differing words, plus a 6-block golden scenario);
//! `tests_compressor_golden.rs` holds that golden vector and this port matches it to
//! [`GOLDEN_TOLERANCE`] (f32 arithmetic here, 40-bit registers and truncating stores there).
//! The context layout below is the firmware's (offsets from `I1`); the left threshold word is the
//! right makeup word (VERIFIED overlap). ASSUMED: which ARM parameters (`kCmpAmt`,
//! `kCmpResponse`) write which word, so [`FwCompressor::new`] only builds a neutral context.

/// Number of context words (`I1 - 0x20 ..= I1 - 0x07`).
pub const CTX_WORDS: usize = 26;
/// Offset of `I1 - 0x20` in the word array.
const BASE: i32 = 0x20;

/// Word offsets from `I1` (index = offset + 0x20). VERIFIED against the oracle; roles EMPIRICAL.
pub mod ctx {
    /// `M3`: offset of frame 2 (4 in the firmware pass).
    pub const M3: i32 = -0x13;
    /// `M4`: offset of frame 3 (6).
    pub const M4: i32 = -0x12;
    /// Pair: smoothed level state (written back).
    pub const LEVEL: i32 = -0x11;
    /// Pair: smoothed excess state (written back).
    pub const EXCESS: i32 = -0x0F;
    /// Exponent shift of the `MANT` term (-31 gives `log2 = e + frac`).
    pub const MANT_SCALE: i32 = -0x0D;
    /// `FIX` exponent shift (31 gives a Q31 reduction fraction).
    pub const FIX_SCALE: i32 = -0x0C;
    /// Multiplier of the `log2` estimate.
    pub const LOG_GAIN: i32 = -0x0B;
    /// Float bit pattern at reduction fraction 0.
    pub const TABLE0: i32 = -0x0A;
    /// Coefficient of the makeup smoother.
    pub const SMOOTH: i32 = -0x09;
    /// Pair: coefficient while the excess rises.
    pub const ATTACK: i32 = -0x20;
    /// Pair: coefficient of the level release and of the excess fall.
    pub const RELEASE: i32 = -0x1E;
    /// Pair: makeup gain target.
    pub const MAKEUP_TARGET: i32 = -0x1C;
    /// Pair: smoothed makeup gain (written back; both elements use the left word).
    pub const MAKEUP: i32 = -0x1A;
    /// Pair: threshold. The left word overlaps `MAKEUP + 1`.
    pub const THRESHOLD: i32 = -0x19;
    /// Pair: reduction per `log2` unit above the threshold.
    pub const SLOPE: i32 = -0x17;
    /// Scale (and sign) of the reduction before `FIX`.
    pub const REDUCTION_SCALE: i32 = -0x15;
    /// Float bit pattern at reduction fraction 1.
    pub const TABLE1: i32 = -0x14;
}

/// Relative tolerance of this f32 port against the 40-bit oracle golden vector (measured worst
/// case over the 48 output words: 2.1e-7, about two f32 ulp).
#[cfg(test)]
pub(super) const GOLDEN_TOLERANCE: f32 = 1.0e-6;

/// Firmware compressor state: the 26 context words. Allocation free.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FwCompressor {
    words: [u32; CTX_WORDS],
}

#[inline]
fn idx(offset: i32) -> usize {
    (offset + BASE) as usize
}

/// Float32 word to value; a zero exponent field flushes to zero like the DSP's input stage.
#[inline]
fn val(word: u32) -> f32 {
    if (word >> 23) & 0xFF == 0 {
        0.0
    } else {
        f32::from_bits(word)
    }
}

/// `Rn = FIX Fx BY scale`: round to nearest even; out of range gives all ones.
#[inline]
fn fix_by(x: f32, scale: i32) -> u32 {
    if x == 0.0 {
        return 0;
    }
    let v = f64::from(x) * 2f64.powi(scale);
    let r = v.round_ties_even();
    if !(-2147483648.0..2147483648.0).contains(&r) {
        return 0xFFFF_FFFF;
    }
    (r as i64) as u32
}

/// `Rn = Rx * Ry (SSF)`: signed fractional product, upper 32 bits of `2 a b` (floor).
#[inline]
fn ssf(a: u32, b: u32) -> u32 {
    (((i64::from(a as i32) * i64::from(b as i32)) << 1) >> 32) as u32
}

/// `log2` estimate of the level: `(e + 1) + MANT * 2^scale` (`MANT` read as a signed integer).
#[inline]
fn log2_estimate(level: f32, mant_scale: i32) -> f32 {
    let bits = level.to_bits();
    let (exp1, mant): (i32, i32) = if (bits >> 23) & 0xFF == 0 {
        (0xFF80_0000u32 as i32 + 1, 0) // LOGB of zero is -inf as a word; MANT of zero is 0
    } else {
        let e = ((bits >> 23) & 0xFF) as i32 - 127;
        (e + 1, (0x8000_0000 | (bits << 8)) as i32)
    };
    (f64::from(exp1) + f64::from(mant) * 2f64.powi(mant_scale)) as f32
}

impl Default for FwCompressor {
    fn default() -> Self {
        Self::new()
    }
}

impl FwCompressor {
    /// Neutral context: unity makeup and table, a threshold far above any level (no reduction),
    /// frame offsets 4 and 6. Every other parameter value is ASSUMED.
    pub fn new() -> Self {
        let mut c = Self {
            words: [0; CTX_WORDS],
        };
        c.set_int(ctx::M3, 4);
        c.set_int(ctx::M4, 6);
        c.set_int(ctx::MANT_SCALE, -31);
        c.set_int(ctx::FIX_SCALE, 31);
        c.set_value(ctx::LOG_GAIN, 1.0);
        c.set_word(ctx::TABLE0, 1.0f32.to_bits());
        c.set_word(ctx::TABLE1, 0.5f32.to_bits());
        c.set_value(ctx::SMOOTH, 0.9908);
        for off in [ctx::MAKEUP_TARGET, ctx::MAKEUP] {
            c.set_pair(off, 1.0);
        }
        c.set_pair(ctx::THRESHOLD, 100.0);
        c.set_pair(ctx::SLOPE, 1.0);
        c.set_value(ctx::REDUCTION_SCALE, 0.5);
        c.set_pair(ctx::ATTACK, 0.5);
        c.set_pair(ctx::RELEASE, 0.02);
        c
    }

    /// Builds a compressor from the 26 raw context words (`I1 - 0x20 ..= I1 - 0x07`).
    pub fn from_words(words: [u32; CTX_WORDS]) -> Self {
        Self { words }
    }

    /// The 26 context words, including the written-back state.
    pub fn words(&self) -> &[u32; CTX_WORDS] {
        &self.words
    }

    /// Sets one raw word at `offset` (see [`ctx`]).
    pub fn set_word(&mut self, offset: i32, word: u32) {
        self.words[idx(offset)] = word;
    }

    /// Sets a signed integer word (`M3`, `M4`, the two scale shifts).
    pub fn set_int(&mut self, offset: i32, value: i32) {
        self.set_word(offset, value as u32);
    }

    /// Sets a float word.
    pub fn set_value(&mut self, offset: i32, value: f32) {
        self.set_word(offset, value.to_bits());
    }

    /// Sets both words of a pair field to `value` (the right threshold word is `offset + 1`).
    pub fn set_pair(&mut self, offset: i32, value: f32) {
        self.set_value(offset, value);
        self.set_value(offset + 1, value);
    }

    #[inline]
    fn f(&self, offset: i32) -> f32 {
        val(self.words[idx(offset)])
    }

    #[inline]
    fn pair(&self, offset: i32) -> [f32; 2] {
        [self.f(offset), self.f(offset + 1)]
    }

    /// Processes one block in place. `block` holds 4 stereo frames as the DSP sees them
    /// (`[l0, r0, l1, r1, l2, r2, l3, r3]`); the frame offsets come from `M3` / `M4` and must be
    /// even and inside the block (otherwise 4 and 6 are used).
    pub fn process_block(&mut self, block: &mut [f32; 8]) {
        let ok = |m: i32| (0..=6).contains(&m) && m % 2 == 0;
        let m3 = self.words[idx(ctx::M3)] as i32;
        let m4 = self.words[idx(ctx::M4)] as i32;
        let offs = [0usize, 2, if ok(m3) { m3 as usize } else { 4 }, {
            if ok(m4) {
                m4 as usize
            } else {
                6
            }
        }];
        let frames: [[f32; 2]; 4] = std::array::from_fn(|k| [block[offs[k]], block[offs[k] + 1]]);
        let peak: [f32; 2] = std::array::from_fn(|c| {
            frames
                .iter()
                .map(|fr| val(fr[c].to_bits()).abs())
                .fold(0.0, f32::max)
        });
        let linked = peak[0].max(peak[1]);
        let (lvl_in, rel) = (self.pair(ctx::LEVEL), self.pair(ctx::RELEASE));
        let level: [f32; 2] = std::array::from_fn(|c| {
            let keep = lvl_in[c] - lvl_in[c] * rel[c];
            linked.max(linked * rel[c] + keep)
        });
        let (target, cur) = (self.pair(ctx::MAKEUP_TARGET), self.pair(ctx::MAKEUP));
        let k = self.f(ctx::SMOOTH);
        let makeup: [f32; 2] = std::array::from_fn(|c| target[c] + (cur[c] - target[c]) * k);
        for (c, (l, m)) in level.iter().zip(makeup).enumerate() {
            self.words[idx(ctx::LEVEL) + c] = l.to_bits();
            self.words[idx(ctx::MAKEUP) + c] = m.to_bits();
        }
        let ms = self.words[idx(ctx::MANT_SCALE)] as i32;
        // the threshold is read after the makeup store: its left word is the right makeup word
        let (thr, atk) = (self.pair(ctx::THRESHOLD), self.pair(ctx::ATTACK));
        let (lg, ex_in, slope) = (
            self.f(ctx::LOG_GAIN),
            self.pair(ctx::EXCESS),
            self.pair(ctx::SLOPE),
        );
        let excess: [f32; 2] = std::array::from_fn(|c| {
            let scaled = log2_estimate(level[c], ms) * lg;
            let over = scaled.max(thr[c]) - thr[c];
            let coef = if over > ex_in[c] { atk[c] } else { rel[c] };
            over * coef + (ex_in[c] - ex_in[c] * coef)
        });
        let red: [f32; 2] = std::array::from_fn(|c| excess[c] * slope[c]);
        let linked_red = red[0].max(red[1]) * self.f(ctx::REDUCTION_SCALE);
        let frac = fix_by(linked_red, self.words[idx(ctx::FIX_SCALE)] as i32);
        for (c, e) in excess.iter().enumerate() {
            self.words[idx(ctx::EXCESS) + c] = e.to_bits();
        }
        let (t0, t1) = (self.words[idx(ctx::TABLE0)], self.words[idx(ctx::TABLE1)]);
        let gword = t0.wrapping_sub(ssf(t0, frac)).wrapping_add(ssf(t1, frac));
        // both elements use the left makeup word (loaded through I1, broadcast)
        let gain = val(self.words[idx(ctx::MAKEUP)]) * val(gword);
        for (k, fr) in frames.iter().enumerate() {
            for (c, x) in fr.iter().enumerate() {
                block[offs[k] + c] = x * gain;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::tests_compressor_golden::{
        CMP_GOLDEN_CONTEXT, CMP_GOLDEN_CONTEXT_IN, CMP_GOLDEN_INPUT, CMP_GOLDEN_OUTPUT,
    };

    fn rel_err(a: f32, b: f32) -> f32 {
        (a - b).abs() / b.abs().max(1e-9)
    }

    #[test]
    fn fixed_point_helpers_match_the_documented_cases() {
        assert_eq!(ssf(0x4000_0000, 0x4000_0000), 0x2000_0000); // 0.5 * 0.5 = 0.25
        assert_eq!(ssf(0xC000_0000, 0x4000_0000), 0xE000_0000); // -0.5 * 0.5 = -0.25
        assert_eq!(fix_by(1.5, 0), 2);
        assert_eq!(fix_by(2.5, 0), 2); // ties to even
        assert_eq!(fix_by(-0.5, 1), 0xFFFF_FFFF);
        assert_eq!(fix_by(1.0, 31), 0xFFFF_FFFF); // out of range
        assert_eq!(fix_by(0.0, 31), 0);
        // e + frac estimate: 0.75 = 2^-1 * 1.5
        assert!((log2_estimate(0.75, -31) - (-0.5)).abs() < 1e-6);
        assert!(log2_estimate(0.0, -31) < -8.0e6); // LOGB(0) is -inf as a word
    }

    #[test]
    fn matches_the_oracle_golden_vector_over_six_blocks() {
        let mut c = FwCompressor::from_words(CMP_GOLDEN_CONTEXT_IN);
        let mut worst = 0.0f32;
        for b in 0..6 {
            let mut blk: [f32; 8] =
                std::array::from_fn(|k| f32::from_bits(CMP_GOLDEN_INPUT[8 * b + k]));
            c.process_block(&mut blk);
            for (k, y) in blk.iter().enumerate() {
                let want = f32::from_bits(CMP_GOLDEN_OUTPUT[8 * b + k]);
                worst = worst.max(rel_err(*y, want));
            }
        }
        assert!(worst < GOLDEN_TOLERANCE, "worst relative error {worst}");
        // written-back state (level, excess, makeup) agrees too
        for (i, (&got, &want)) in c.words().iter().zip(CMP_GOLDEN_CONTEXT.iter()).enumerate() {
            let (g, w) = (f32::from_bits(got), f32::from_bits(want));
            if (1..0xFF).contains(&((want >> 23) & 0xFF)) {
                assert!(
                    rel_err(g, w) < 4.0 * GOLDEN_TOLERANCE || (g - w).abs() < 1e-6,
                    "context word {i}: {g} vs {w}"
                );
            }
        }
    }

    #[test]
    fn neutral_context_passes_audio_unchanged_after_the_smoother_settles() {
        let mut c = FwCompressor::new();
        let input = [0.25, -0.5, 0.125, 0.0, -0.375, 0.5, 0.0625, -0.25];
        let mut blk = input;
        c.process_block(&mut blk);
        for (y, x) in blk.iter().zip(input) {
            assert!((y - x).abs() < 1e-6, "{y} vs {x}");
        }
    }

    #[test]
    fn loud_blocks_reduce_gain_and_the_reduction_is_linked_across_channels() {
        let mut c = FwCompressor::new();
        c.set_pair(ctx::THRESHOLD, -4.0);
        c.set_pair(ctx::ATTACK, 1.0);
        c.set_value(ctx::REDUCTION_SCALE, 0.2);
        let mut blk = [1.0, 0.001, 0.9, 0.001, 0.8, 0.001, 0.7, 0.001];
        let before = blk;
        c.process_block(&mut blk);
        let (gl, gr) = (blk[0] / before[0], blk[1] / before[1]);
        assert!(gl < 0.99 && (gl - gr).abs() < 1e-6, "{gl} {gr}");
    }

    #[test]
    fn output_is_finite_for_zero_and_extreme_input() {
        let mut c = FwCompressor::new();
        c.set_pair(ctx::THRESHOLD, -4.0);
        for x in [0.0f32, 1.0e-30, 1.0e6, -1.0e6] {
            let mut blk = [x; 8];
            c.process_block(&mut blk);
            assert!(blk.iter().all(|v| v.is_finite()));
        }
    }
}
