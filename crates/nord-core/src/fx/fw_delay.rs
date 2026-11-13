//! Firmware stereo delay routine (FX routine `0x1206B0`, module list `0xC0279098`) as an opt-in
//! stage: packed 16-bit-float delay memory with a fractional two-tap read, a damping filter, a
//! ping-pong send matrix, a level-normalised waveshaped feedback written back to the line, peak
//! meters and 0.99078 one-pole parameter smoothers, two stereo frames per call.
//!
//! VERIFIED: `tools/fx_model_delay.py` reproduces the routine word for word against the
//! `sharc-fx` oracle (4,600 random scenarios, per-instruction register compare, mutation checked);
//! the equations below are `tools/fx_closed_delay.py` (agreeing with the model to 5e-6, the
//! packed words exactly), extracted by the symbolic run `tools/fx_symbolic.py delay`.
//! `tests_delay_golden.rs` holds golden vectors matched to [`GOLDEN_TOLERANCE`]. ASSUMED: the
//! musical meaning of the words (time, feedback, tone) and which ARM parameter writes which
//! target; the firmware's two pass pointers (`DM(0xC03D4)`, `DM(0xC03D5)`) become the base indices
//! in [`DelayIo`].

use super::fw_short_float::{fpack, lanes};
use super::fw_words::{clip, fix_by, smooth, Words};

/// Number of context words (`I1 - 0x20 ..= I1 + 0x25`).
pub const CTX_WORDS: usize = 70;
const BASE: i32 = -0x20;

/// Relative tolerance of this f32 port against the 40-bit golden vectors (measured worst case
/// below 1e-6 over the outputs, the sends and the context; the packed line words match exactly).
#[cfg(test)]
pub(super) const GOLDEN_TOLERANCE: f32 = 2.0e-6;

/// `0x3F7DA3C2` (0.99078), the smoothing coefficient.
const K: f32 = f32::from_bits(0x3F7D_A3C2);
/// `0x406429FB` (3.5649), the gain of the waveshaper output.
const SHAPER_GAIN: f32 = f32::from_bits(0x4064_29FB);
/// `0x3E99999A` (0.3), the scale of the waveshaper input.
const SHAPER_SCALE: f32 = f32::from_bits(0x3E99_999A);
/// `0x3C23D70A` (0.01), lower clamp of the level estimates.
const FLOOR: f32 = f32::from_bits(0x3C23_D70A);

/// Word offsets from `I1` of the fields the stage reads. VERIFIED against the oracle; roles
/// EMPIRICAL.
pub mod ctx {
    /// Pair: smoothed level ratio state (written back).
    pub const RATIO: i32 = 0xC;
    /// Smoothing coefficient of the ratio.
    pub const RATIO_K: i32 = 0xE;
    /// Upper and lower clamp of the ratio.
    pub const RATIO_HI_LO: [i32; 2] = [0x24, 0x25];
    /// Peak meters: pair of sends, pair of fed-back signals; decay per call at [`METER_DECAY`].
    pub const METERS: i32 = 7;
    /// Decay factor of the peak meters.
    pub const METER_DECAY: i32 = 0xB;
    /// Write position offset in the write line (`M4`).
    pub const WRITE_OFFSET: i32 = 0x10;
    /// Read position offset of the samples (`M5`).
    pub const READ_OFFSET: i32 = 0x11;
    /// Read position offset of the weight words.
    pub const WEIGHT_OFFSET: i32 = 0x12;
    /// Shift of the weight word (a left shift, negative shifts right).
    pub const WEIGHT_SHIFT: i32 = 0x13;
    /// Damping filter `b0 b1 a` (floats) and its pair state at [`FILTER_STATE`].
    pub const FILTER: [i32; 3] = [-1, 0, 1];
    /// Pair: state of the damping filter (written back).
    pub const FILTER_STATE: i32 = 2;
    /// Pair of taps of the previous call, frame 0 then frame 1 (written back).
    pub const TAPS: i32 = 0x1D;
    /// `FIX` scale of the waveshaper, its `FEXT` / `FDEP` control and the `FLOAT` exponent.
    pub const SHAPER_FIX: [i32; 3] = [-0x1E, -0x1C, -0x1D];
    /// Clip bound of the waveshaper input.
    pub const SHAPER_BOUND: i32 = -0xC;
}

