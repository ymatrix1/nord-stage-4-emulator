//! Vox output stage `0x1229B3`: three 2-tap sections, two smoothed gains, fixed scale.
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1.6; method `docs/research/organ-post-stage.md`
//! section 4 and `organ-model-paths.md` section 4. The weighted sum of the 96-word post-stage
//! block is formed by [`super::vox_source::VoxSource`]; this module is what follows it:
//!
//! ```text
//! section i:  out = (T b0 + out' a0) + (T b1 + T' a1)           (T = input, ' = previous frame)
//! chain:      T -> section 1 -> section 2 -> section 3
//! gains:      g1 += 0.001 (G0 - g1),  g2 += 0.001 (G1 - g2)
//! output:     y = (out_3 g1 + out_1 g2) * 0.090075 * gain
//! ```
//!
//! The coefficients are the image values of the parameter block (the ARM install writes only
//! `G0 = 1.0`, `G1 = 0`), so at steady state `y = out_3 * 0.090075 * gain`. The sections are
//! defined per firmware frame; they are mapped to the engine rate with the bilinear transform of
//! their analog equivalent, which is the identity at the firmware frame rate.

use super::lane_mix::FIRMWARE_FRAME_RATE_HZ;

/// VERIFIED: fixed output scale of `0x1229B3` (`0x3DB8793E`, `tools/organ_post_ref.py` `K009008`).
pub const OUTPUT_SCALE: f32 = 0.090_075;
/// VERIFIED: smoothing step of both output gains per firmware frame (`0x3A83126F`).
const GAIN_STEP: f64 = 0.001;
/// VERIFIED: gain targets after the ARM install of model 1 (`G + 0 = 1.0`, `G + 1 = 0`,
/// `docs/research/organ-model-paths.md` section 4).
const GAIN_TARGETS: [f32; 2] = [1.0, 0.0];
/// ASSUMED: the gain word `DM(G + 0xB)` is the image default 1.0 (no ARM message found).
const GAIN_WORD: f32 = 1.0;

/// VERIFIED (image parameter block, `sharc-chain --dump 0xE2CC0:64`): section coefficients
/// `(a0, a1, b0, b1)` per firmware frame. Section 1 is a DC-blocking high-pass
/// (`y = 0.9944 (x - x') + 0.9891 y'`), sections 2 and 3 low-passes with DC gain 0.957.
pub const SECTIONS: [[f32; 4]; 3] = [
    [0.9891, -0.9944, 0.9944, 0.0],
    [0.9514, 0.0305, 0.0161, 0.0],
    [0.8348, 0.1035, 0.0546, 0.0],
];

/// One section in direct form: `y = bx x + bz x' + a y'`.
#[derive(Debug, Clone, Copy, Default)]
struct Section {
    bx: f32,
    bz: f32,
    a: f32,
    prev_in: f32,
    prev_out: f32,
}

impl Section {
    /// Section `(a0, a1, b0, b1)` of the firmware frame rate mapped to `fs`.
    fn design(coef: [f32; 4], fs: f64) -> Self {
        let [a0, a1, b0, b1] = coef.map(f64::from);
        // H(z) = (B0 + B1 z^-1) / (1 - a0 z^-1), B0 = b0 + b1, B1 = a1; analog equivalent by the
        // bilinear transform at F, back to z at fs: (n1 s + n0) / (d1 s + d0)
        let (big_b0, big_b1) = (b0 + b1, a1);
        let f2 = 2.0 * FIRMWARE_FRAME_RATE_HZ;
        let (n1, n0) = (big_b0 - big_b1, f2 * (big_b0 + big_b1));
        let (d1, d0) = (1.0 + a0, f2 * (1.0 - a0));
        let s2 = 2.0 * fs;
        let den = d1 * s2 + d0;
        Self {
            bx: ((n1 * s2 + n0) / den) as f32,
            bz: ((n0 - n1 * s2) / den) as f32,
            a: ((d1 * s2 - d0) / den) as f32,
            prev_in: 0.0,
            prev_out: 0.0,
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        let y = flush(self.bx * x + self.bz * self.prev_in + self.a * self.prev_out);
        self.prev_in = x;
        self.prev_out = y;
        y
    }
}

/// Flushes denormals and vanishing tails to zero (the tails decay geometrically).
#[inline]
fn flush(v: f32) -> f32 {
    if v.abs() < 1e-20 {
        0.0
    } else {
        v
    }
}

/// The three sections and the two smoothed gains.
#[derive(Debug, Clone)]
pub struct VoxOutput {
    sections: [Section; 3],
    gains: [f32; 2],
    gain_step: f32,
}

impl VoxOutput {
    /// Output stage for engine rate `fs`, gains settled at their targets (ASSUMED start state).
    pub fn new(fs: f32) -> Self {
        let mut o = Self {
            sections: [Section::default(); 3],
            gains: GAIN_TARGETS,
            gain_step: 0.0,
        };
        o.set_sample_rate(fs);
        o
    }

    /// Re-derives the section coefficients and the gain smoothing for `fs` (keeps the states).
    pub fn set_sample_rate(&mut self, fs: f32) {
        let fs = f64::from(fs.max(8000.0));
        for (s, c) in self.sections.iter_mut().zip(SECTIONS) {
            let fresh = Section::design(c, fs);
            *s = Section {
                prev_in: s.prev_in,
                prev_out: s.prev_out,
                ..fresh
            };
        }
        // same time constant in seconds: 1 - (1 - 0.001)^(F / fs)
        self.gain_step = (1.0 - (1.0 - GAIN_STEP).powf(FIRMWARE_FRAME_RATE_HZ / fs)) as f32;
    }

