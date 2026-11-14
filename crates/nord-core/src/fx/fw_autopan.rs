//! Firmware auto-pan stage (FX routine `0x122D93`, module `0xC02BAF54`, `Fx1` types 0 and 1,
//! the A-PAN of Mod 1) as an opt-in stage.
//!
//! A free-running phase word (the pass pointer `ctx[-0x1B]` points at the context's own
//! `ctx[-0x1F]`, VERIFIED in the boot log of the FX image) advances by `RATE` per call. The signed
//! phase `x = phase * scale - offset` (mod 2^32) becomes a triangle `2 |x| 2^-31 - 1` and an odd
//! polynomial (a sine), which is smoothed once per call. The pan law is
//! `w = clip(A S k1, B) k2`, `g_L = P + w`, `g_R = P - w` with `A`, `P`, `B` smoothed parameters;
//! each block of two stereo frames is multiplied by `(g_L, g_R)`.
//!
//! VERIFIED: `tools/fx_model_apan.py` reproduces the routine word for word against the `sharc-fx`
//! oracle (1,000 random scenarios and the per-instruction register compare of
//! `tools/fx_models.py trace apan`), `tests_apan_golden.rs` holds the golden vectors, matched to
//! [`GOLDEN_TOLERANCE`]. The LFO value of the *left* channel drives both gains (the routine
//! broadcasts it, so the right phase word never reaches the audio). EMPIRICAL: the role names
//! (the words follow `kFx1Amt` / `kFx1RateClkDiv`, `docs/research/fx-mod-types.md`).

use super::fw_words::{clip, smooth, Words};

/// Number of context words (`I1 - 0x20 ..= I1 - 1`).
pub const CTX_WORDS: usize = 32;
const BASE: i32 = -0x20;

/// Relative tolerance of this f32 port against the 40-bit golden vectors.
#[cfg(test)]
pub(super) const GOLDEN_TOLERANCE: f32 = 5.0e-6;

/// Word offsets from `I1`. VERIFIED against the oracle; roles EMPIRICAL.
pub mod ctx {
    /// Integer phase increment per call (`kFx1RateClkDiv`: `12800 + 16384 v`).
    pub const RATE: i32 = -0x20;
    /// Integer phase accumulator pair (written back, both words).
    pub const PHASE: i32 = -0x1F;
    /// Pass pointer to the phase pair (the context's own `PHASE` address in the firmware).
    pub const PHASE_PTR: i32 = -0x1B;
    /// Multiplier of the phase (unsigned integer product).
    pub const SCALE: i32 = -0x1A;
    /// Offset subtracted from the scaled phase (selects the LFO phase; type 1 differs).
    pub const OFFSET: i32 = -0x19;
    /// Pair: smoothed LFO value (state, written back).
    pub const LFO_STATE: i32 = -0x1D;
    /// Depth target `A` (`kFx1Amt`), its state at [`A_STATE`], the copy used at [`A_USED`].
    pub const A_TARGET: i32 = -0x18;
    /// Clip bound target `B`, state [`B_STATE`].
    pub const B_TARGET: i32 = -0x17;
    /// Offset target `P`, state [`P_STATE`], the copy used at [`P_USED`].
    pub const P_TARGET: i32 = -0x16;
    /// State of `A`.
    pub const A_STATE: i32 = -0x11;
    /// `A` as used by the block (pair, written back).
    pub const A_USED: i32 = -0x15;
    /// State of `P`.
    pub const P_STATE: i32 = -0xF;
    /// `P` as used by the block (pair, written back).
    pub const P_USED: i32 = -0x13;
    /// State of `B` (also the bound used by the clip).
    pub const B_STATE: i32 = -0xD;
    /// Sine polynomial coefficients `a1 u + a3 u^3 + a5 u^5` (floats).
    pub const POLY: [i32; 3] = [-0xB, -0xA, -9];
    /// Shift of the integer to float conversion (`-31`).
    pub const SHIFT: i32 = -8;
    /// Smoothing coefficient `k` of every smoother.
    pub const SMOOTH: i32 = -7;
    /// Multiplier of `A S` before the clip (4.0 in the image).
    pub const PRE_GAIN: i32 = -6;
    /// Offset added to the doubled triangle (`-1.0`).
    pub const TRI_OFFSET: i32 = -5;
    /// Multiplier of the clipped value (0.5 in the image).
    pub const POST_GAIN: i32 = -4;
    /// Number of blocks of two frames per call (2 in the image: 4 frames per call).
    pub const BLOCKS: i32 = -3;
}

/// Firmware auto-pan stage: the 32 context words. Allocation free.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FwAutoPan {
    ctx: Words<CTX_WORDS>,
}

impl Default for FwAutoPan {
    /// Zero context: both gains are 0 (silence) until the words are set.
    fn default() -> Self {
        Self {
            ctx: Words::new(BASE),
        }
    }
}

impl FwAutoPan {
    /// Builds the stage from the 32 raw context words (`I1 - 0x20 ..= I1 - 1`).
    pub const fn from_words(words: [u32; CTX_WORDS]) -> Self {
        Self {
            ctx: Words::from_words(BASE, words),
        }
    }

    /// The context words, including the written-back state.
    pub const fn words(&self) -> &[u32; CTX_WORDS] {
        self.ctx.words()
    }

    /// Sets a raw context word.
    pub fn set_word(&mut self, offset: i32, word: u32) {
        self.ctx.set_word(offset, word);
    }

    /// Sets a float context word.
    pub fn set_value(&mut self, offset: i32, value: f32) {
        self.ctx.set(offset, value);
    }

    /// Reads a float context word.
    pub fn value(&self, offset: i32) -> f32 {
        self.ctx.f(offset)
    }

