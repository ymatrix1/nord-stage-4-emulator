//! Firmware nonlinear resonant filter pair (FX routines `0x1243F4` and `0x124497`) as opt-in
//! stages: a state-variable section whose integrator is clipped in the feedback path, one-pole
//! shelves and parameter smoothers, left and right channel running the same recurrence.
//!
//! VERIFIED: `tools/fx_model_resonant.py` and `tools/fx_model_resonant4.py` reproduce both routines
//! word for word against the `sharc-fx` oracle (2,500 and 2,200 random scenarios, per-instruction
//! register compare on the trace); the recurrences below are `tools/fx_closed_forms.py`
//! (`resonant2`, `resonant4`, agreeing with the models to 1e-4), derived from the symbolic run
//! `tools/fx_symbolic.py`. `tests_resonant_golden.rs` holds golden vectors from the models and the
//! ports match them to [`GOLDEN_TOLERANCE`] (f32 here, 40-bit registers and truncating stores
//! there). The context layout is the firmware's (offsets from `I1`, see `docs/research/
//! fx-routine-models.md` s5); the meaning of the words (cutoff, resonance, ...) is ASSUMED, and so
//! is which ARM parameter writes which target word.

use super::fw_words::{clip, smooth, Words};

/// Number of context words (`I1 - 0x20 ..= I1 - 0x01`).
pub const CTX_WORDS: usize = 32;
const BASE: i32 = -0x20;

/// Relative tolerance of these f32 ports against the 40-bit golden vectors (measured worst case
/// between 1e-6 and 3e-6 over the outputs and the final context).
#[cfg(test)]
pub(super) const GOLDEN_TOLERANCE: f32 = 5.0e-6;

/// Per channel state words of both routines (left offset; the right word is `+ 1`).
mod state {
    /// Input smoother (low-pass) state.
    pub const SMOOTHER: i32 = -0xD;
    /// Previous input (`0x1243F4`) or previous shelf output (`0x124497`).
    pub const PREVIOUS: i32 = -0x9;
    /// Band-pass state `w` (the clipped one).
    pub const BAND: i32 = -0x7;
    /// Low-pass state `z`.
    pub const LOW: i32 = -0x5;
    /// Output shelf state `q`.
    pub const SHELF: i32 = -0xB;
}

/// Offsets of the words shared by both routines.
pub mod ctx {
    /// Smoothing coefficient `k` of the parameter smoothers (about 0.9908 per call).
    pub const SMOOTH: i32 = -0x11;
    /// Clip bound of the integrator and reference of the output gain.
    pub const LIMIT: i32 = -0xE;
    /// Pole of the input smoother (`0x1243F4`) or of the output smoother (`0x124497`).
    pub const POLE: i32 = -0x14;
}

macro_rules! words_api {
    ($name:ident) => {
        impl $name {
            /// Builds the stage from the 32 raw context words (`I1 - 0x20 ..= I1 - 0x01`).
            pub const fn from_words(words: [u32; CTX_WORDS]) -> Self {
                Self {
                    ctx: Words::from_words(BASE, words),
                }
            }

            /// The context words, including the written-back state.
            pub const fn words(&self) -> &[u32; CTX_WORDS] {
                self.ctx.words()
            }

            /// Sets a float context word.
            pub fn set_value(&mut self, offset: i32, value: f32) {
                self.ctx.set(offset, value);
            }

            /// Reads a float context word.
            pub fn value(&self, offset: i32) -> f32 {
                self.ctx.f(offset)
            }
        }

        impl Default for $name {
            /// Zero context: every gain and state is zero, so the stage passes the input through.
            fn default() -> Self {
                Self {
                    ctx: Words::new(BASE),
                }
            }
        }
    };
}

/// `0x1243F4`: two stereo frames per call. The parameters are smoothed first, then per frame
/// `u += a (x - u); s = x + A u; z += B w; w = clip(w + B ((s + x_prev) / 2 - z - C w), L);
/// y = s + D w; r = G y; d = r - q; out = y + F d; q = r + E d`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FwResonant2 {
    ctx: Words<CTX_WORDS>,
}

words_api!(FwResonant2);

