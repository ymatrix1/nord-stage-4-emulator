//! Firmware Amp **drive** stage (FX routine `0x1228CF`, module list `0xC04438D8#0`) as an opt-in
//! stage. Formerly called the "table LFO": role correction of `docs/research/fx-contexts.md` s5.2.
//! The phase word `ctx[-0x20]` is `kAmpDrive / 127` (VERIFIED words, `fw_map::amp_drive_target`),
//! so the "phase" is the drive amount and the tables are drive curves, not an oscillator. The
//! equations below are unchanged (the word "phase" is the model's name).
//!
//! A smoothed phase (`2^31` per half turn in the firmware's integer, `ctx[-2]` sets the scale)
//! indexes two float tables (the `-cos` quarter wave and its companion) with linear
//! interpolation, giving the gains `G1` and `G2`. Per block of two stereo frames and channel:
//! a transposed direct form II section, a differentiating high-pass `e = p (a_n - a_{n-1}) - q
//! e_{n-1}` of the section output scaled by `G1`, a table waveshaper of `e` (interpolated in a
//! third table) mixed with the dry section output, a second section and a second high-pass.
//!
//! VERIFIED: `tools/fx_model_lfo.py` reproduces the routine word for word against the `sharc-fx`
//! oracle (3,600 random scenarios, per-instruction register compare); `tools/fx_closed_forms.py::lfo`
//! holds these equations (agreeing with the model to 2e-3 where a table index sits on a rounding
//! boundary, 1e-4 otherwise) and `tests_lfo_golden.rs` the golden vectors, matched to
//! [`GOLDEN_TOLERANCE`]. ASSUMED: the musical meaning of the words and which ARM parameter writes
//! which target. The tables are the caller's (the firmware reads them through the pointer words
//! `ctx[-0x15]`, `ctx[-0x14]`, `ctx[2]`); reads outside a slice return zero.

use super::fw_words::{clip, fix_by, smooth, Words};

/// Number of context words (`I1 - 0x20 ..= I1 + 0x15`).
pub const CTX_WORDS: usize = 54;
const BASE: i32 = -0x20;

/// Relative tolerance of this f32 port against the 40-bit golden vectors (measured worst case
/// between 1e-5 and 2e-5 over outputs and context (the high-pass difference cancels)).
#[cfg(test)]
pub(super) const GOLDEN_TOLERANCE: f32 = 3.0e-5;

/// Word offsets from `I1`. VERIFIED against the oracle; roles EMPIRICAL.
pub mod ctx {
    /// Phase target (broadcast) and, at `+ 1`, the smoothed phase state (written back, both words).
    pub const PHASE_TARGET: i32 = -0x20;
    /// `FIX` scale of the phase and of the waveshaper index (31 gives Q31).
    pub const FIX_SCALE: i32 = -2;
    /// Gain of the first table output (`G1 = lerp(A) * this`).
    pub const G1_SCALE: i32 = -0xD;
    /// `G2 = lerp(B) * (phase * SLOPE + OFFSET)`.
    pub const G2_SLOPE: i32 = -0xC;
    /// Offset of `G2`.
    pub const G2_OFFSET: i32 = -0xB;
    /// Clip bound of the dry/wet mix (`mix = clip(20 phase, bound)`) and the reference of `1 - mix`.
    pub const MIX_BOUND: i32 = 0;
    /// Clip bound of the waveshaper input.
    pub const SHAPER_BOUND: i32 = 1;
    /// High-pass coefficients `p` and `q`.
    pub const HP: [i32; 2] = [-0x13, -0x12];
    /// First section coefficients `b0 b1 b2 a1 a2` (floats).
    pub const SECTION1: [i32; 5] = [7, 8, 9, 0xA, 0xB];
    /// Second section coefficients `b0 b1 b2 a1 a2` (floats).
    pub const SECTION2: [i32; 5] = [0x10, 0x11, 0x12, 0x13, 0x14];
}

/// The three tables the routine reads: `a` and `b` have 257 entries (index `0 ..= 255`, plus one
/// for the interpolation), `c` is indexed by a signed 10-bit field around `c_center`.
#[derive(Debug, Clone, Copy)]
pub struct DriveTables<'a> {
    /// First LFO table (the `-cos` quarter wave in the firmware).
    pub a: &'a [f32],
    /// Second LFO table.
    pub b: &'a [f32],
    /// Waveshaper table.
    pub c: &'a [f32],
    /// Index in `c` of the entry for field value 0.
    pub c_center: usize,
}

