//! Firmware LFO-modulated dual-biquad stage (FX routine `0x122C39`, module `0xC02BB030`, the
//! phaser / vibe class) as an opt-in stage.
//!
//! A free-running counter word (the pass pointer `ctx[-0x1D]` points at it) is scaled and offset
//! into a signed phase, turned into a triangle and then an odd polynomial (sine), smoothed, and
//! combined with two smoothed parameters into one gain `g`. Per channel two biquads `N` and `F`
//! (transposed direct form II, shared coefficients) run on the input and the output is
//! `g ((1 - m) x + m F(x)) + m N(x)` with the mix `m = ctx[2]`.
//!
//! VERIFIED: `tools/fx_model_allpass.py` reproduces the routine word for word against the
//! `sharc-fx` oracle (2,500 random scenarios, per-instruction register compare);
//! `tools/fx_closed_forms.py::allpass` holds these equations (agreeing with the model to 1e-4) and
//! `tests_allpass_golden.rs` the golden vectors, matched to [`GOLDEN_TOLERANCE`]. The right
//! channel's LFO value is the constant `ctx[-0x16]` (not the LFO), the clip bound and the gain
//! offset are the smoothed *right* words `ctx[-0x14]` / `ctx[-0x17]` (VERIFIED by the symbolic run
//! `tools/fx_symbolic.py allpass`). ASSUMED: the musical meaning of the words and which ARM
//! parameter writes which target.

use super::fw_words::{clip, smooth, Words};

/// Number of context words (`I1 - 0x20 ..= I1 + 0x04`).
pub const CTX_WORDS: usize = 37;
const BASE: i32 = -0x20;

/// Relative tolerance of this f32 port against the 40-bit golden vectors (measured worst case
/// between 1e-6 and 3e-6 over outputs and context).
#[cfg(test)]
pub(super) const GOLDEN_TOLERANCE: f32 = 5.0e-6;

/// Word offsets from `I1`. VERIFIED against the oracle; roles EMPIRICAL.
pub mod ctx {
    /// Integer phase increment (added to `PHASE` each call).
    pub const RATE: i32 = -0x20;
    /// Integer phase accumulator (written back, both words).
    pub const PHASE: i32 = -0x1F;
    /// Multiplier of the free-running counter (unsigned integer product).
    pub const COUNTER_SCALE: i32 = -0x1C;
    /// Offset subtracted from the scaled counter.
    pub const COUNTER_OFFSET: i32 = -0x1B;
    /// Sine polynomial coefficients `a1 u + a3 u^3 + a5 u^5` (floats).
    pub const POLY: [i32; 3] = [-0x13, -0x12, -0x11];
    /// Pair: targets of the smoother `A` (its state pair is at [`A_STATE`]).
    pub const A_TARGET: i32 = -0x1A;
    /// Pair: state of `A` (written back).
    pub const A_STATE: i32 = -0x18;
    /// Constant used as the right channel's LFO value.
    pub const RIGHT_LFO: i32 = -0x16;
    /// Pair: LFO smoother state `S` (written back; the right word is the clip bound of `g`).
    pub const S_STATE: i32 = -0x15;
    /// Smoothing coefficient `k` of `A` and `S`.
    pub const SMOOTH: i32 = 3;
    /// Wet / dry mix `m`.
    pub const MIX: i32 = 2;
}

/// Firmware dual-biquad stage: the 37 context words. Allocation free.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FwAllpass {
    ctx: Words<CTX_WORDS>,
}

impl Default for FwAllpass {
    /// Zero context: unity pass-through is not available (the routine multiplies by `g = 0`), so
    /// the output is silence until the words are set.
    fn default() -> Self {
        Self {
            ctx: Words::new(BASE),
        }
    }
}

impl FwAllpass {
    /// Builds the stage from the 37 raw context words (`I1 - 0x20 ..= I1 + 0x04`).
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

    /// Processes whole blocks in place. `timer` is the pair of counter words the pass pointer
    /// `ctx[-0x1D]` addresses (left, right); each block of 4 words is two stereo frames
    /// `[l0, r0, l1, r1]`, and the state runs through all blocks of the call. The loop count word
    /// `ctx[4]` of the firmware is the number of blocks and is not read here.
    pub fn process(&mut self, timer: [u32; 2], frames: &mut [f32]) {
        let c = &mut self.ctx;
        let phase = c.word(ctx::PHASE).wrapping_add(c.word(ctx::RATE));
        c.set_word(ctx::PHASE, phase);
        c.set_word(ctx::PHASE + 1, phase);
        let k = c.f(ctx::SMOOTH);
        let lfo = lfo(c, timer[0]);
        let a_l = smooth(c.f(-0x1A), c.f(-0x18), k);
        let a_r = smooth(c.f(-0x19), c.f(-0x17), k);
        let s_l = smooth(lfo as f32, c.f(ctx::S_STATE), k);
        let s_r = smooth(c.f(ctx::RIGHT_LFO), c.f(ctx::S_STATE + 1), k);
        for (off, v) in [(-0x18, a_l), (-0x17, a_r), (-0x15, s_l), (-0x14, s_r)] {
            c.set(off, v);
        }
        let gain = clip(s_l * a_l, s_r) + a_r;
        let mix = c.f(ctx::MIX);
        let near = [-7, -6, -5, -4, -3].map(|o| c.f(o));
        let far = [-0x10, -0xF, -0xE, -0xD, -0xC].map(|o| c.f(o));
        for ch in 0..2 {
            let mut near_state = [c.f(-2 + ch), c.f(ch)];
            let mut far_state = [c.f(-0xB + ch), c.f(-9 + ch)];
            let (blocks, _) = frames.as_chunks_mut::<4>();
            for block in blocks {
                for frame in 0..2 {
                    let x = block[2 * frame + ch as usize];
                    let n = tdf2(x, &mut near_state, &near);
                    let f = tdf2(x, &mut far_state, &far);
                    block[2 * frame + ch as usize] = ((x - mix * x) + mix * f) * gain + n * mix;
                }
            }
            c.set(-2 + ch, near_state[0]);
            c.set(ch, near_state[1]);
            c.set(-0xB + ch, far_state[0]);
            c.set(-9 + ch, far_state[1]);
        }
    }
}