impl FwResonant2 {
    /// Processes one call in place: `frames = [l0, r0, l1, r1]`.
    pub fn process(&mut self, frames: &mut [f32; 4]) {
        let c = &mut self.ctx;
        let (k, lim, pole) = (c.f(ctx::SMOOTH), c.f(ctx::LIMIT), c.f(ctx::POLE));
        let feed = smooth(c.f(-0x20), c.f(-0x1E), k); // A
        let step = smooth(c.f(-0x1F), c.f(-0x1D), k); // B
        let damp = smooth(c.f(-0x17), c.f(-0x15), k); // C
        let mix = smooth(c.f(-0x18), c.f(-0x16), k); // D
        let shelf_pole = smooth(c.f(-0x1C), c.f(-0x1A), k); // E
        let shelf_gain = smooth(c.f(-0x1B), c.f(-0x19), k); // F
        let level = (lim - shelf_pole) / 2.0; // G
        for (off, v) in [
            (-0x1E, feed),
            (-0x1D, step),
            (-0x15, damp),
            (-0x16, mix),
            (-0x1A, shelf_pole),
            (-0x19, shelf_gain),
            (-0x13, level),
            (-0x12, (lim - shelf_gain) / 2.0),
        ] {
            c.set(off, v);
        }
        for ch in 0..2 {
            let mut u = c.f(state::SMOOTHER + ch);
            let mut prev = c.f(state::PREVIOUS + ch);
            let mut w = c.f(state::BAND + ch);
            let mut z = c.f(state::LOW + ch);
            let mut q = c.f(state::SHELF + ch);
            for n in 0..2 {
                let x = frames[2 * n + ch as usize];
                u += pole * (x - u);
                let s = x + feed * u;
                z += step * w;
                w = clip(w + step * ((s + prev) / 2.0 - z - damp * w), lim);
                let y = s + mix * w;
                let r = level * y;
                let d = r - q;
                frames[2 * n + ch as usize] = y + shelf_gain * d;
                q = r + shelf_pole * d;
                prev = x;
            }
            c.set(state::SMOOTHER + ch, u);
            c.set(state::PREVIOUS + ch, prev);
            c.set(state::BAND + ch, w);
            c.set(state::LOW + ch, z);
            c.set(state::SHELF + ch, q);
        }
    }
}

/// `0x124497`: four stereo frames per call, fully unrolled in the firmware. Per frame a shelf on the
/// input (`r = x P; d = r - q; s = x + d R; q = r + d S`), the state-variable section on `s`, a
/// one-pole smoother of the section output and a feed-forward output; the parameters are smoothed
/// after the frames.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FwResonant4 {
    ctx: Words<CTX_WORDS>,
}

words_api!(FwResonant4);