/// Firmware table LFO stage: the 54 context words. Allocation free.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FwAmpDrive {
    ctx: Words<CTX_WORDS>,
}

impl Default for FwAmpDrive {
    fn default() -> Self {
        Self {
            ctx: Words::new(BASE),
        }
    }
}

/// The smoothing coefficient `0x3F7DA3C2` (0.99078) of the ORG, delay and LFO routines.
fn smoothing() -> f32 {
    f32::from_bits(0x3F7D_A3C2)
}

/// Table entry at signed `index` (zero outside the slice).
#[inline]
fn at(table: &[f32], index: i64) -> f32 {
    usize::try_from(index)
        .ok()
        .and_then(|i| table.get(i))
        .copied()
        .unwrap_or(0.0)
}

/// `T[i] + frac (T[i + 1] - T[i])`.
#[inline]
fn lerp(table: &[f32], index: i64, frac: f32) -> f32 {
    let lo = at(table, index);
    lo + frac * (at(table, index + 1) - lo)
}

/// Transposed direct form II step of the firmware: `y = b0 x + z1; z1' = b1 x + z2 - a1 y;
/// z2' = b2 x - a2 y`.
#[inline]
fn tdf2(x: f32, z: &mut [f32; 2], c: &[f32; 5]) -> f32 {
    let y = x * c[0] + z[0];
    z[0] = c[1] * x + z[1] - c[3] * y;
    z[1] = c[2] * x - c[4] * y;
    y
}

impl FwAmpDrive {
    /// Builds the stage from the 54 raw context words (`I1 - 0x20 ..= I1 + 0x15`).
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