/// The memory a call reads and writes besides the context.
pub struct DelayIo<'a> {
    /// Input and output frames `[l0, r0, l1, r1]`, rewritten in place.
    pub frames: &'a mut [f32; 4],
    /// Sends of the previous call in, sends of this call out (the firmware's output buffer).
    pub sends: &'a mut [f32; 4],
    /// Packed delay memory words (two 16-bit floats per word, left in the low half).
    pub line: &'a mut [u32],
    /// Index in `line` of `DM(0xC03D4)`: the frames are written at `write_base + M4` and `+ 0x14`.
    pub write_base: usize,
    /// Index in `line` of `DM(0xC03D5)`: weights at `read_base + ctx[0x12]`, samples at
    /// `read_base + ctx[0x11]`, each pair again at `+ 0x28` for frame 1.
    pub read_base: usize,
    /// Waveshaper table (signed 10-bit index around `shaper_center`).
    pub shaper: &'a [f32],
    /// Index in `shaper` of the entry for index 0.
    pub shaper_center: usize,
}

/// Firmware delay stage: the 70 context words. Allocation free.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FwDelay {
    ctx: Words<CTX_WORDS>,
}

impl Default for FwDelay {
    fn default() -> Self {
        Self {
            ctx: Words::new(BASE),
        }
    }
}

/// Linear interpolation of the adjacent packed samples at `at` by the 16-bit weight word.
fn tap(line: &[u32], weight_at: usize, at: usize, shift: i32) -> [f32; 2] {
    let word = if shift >= 0 {
        line[weight_at].checked_shl(shift as u32).unwrap_or(0)
    } else {
        line[weight_at]
            .checked_shr(shift.unsigned_abs())
            .unwrap_or(0)
    };
    let w = (word & 0xFFFF) as f32 * (1.0 / 32768.0);
    let (a, b) = (lanes(line[at]), lanes(line[at + 1]));
    [0, 1].map(|ch| w * a[ch] + (1.0 - w) * b[ch])
}

impl FwDelay {
    /// Builds the stage from the 70 raw context words (`I1 - 0x20 ..= I1 + 0x25`).
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

    /// The waveshaper of one fed-back sample `v`: `clip(0.3 v, bound)` as a fixed-point word, the
    /// signed 10-bit table index in bits 31..22 and the fraction selected by the control word.
    fn shape(c: &Words<CTX_WORDS>, io: &DelayIo, v: f32) -> f32 {
        let [fix, control, exponent] = ctx::SHAPER_FIX.map(|o| c.int(o));
        let w = fix_by(clip(SHAPER_SCALE * v, c.f(ctx::SHAPER_BOUND)), fix);
        let (pos, len) = ((control & 63) as u32, ((control >> 6) & 63) as u32);
        let field = u64::from(w) & ((1u64 << len) - 1);
        let frac = ((field << pos) as u32 as i32) as f32 * 2f32.powi(exponent);
        let index = io.shaper_center as i64 + i64::from((w as i32) >> 22);
        let at = |i: i64| {
            usize::try_from(i)
                .ok()
                .and_then(|i| io.shaper.get(i))
                .copied()
                .unwrap_or(0.0)
        };
        at(index) + frac * (at(index + 1) - at(index))
    }