impl FwResonant4 {
    /// Processes one call in place: `frames = [l0, r0, l1, r1, l2, r2, l3, r3]`.
    pub fn process(&mut self, frames: &mut [f32; 8]) {
        let c = &mut self.ctx;
        let (k, lim, pole) = (c.f(ctx::SMOOTH), c.f(ctx::LIMIT), c.f(ctx::POLE));
        let (gain_p, gain_r, gain_s) = (c.f(-0x13), c.f(-0x19), c.f(-0x1A));
        let (step, damp, mix, out_gain) = (c.f(-0x1D), c.f(-0x15), c.f(-0x16), c.f(-0x1E));
        for ch in 0..2 {
            let mut u = c.f(state::SMOOTHER + ch);
            let mut prev = c.f(state::PREVIOUS + ch);
            let mut w = c.f(state::BAND + ch);
            let mut z = c.f(state::LOW + ch);
            let mut q = c.f(state::SHELF + ch);
            for n in 0..4 {
                let x = frames[2 * n + ch as usize];
                let r = x * gain_p;
                let d = r - q;
                let s = x + d * gain_r;
                q = r + d * gain_s;
                z += w * step;
                w = clip(w + step * ((s + prev) / 2.0 - z - w * damp), lim);
                prev = s;
                let y = s + w * mix;
                u += pole * (y - u);
                frames[2 * n + ch as usize] = y + u * out_gain;
            }
            c.set(state::SMOOTHER + ch, u);
            c.set(state::PREVIOUS + ch, prev);
            c.set(state::BAND + ch, w);
            c.set(state::LOW + ch, z);
            c.set(state::SHELF + ch, q);
        }
        let shelf_pole = smooth(c.f(-0x1C), c.f(-0x1A), k);
        let shelf_gain = smooth(c.f(-0x1B), c.f(-0x19), k);
        for (off, v) in [
            (-0x1E, smooth(c.f(-0x20), c.f(-0x1E), k)),
            (-0x1D, smooth(c.f(-0x1F), c.f(-0x1D), k)),
            (-0x1A, shelf_pole),
            (-0x19, shelf_gain),
            (-0x16, smooth(c.f(-0x18), c.f(-0x16), k)),
            (-0x15, smooth(c.f(-0x17), c.f(-0x15), k)),
            (-0x13, (lim - shelf_pole) / 2.0),
            (-0x12, (lim - shelf_gain) / 2.0),
        ] {
            c.set(off, v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::fw_words::assert_close;
    use super::super::tests_resonant_golden as g;
    use super::*;

    /// Runs `calls` blocks of `N` words through `process` and checks outputs and final context.
    fn golden<const N: usize>(
        mut process: impl FnMut(&mut [f32; N]),
        input: &[u32],
        want: &[u32],
        words: impl Fn() -> [u32; CTX_WORDS],
        want_ctx: &[u32],
    ) {
        let (ins, outs) = (input.as_chunks::<N>().0, want.as_chunks::<N>().0);
        assert_eq!(ins.len(), 3);
        for (n, (inp, out)) in ins.iter().zip(outs).enumerate() {
            let mut frames = inp.map(f32::from_bits);
            process(&mut frames);
            assert_close(
                &frames.map(f32::to_bits),
                out,
                GOLDEN_TOLERANCE,
                &format!("call {n}"),
            );
        }
        assert_close(&words(), want_ctx, GOLDEN_TOLERANCE, "final context");
    }

    #[test]
    fn two_frame_routine_matches_the_firmware_model() {
        let f = std::cell::RefCell::new(FwResonant2::from_words(g::RES2_CTX_IN));
        golden(
            |b| f.borrow_mut().process(b),
            &g::RES2_FRAMES_IN,
            &g::RES2_FRAMES_OUT,
            || *f.borrow().words(),
            &g::RES2_CTX_OUT,
        );
    }

    #[test]
    fn four_frame_routine_matches_the_firmware_model() {
        let f = std::cell::RefCell::new(FwResonant4::from_words(g::RES4_CTX_IN));
        golden(
            |b| f.borrow_mut().process(b),
            &g::RES4_FRAMES_IN,
            &g::RES4_FRAMES_OUT,
            || *f.borrow().words(),
            &g::RES4_CTX_OUT,
        );
    }

    #[test]
    fn golden_vectors_are_not_silent_and_the_filters_changed_the_input() {
        assert!(g::RES2_FRAMES_IN != g::RES2_FRAMES_OUT);
        assert!(g::RES4_FRAMES_IN != g::RES4_FRAMES_OUT);
        assert!(g::RES2_CTX_IN != g::RES2_CTX_OUT && g::RES4_CTX_IN != g::RES4_CTX_OUT);
    }

    #[test]
    fn zero_context_passes_the_input_through() {
        let mut a = FwResonant2::default();
        let mut f = [0.5, -0.25, 1.0, 0.125];
        a.process(&mut f);
        assert_eq!(f, [0.5, -0.25, 1.0, 0.125]);
        let mut b = FwResonant4::default();
        let mut g = [0.5, -0.25, 1.0, 0.125, 0.0, 2.0, -1.0, 3.0];
        b.process(&mut g);
        assert_eq!(g, [0.5, -0.25, 1.0, 0.125, 0.0, 2.0, -1.0, 3.0]);
    }

    #[test]
    fn parameters_move_toward_their_targets_by_one_pole_steps() {
        let mut a = FwResonant2::default();
        a.set_value(ctx::SMOOTH, 0.5);
        a.set_value(-0x20, 1.0); // target of A, state 0
        a.process(&mut [0.0; 4]);
        assert_eq!(a.value(-0x1E), 0.5, "A = target + k (state - target)");
        a.process(&mut [0.0; 4]);
        assert_eq!(a.value(-0x1E), 0.75);
        let mut b = FwResonant4::default();
        b.set_value(ctx::SMOOTH, 0.5);
        b.set_value(-0x20, 1.0);
        b.set_value(ctx::LIMIT, 2.0);
        b.process(&mut [0.0; 8]);
        assert_eq!(b.value(-0x1E), 0.5);
        assert_eq!(b.value(-0x13), 1.0, "G = (limit - E) / 2");
    }

    #[test]
    fn the_integrator_is_clipped_to_the_limit() {
        for four in [false, true] {
            let mut words = [0u32; CTX_WORDS];
            let mut set = |off: i32, v: f32| words[(off - BASE) as usize] = v.to_bits();
            set(ctx::LIMIT, 0.01);
            set(-0x1D, 1.0); // integrator step B
            set(-0x15, 0.0);
            set(state::BAND, 0.9);
            set(state::BAND + 1, -0.9);
            let band = if four {
                let mut f = FwResonant4::from_words(words);
                f.process(&mut [0.3; 8]);
                (f.value(state::BAND), f.value(state::BAND + 1))
            } else {
                let mut f = FwResonant2::from_words(words);
                f.process(&mut [0.3; 4]);
                (f.value(state::BAND), f.value(state::BAND + 1))
            };
            assert!(band.0.abs() <= 0.01 && band.1.abs() <= 0.01, "{band:?}");
        }
    }

    #[test]
    fn the_left_and_right_channels_are_independent() {
        let mut a = FwResonant2::default();
        a.set_value(-0x20, 0.7);
        a.set_value(ctx::POLE, 0.3);
        let mut b = a;
        let (mut fa, mut fb) = ([0.4, 0.0, -0.2, 0.0], [0.4, 0.9, -0.2, -0.9]);
        a.process(&mut fa);
        b.process(&mut fb);
        assert_eq!(
            (fa[0], fa[2]),
            (fb[0], fb[2]),
            "left ignores the right input"
        );
    }
}