    /// One frame: `t` is the weighted sum of the note block; returns the output sample.
    pub fn process(&mut self, t: f32) -> f32 {
        let out1 = self.sections[0].process(t);
        let out2 = self.sections[1].process(out1);
        let out3 = self.sections[2].process(out2);
        for (g, target) in self.gains.iter_mut().zip(GAIN_TARGETS) {
            *g += self.gain_step * (target - *g);
        }
        (out3 * self.gains[0] + out1 * self.gains[1]) * OUTPUT_SCALE * GAIN_WORD
    }

    /// `true` once every section state has decayed to zero (silence test).
    pub fn is_idle(&self) -> bool {
        self.sections
            .iter()
            .all(|s| s.prev_in == 0.0 && s.prev_out == 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::super::vox_golden as g;
    use super::*;
    use std::f64::consts::PI;

    fn gain_at(fs: f32, hz: f64) -> f64 {
        let mut o = VoxOutput::new(fs);
        let n = (fs as usize) * 2;
        let (mut re, mut im, mut cnt) = (0.0f64, 0.0f64, 0usize);
        for i in 0..n {
            let w = 2.0 * PI * hz * i as f64 / f64::from(fs);
            let y = f64::from(o.process(w.sin() as f32));
            if i > n / 2 {
                re += y * w.sin();
                im += y * w.cos();
                cnt += 1;
            }
        }
        2.0 * re.hypot(im) / cnt as f64 / f64::from(OUTPUT_SCALE)
    }

    #[test]
    fn golden_output_matches_the_python_model() {
        // tools/organ_vox_golden.py: organ_post_ref.output_stage with the image sections,
        // weight 1 on a single note word, input = G_INPUT
        let mut o = VoxOutput::new(FIRMWARE_FRAME_RATE_HZ as f32);
        for (i, (&x, &want)) in g::OUT_INPUT.iter().zip(g::OUT_EXPECTED.iter()).enumerate() {
            let y = o.process(x);
            assert!((y - want).abs() < 1e-6, "frame {i}: {y} vs {want}");
        }
    }

    /// `|H|` of the three sections of the firmware frame rate at `hz`.
    fn analytic_gain(hz: f64) -> f64 {
        let w = 2.0 * PI * hz / FIRMWARE_FRAME_RATE_HZ;
        SECTIONS
            .iter()
            .map(|&[a0, a1, b0, b1]| {
                let (a0, a1, b0, b1) = (f64::from(a0), f64::from(a1), f64::from(b0), f64::from(b1));
                let num = ((b0 + b1) + a1 * w.cos()).hypot(-a1 * w.sin());
                let den = (1.0 - a0 * w.cos()).hypot(a0 * w.sin());
                num / den
            })
            .product()
    }

    #[test]
    fn dc_is_blocked_and_the_response_is_the_section_product() {
        let fw = FIRMWARE_FRAME_RATE_HZ as f32;
        // DC: the first section has a zero at z = 1
        let mut o = VoxOutput::new(fw);
        let mut y = 0.0;
        for _ in 0..200_000 {
            y = o.process(1.0);
        }
        assert!(y.abs() < 1e-4, "dc leaks {y}");
        // a band-pass shape: 0.12 at 10 Hz (DC blocker), peak 0.75 near 150 Hz, 0.23 at 1 kHz
        // (the two low-pass sections have corners near 340 Hz and 1.1 kHz)
        for hz in [150.0, 1000.0, 5000.0] {
            let (m, want) = (gain_at(fw, hz), analytic_gain(hz));
            assert!((m / want - 1.0).abs() < 0.02, "{hz} Hz: {m} vs {want}");
        }
        assert!((analytic_gain(150.0) - 0.7456).abs() < 1e-3);
        assert!((analytic_gain(10.0) - 0.1208).abs() < 1e-3);
        assert!((analytic_gain(1000.0) - 0.23).abs() < 0.02);
    }

    #[test]
    fn corner_frequencies_are_the_same_in_hz_at_every_engine_rate() {
        for hz in [15.0, 200.0, 2000.0, 8000.0, 15_000.0] {
            let a = gain_at(FIRMWARE_FRAME_RATE_HZ as f32, hz);
            let b = gain_at(48_000.0, hz);
            // bilinear without pre-warping: exact analog equivalence up to the warp of the
            // 43 kHz design; the two rates agree within 1.5 dB below 8 kHz, and lose the
            // 43 kHz warp above
            let tol = if hz > 8000.0 { 0.35 } else { 0.06 };
            assert!((a / b - 1.0).abs() < tol, "{hz} Hz: {a} vs {b}");
        }
    }

    #[test]
    fn gains_settle_at_their_targets_and_stay_finite() {
        let mut o = VoxOutput::new(48_000.0);
        o.gains = [0.0, 1.0];
        for _ in 0..200_000 {
            assert!(o.process(0.1).is_finite());
        }
        assert!((o.gains[0] - 1.0).abs() < 1e-4 && o.gains[1].abs() < 1e-4);
    }

    #[test]
    fn silence_in_silence_out_and_the_state_goes_idle() {
        let mut o = VoxOutput::new(48_000.0);
        for _ in 0..100 {
            o.process(1.0);
        }
        assert!(!o.is_idle());
        for _ in 0..200_000 {
            o.process(0.0);
        }
        assert!(o.is_idle());
        assert_eq!(o.process(0.0), 0.0);
    }
}