    /// Processes one call: two stereo frames, the sends, the delay line and the context state.
    pub fn process(&mut self, io: &mut DelayIo) {
        let x = *io.frames;
        let old = *io.sends;
        let c = self.ctx;
        let cf = |o: i32| c.f(o);
        // level ratio, smoothed and clamped per channel
        let [hi, lo] = ctx::RATIO_HI_LO.map(&cf);
        let num = cf(7).max(cf(8)).clamp(FLOOR, 1.0);
        let ratio = num / cf(9).max(cf(10)).max(FLOOR);
        let kt = cf(ctx::RATIO_K);
        let ratio_ch: [f32; 2] = [0, 1].map(|ch| {
            (ratio - ratio * kt + cf(ctx::RATIO + ch) * kt)
                .min(hi)
                .max(lo)
        });
        // parameter smoothers; the left result is the one read back for both channels
        let g_dry = smooth(cf(4), cf(-0x20), K);
        let g_shape = smooth(cf(5), cf(-0x1F), K);
        let g_in = smooth(cf(-0x1B), cf(-0x14), K);
        let [ll, rr, xl, xr, rl, lr] = [
            (-0x1A, -0x12),
            (-0x19, -0x11),
            (-0x18, -0x10),
            (-0x17, -0xF),
            (-0x16, -0xE),
            (-0x15, -0xD),
        ]
        .map(|(t, s)| smooth(cf(t), cf(s), K));
        let (a1, a2) = (smooth(cf(-0xB), cf(-7), K), smooth(cf(-0xA), cf(-6), K));
        let w = &mut self.ctx;
        w.set(ctx::RATIO, ratio_ch[0]);
        w.set(ctx::RATIO + 1, ratio_ch[1]);
        for (off, v) in [
            (-0x20, g_dry),
            (-0x1F, g_shape),
            (-0x14, g_in),
            (-0x13, smooth(cf(-0x1B), cf(-0x13), K)),
            (-0x12, ll),
            (-0x11, rr),
            (-0x10, xl),
            (-0xF, xr),
            (-0xE, rl),
            (-0xD, lr),
            (-7, a1),
            (-6, a2),
            (-9, (1.0 + a1) / 2.0),
            (-8, (1.0 + a2) / 2.0),
        ] {
            w.set(off, v);
        }
        // two cascaded first-order all-passes over the previous sends
        let mut fed = [[0.0f32; 2]; 2];
        for ch in 0..2 {
            let (mut s1, mut s2) = (cf(-5 + ch as i32), cf(-3 + ch as i32));
            for (n, slot) in fed[ch].iter_mut().enumerate() {
                let v = (1.0 + a1) / 2.0 * old[2 * n + ch];
                let y1 = s1 + v;
                s1 = y1 * a1 - v;
                let u = (1.0 + a2) / 2.0 * y1;
                let y2 = s2 + u;
                s2 = u - y2 * a2;
                *slot = y2;
            }
            w.set(-5 + ch as i32, s1);
            w.set(-3 + ch as i32, s2);
            let meter = (cf(9 + ch as i32) * cf(ctx::METER_DECAY)).max(fed[ch][0].abs());
            w.set(9 + ch as i32, meter.max(fed[ch][1].abs()));
        }
        // waveshaped, level-normalised feedback packed into the write line
        let m4 = c.int(ctx::WRITE_OFFSET) as isize;
        for (n, frame_offset) in [0usize, 0x14].into_iter().enumerate() {
            let packed = [0, 1].map(|ch| {
                let v = fed[ch][n] * ratio_ch[ch];
                let shaped = Self::shape(&c, io, v);
                fpack((v - v * g_shape + shaped * SHAPER_GAIN * g_shape).to_bits())
            });
            io.line[(io.write_base as isize + m4) as usize + frame_offset] =
                packed[0] | (packed[1] << 16);
        }
        // fractional two-tap read, damping filter, outputs
        let (off, m5, shift) = (c.int(0x12) as isize, c.int(0x11) as isize, c.int(0x13));
        let at = |base: usize, o: isize, n: usize| (base as isize + o) as usize + 0x28 * n;
        let taps = [0, 1].map(|n| {
            let (weight_at, sample_at) = (at(io.read_base, off, n), at(io.read_base, m5, n));
            tap(io.line, weight_at, sample_at, shift)
        });
        let w = &mut self.ctx;
        let [b0, b1, a] = ctx::FILTER.map(&cf);
        for ch in 0..2 {
            let mut z = cf(ctx::FILTER_STATE + ch as i32);
            for n in 0..2 {
                let t = taps[n][ch];
                let y = t * b0 + z;
                z = t * b1 + y * a;
                io.frames[2 * n + ch] = x[2 * n + ch] * g_dry + y;
            }
            w.set(ctx::FILTER_STATE + ch as i32, z);
            w.set(ctx::TAPS + ch as i32, taps[0][ch]);
            w.set(ctx::TAPS + 2 + ch as i32, taps[1][ch]);
        }
        // sends for the next call and their peak holds
        let prev = [cf(0x1D), cf(0x1E), cf(0x1F), cf(0x20)];
        for n in 0..2 {
            let (l, r, pl, pr) = (x[2 * n], x[2 * n + 1], prev[2 * n], prev[2 * n + 1]);
            io.sends[2 * n] = l * g_in + (r + l) * xl + (pl * ll + pr * rl);
            io.sends[2 * n + 1] = r * g_in + (l + r) * xr + (pr * rr + pl * lr);
        }
        for ch in 0..2 {
            let meter = (cf(7 + ch as i32) * cf(ctx::METER_DECAY)).max(io.sends[ch].abs());
            w.set(7 + ch as i32, meter.max(io.sends[2 + ch].abs()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::fw_words::assert_close;
    use super::super::tests_delay_golden as g;
    use super::*;

    fn floats(words: &[u32]) -> Vec<f32> {
        words.iter().map(|w| f32::from_bits(*w)).collect()
    }

    /// An `io` over scratch buffers with the line split 0x80 words after the write base.
    fn run(f: &mut FwDelay, frames: &mut [f32; 4], line: &mut [u32]) {
        f.process(&mut DelayIo {
            frames,
            sends: &mut [0.0; 4],
            line,
            write_base: 0,
            read_base: 0x80,
            shaper: &[0.0; 3],
            shaper_center: 1,
        });
    }

    #[test]
    fn matches_the_firmware_model_over_three_calls() {
        let shaper = floats(&g::DLY_SHAPER);
        let mut line = g::DLY_LINE.to_vec();
        let mut sends: [f32; 4] = floats(&g::DLY_SENDS).try_into().unwrap();
        let mut f = FwDelay::from_words(g::DLY_CTX_IN);
        let ins = g::DLY_FRAMES_IN.as_chunks::<4>().0;
        let outs = g::DLY_FRAMES_OUT.as_chunks::<4>().0;
        assert_eq!((ins.len(), outs.len()), (3, 3));
        for (n, (inp, out)) in ins.iter().zip(outs).enumerate() {
            let mut frames = inp.map(f32::from_bits);
            f.process(&mut DelayIo {
                frames: &mut frames,
                sends: &mut sends,
                line: &mut line,
                write_base: 0,
                read_base: g::DLY_READ_BASE,
                shaper: &shaper,
                shaper_center: (shaper.len() - 1) / 2,
            });
            assert_close(
                &frames.map(f32::to_bits),
                out,
                GOLDEN_TOLERANCE,
                &format!("call {n}"),
            );
        }
        assert_close(
            f.words(),
            &g::DLY_CTX_OUT,
            GOLDEN_TOLERANCE,
            "final context",
        );
        assert_close(
            &sends.map(f32::to_bits),
            &g::DLY_SENDS_OUT,
            GOLDEN_TOLERANCE,
            "sends",
        );
        assert_eq!(line, g::DLY_LINE_OUT, "packed delay memory");
    }

    #[test]
    fn the_level_ratio_state_is_the_clamped_quotient_of_the_meters() {
        let mut f = FwDelay::default();
        for (o, v) in [
            (7, 0.4),
            (8, 0.4),
            (9, 0.8),
            (10, 0.8),
            (0x24, 1.0),
            (0x25, 0.01),
        ] {
            f.set_value(o, v);
        }
        let mut line = vec![0u32; 0x100];
        run(&mut f, &mut [0.0; 4], &mut line);
        assert!((f.value(ctx::RATIO) - 0.5).abs() < 1e-6 && f.value(ctx::RATIO) == f.value(0xD));
        f.set_value(0x24, 0.3); // upper clamp below the ratio
        run(&mut f, &mut [0.0; 4], &mut line);
        assert_eq!(f.value(ctx::RATIO), 0.3);
    }

    #[test]
    fn zero_context_is_silent_and_the_golden_vectors_are_not() {
        let mut f = FwDelay::default();
        let mut frames = [0.5, -0.25, 1.0, 0.125];
        run(&mut f, &mut frames, &mut vec![0u32; 0x100]);
        assert_eq!(frames, [0.0; 4], "no dry gain and no tap");
        assert!(g::DLY_FRAMES_IN != g::DLY_FRAMES_OUT && g::DLY_LINE != g::DLY_LINE_OUT);
    }
}