    /// Reads a raw context word.
    pub fn word(&self, offset: i32) -> u32 {
        self.ctx.word(offset)
    }

    /// One call: advances the phase and the smoothers once and multiplies every block of two
    /// stereo frames `[l0, r0, l1, r1]` of `frames` by the pan gains (the firmware's block count
    /// word [`ctx::BLOCKS`] is the caller's slice length here).
    pub fn process(&mut self, frames: &mut [f32]) {
        let c = &mut self.ctx;
        let k = c.f(ctx::SMOOTH);
        // Phase pair: counter word plus rate, stored back to both words.
        let (old_l, old_r) = (c.word(ctx::PHASE), c.word(ctx::PHASE + 1));
        let rate = c.word(ctx::RATE);
        let (phase_l, phase_r) = (old_l.wrapping_add(rate), old_r.wrapping_add(rate));
        c.set_word(ctx::PHASE, phase_l);
        c.set_word(ctx::PHASE + 1, phase_r);
        // The three smoothed parameters, each stored twice (pair at the state and the used word).
        let a = smooth(c.f(ctx::A_TARGET), c.f(ctx::A_STATE), k);
        let p = smooth(c.f(ctx::P_TARGET), c.f(ctx::P_STATE), k);
        let b = smooth(c.f(ctx::B_TARGET), c.f(ctx::B_STATE), k);
        for (off, v) in [
            (ctx::A_USED, a),
            (ctx::A_STATE, a),
            (ctx::P_USED, p),
            (ctx::P_STATE, p),
        ] {
            c.set_pair(off, v);
        }
        c.set_pair(ctx::B_STATE, b);
        // LFO of the left phase, smoothed against the stored pair state.
        let s = lfo(c, phase_l);
        let s_l = smooth(s, c.f(ctx::LFO_STATE), k);
        let s_r = smooth(s, c.f(ctx::LFO_STATE + 1), k);
        c.set(ctx::LFO_STATE, s_l);
        c.set(ctx::LFO_STATE + 1, s_r);
        let w = clip(c.f(ctx::A_USED) * s_l * c.f(ctx::PRE_GAIN), b) * c.f(ctx::POST_GAIN);
        let (g_l, g_r) = (w + p, p - w);
        let (blocks, _) = frames.as_chunks_mut::<4>();
        for block in blocks {
            block[0] *= g_l;
            block[1] *= g_r;
            block[2] *= g_l;
            block[3] *= g_r;
        }
    }
}

/// The LFO value from the phase: `x = phase * scale - offset` (mod 2^32), converted by the shift
/// word, `u = 2 |x| + tri_offset`, then the odd polynomial.
fn lfo(c: &Words<CTX_WORDS>, phase: u32) -> f32 {
    let x = phase
        .wrapping_mul(c.word(ctx::SCALE))
        .wrapping_sub(c.word(ctx::OFFSET)) as i32;
    let shift = c.int(ctx::SHIFT);
    let u = 2.0 * (f64::from(x) * 2f64.powi(shift)).abs() + f64::from(c.f(ctx::TRI_OFFSET));
    let [a1, a3, a5] = ctx::POLY.map(|o| f64::from(c.f(o)));
    (a1 * u + a3 * u.powi(3) + a5 * u.powi(5)) as f32
}

#[cfg(test)]
mod tests {
    use super::super::fw_words::assert_close;
    use super::super::tests_apan_golden as g;
    use super::*;

    #[test]
    fn matches_the_firmware_model_over_three_calls() {
        let mut f = FwAutoPan::from_words(g::PAN_CTX_IN);
        let (ins, outs) = (
            g::PAN_FRAMES_IN.as_chunks::<8>().0,
            g::PAN_FRAMES_OUT.as_chunks::<8>().0,
        );
        assert_eq!((ins.len(), outs.len()), (3, 3));
        for (n, (inp, out)) in ins.iter().zip(outs).enumerate() {
            let mut frames = inp.map(f32::from_bits);
            f.process(&mut frames);
            let got = frames.map(f32::to_bits);
            assert_close(&got, out, GOLDEN_TOLERANCE, &format!("call {n}"));
        }
        assert_close(
            f.words(),
            &g::PAN_CTX_OUT,
            GOLDEN_TOLERANCE,
            "final context",
        );
    }

    #[test]
    fn the_phase_accumulator_is_an_integer_sum_written_to_both_words() {
        let mut f = FwAutoPan::default();
        f.set_word(ctx::PHASE, 0xFFFF_FFF0);
        f.set_word(ctx::PHASE + 1, 0x10);
        f.set_word(ctx::RATE, 0x20);
        f.process(&mut []);
        assert_eq!(f.word(ctx::PHASE), 0x10);
        assert_eq!(f.word(ctx::PHASE + 1), 0x30);
    }

    #[test]
    fn the_two_channels_get_opposite_gain_offsets_around_p() {
        let mut f = FwAutoPan::from_words(g::PAN_CTX_IN);
        f.set_value(ctx::P_STATE, 0.75);
        f.set_value(ctx::P_TARGET, 0.75);
        let mut frames = [1.0f32; 4];
        f.process(&mut frames);
        // g_L + g_R = 2 P; the pair repeats over the two frames of the block.
        assert!((frames[0] + frames[1] - 1.5).abs() < 1.0e-6, "{frames:?}");
        assert_eq!((frames[0], frames[1]), (frames[2], frames[3]));
    }

    #[test]
    fn the_golden_vectors_are_not_silent() {
        assert!(g::PAN_FRAMES_IN != g::PAN_FRAMES_OUT && g::PAN_CTX_IN != g::PAN_CTX_OUT);
    }
}