/// The LFO value of the left channel from the counter word: the signed phase
/// `x = counter * scale - offset` (mod 2^32), triangle `2 |x| 2^-31 - 1`, sine polynomial.
fn lfo(c: &Words<CTX_WORDS>, counter: u32) -> f64 {
    let x = counter
        .wrapping_mul(c.word(ctx::COUNTER_SCALE))
        .wrapping_sub(c.word(ctx::COUNTER_OFFSET)) as i32;
    let u = 2.0 * (f64::from(x) * 2f64.powi(-31)).abs() - 1.0;
    let [a1, a3, a5] = ctx::POLY.map(|o| f64::from(c.f(o)));
    a1 * u + a3 * u.powi(3) + a5 * u.powi(5)
}

/// One transposed direct form II step with coefficients `[b0, b1, b2, a1, a2]` ordered as
/// `[c(-7), c(-6), c(-5), c(-4), c(-3)]` (`y = b0 x + z1; z1' = b1 x + z2 + a1 y;
/// z2' = b2 x + a2 y`, the firmware adds the feedback terms).
#[inline]
fn tdf2(x: f32, z: &mut [f32; 2], c: &[f32; 5]) -> f32 {
    let y = x * c[0] + z[0];
    z[0] = (x * c[1] + z[1]) + y * c[3];
    z[1] = y * c[4] + x * c[2];
    y
}

#[cfg(test)]
mod tests {
    use super::super::fw_words::assert_close;
    use super::super::tests_allpass_golden as g;
    use super::*;

    #[test]
    fn matches_the_firmware_model_over_three_calls() {
        let mut f = FwAllpass::from_words(g::AP_CTX_IN);
        let (ins, outs) = (
            g::AP_FRAMES_IN.as_chunks::<8>().0,
            g::AP_FRAMES_OUT.as_chunks::<8>().0,
        );
        let timers = g::AP_EXTRA.as_chunks::<2>().0;
        assert_eq!((ins.len(), outs.len(), timers.len()), (3, 3, 3));
        for (n, ((inp, out), timer)) in ins.iter().zip(outs).zip(timers).enumerate() {
            let mut frames = inp.map(f32::from_bits);
            f.process(*timer, &mut frames);
            let got = frames.map(f32::to_bits);
            assert_close(&got, out, GOLDEN_TOLERANCE, &format!("call {n}"));
        }
        assert_close(f.words(), &g::AP_CTX_OUT, GOLDEN_TOLERANCE, "final context");
    }

    #[test]
    fn the_phase_accumulator_is_an_integer_sum_written_to_both_words() {
        let mut f = FwAllpass::default();
        f.set_word(ctx::PHASE, 0xFFFF_FFF0);
        f.set_word(ctx::RATE, 0x20);
        f.process([0, 0], &mut []);
        assert_eq!(f.words()[(ctx::PHASE - BASE) as usize], 0x10);
        assert_eq!(f.words()[(ctx::PHASE + 1 - BASE) as usize], 0x10);
    }

    #[test]
    fn a_zero_mix_scales_the_dry_signal_by_one_common_gain() {
        let mut f = FwAllpass::from_words(g::AP_CTX_IN);
        f.set_value(ctx::MIX, 0.0);
        let mut frames: [f32; 8] = [0.5, -0.25, 0.75, 0.125, -0.5, 0.25, 0.375, -0.0625];
        let dry = frames;
        f.process([0x1234_5678, 0x9ABC_DEF0], &mut frames);
        let ratio = frames[0] / dry[0];
        assert!(ratio != 0.0);
        for (y, x) in frames.iter().zip(dry) {
            assert!(
                (y / x - ratio).abs() <= 1.0e-5 * ratio.abs(),
                "{y} {x} {ratio}"
            );
        }
    }

    #[test]
    fn the_golden_vectors_are_not_silent() {
        assert!(g::AP_FRAMES_IN != g::AP_FRAMES_OUT && g::AP_CTX_IN != g::AP_CTX_OUT);
        assert_eq!(g::AP_EXTRA.len(), 6);
    }
}