    /// Processes whole blocks in place: each block of 4 words is two stereo frames
    /// `[l0, r0, l1, r1]`; the loop count word `ctx[0x15]` of the firmware is the number of blocks
    /// and is not read here.
    pub fn process(&mut self, tables: &DriveTables, frames: &mut [f32]) {
        let c = &mut self.ctx;
        let phase = smooth(
            c.f(ctx::PHASE_TARGET),
            c.f(ctx::PHASE_TARGET + 1),
            smoothing(),
        );
        c.set_pair(ctx::PHASE_TARGET + 1, phase);
        let shift = c.int(ctx::FIX_SCALE);
        let word = fix_by(phase, shift);
        let (index, frac) = (
            i64::from(word >> 24),
            (word & 0xFF_FFFF) as f32 / 16_777_216.0,
        );
        let g1 = lerp(tables.a, index, frac) * c.f(ctx::G1_SCALE);
        let g2 = lerp(tables.b, index, frac) * (phase * c.f(ctx::G2_SLOPE) + c.f(ctx::G2_OFFSET));
        let bound = c.f(ctx::MIX_BOUND);
        let mix = clip(phase * 20.0, bound);
        c.set_pair(-0xA, g1);
        c.set_pair(-8, g2);
        c.set_pair(-0x1D, mix);
        c.set_pair(-0x1B, bound - mix);
        let [p, q] = ctx::HP.map(|o| c.f(o));
        let shaper_bound = c.f(ctx::SHAPER_BOUND);
        let (s1, s2) = (ctx::SECTION1.map(|o| c.f(o)), ctx::SECTION2.map(|o| c.f(o)));
        for ch in 0..2 {
            let mut z1 = [c.f(3 + ch), c.f(5 + ch)];
            let mut z2 = [c.f(0xC + ch), c.f(0xE + ch)];
            let (mut a_prev, mut e_prev) = (c.f(-0x11 + ch), c.f(-0xF + ch));
            let (mut v_prev, mut e2_prev) = (c.f(-6 + ch), c.f(-4 + ch));
            let (blocks, _) = frames.as_chunks_mut::<4>();
            for block in blocks {
                for fr in 0..2 {
                    let slot = 2 * fr + ch as usize;
                    let y = tdf2(block[slot], &mut z1, &s1);
                    c.set(-0x19 + 2 * fr as i32 + ch, y);
                    let a_now = y * g1;
                    let e = p * (a_now - a_prev) - q * e_prev;
                    (a_prev, e_prev) = (a_now, e);
                    let w = fix_by(clip(e, shaper_bound), shift);
                    let shaped = lerp(
                        tables.c,
                        tables.c_center as i64 + i64::from((w as i32) >> 22),
                        (w & 0x3F_FFFF) as f32 / 4_194_304.0,
                    );
                    let z = shaped * g2 * mix + y * (bound - mix);
                    let v = 2.0 * tdf2(z, &mut z2, &s2);
                    let e2 = p * (v - v_prev) - q * e2_prev;
                    (v_prev, e2_prev) = (v, e2);
                    block[slot] = e2;
                }
            }
            c.set(-0x11 + ch, a_prev);
            c.set(-0xF + ch, e_prev);
            c.set(-6 + ch, v_prev);
            c.set(-4 + ch, e2_prev);
            c.set(3 + ch, z1[0]);
            c.set(5 + ch, z1[1]);
            c.set(0xC + ch, z2[0]);
            c.set(0xE + ch, z2[1]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::fw_words::assert_close;
    use super::super::tests_lfo_golden as g;
    use super::*;

    fn floats(words: &[u32]) -> Vec<f32> {
        words.iter().map(|w| f32::from_bits(*w)).collect()
    }

    #[test]
    fn matches_the_firmware_model_over_three_calls() {
        let (a, b, c) = (
            floats(&g::LFO_TABLE_A),
            floats(&g::LFO_TABLE_B),
            floats(&g::LFO_TABLE_C),
        );
        let tables = DriveTables {
            a: &a,
            b: &b,
            c: &c,
            c_center: (c.len() - 1) / 2,
        };
        let mut f = FwAmpDrive::from_words(g::LFO_CTX_IN);
        let (ins, outs) = (
            g::LFO_FRAMES_IN.as_chunks::<8>().0,
            g::LFO_FRAMES_OUT.as_chunks::<8>().0,
        );
        assert_eq!((ins.len(), outs.len()), (3, 3));
        for (n, (inp, out)) in ins.iter().zip(outs).enumerate() {
            let mut frames = inp.map(f32::from_bits);
            f.process(&tables, &mut frames);
            assert_close(
                &frames.map(f32::to_bits),
                out,
                GOLDEN_TOLERANCE,
                &format!("call {n}"),
            );
        }
        assert_close(
            f.words(),
            &g::LFO_CTX_OUT,
            GOLDEN_TOLERANCE,
            "final context",
        );
    }

    #[test]
    fn the_phase_is_smoothed_toward_its_target_and_picks_the_table_entry() {
        let a: Vec<f32> = (0..257).map(|k| k as f32).collect();
        let tables = DriveTables {
            a: &a,
            b: &a,
            c: &[0.0; 3],
            c_center: 1,
        };
        let mut f = FwAmpDrive::default();
        f.ctx.set_word(ctx::FIX_SCALE, 31);
        f.set_value(ctx::PHASE_TARGET, 0.25);
        f.set_value(ctx::PHASE_TARGET + 1, 0.25); // state == target: the phase stays 0.25
        f.set_value(ctx::G1_SCALE, 1.0);
        f.process(&tables, &mut []);
        assert_eq!(f.value(ctx::PHASE_TARGET + 1), 0.25);
        assert_eq!(f.value(-0xA), 32.0, "index 0.25 * 128 = 32, fraction 0");
        assert_eq!(f.value(-9), 32.0, "both words of the pair");
    }

    #[test]
    fn table_reads_outside_the_slice_are_zero() {
        assert_eq!(at(&[1.0, 2.0], -1), 0.0);
        assert_eq!(at(&[1.0, 2.0], 2), 0.0);
        assert_eq!(lerp(&[1.0, 3.0], 0, 0.5), 2.0);
        assert_eq!(
            lerp(&[1.0, 3.0], 1, 0.5),
            1.5,
            "the entry after the end reads as zero"
        );
    }

    #[test]
    fn the_second_section_is_a_transposed_direct_form_two_biquad() {
        let mut z = [0.0f32; 2];
        let c = [1.0, 0.5, 0.25, 0.1, 0.05];
        let y: Vec<f32> = [1.0, 0.0, 0.0]
            .iter()
            .map(|x| tdf2(*x, &mut z, &c))
            .collect();
        assert_eq!(y[0], 1.0);
        assert!(
            (y[1] - 0.4).abs() < 1e-7 && (y[2] - 0.16).abs() < 1e-7,
            "{y:?}"
        );
    }

    #[test]
    fn the_golden_vectors_are_not_silent() {
        assert!(g::LFO_FRAMES_IN != g::LFO_FRAMES_OUT && g::LFO_CTX_IN != g::LFO_CTX_OUT);
    }
}
