//! Firmware Ladder24 filter (Core2 stage `0x12549D`, `kFltType = 2`) as an opt-in filter.
//!
//! Spec: `docs/specs/synth-engine.md` section 4.1; evidence
//! `docs/research/synth-stage-models-9.md`; bit-exact model `tools/synth_filter_ladder.py`
//! (1840 of 1840 oracle words over 40 trials); goldens `tests_fw_ladder_golden.rs`.
//!
//! Four identical one-pole sections in cascade (`n(j+1) = (a o(j) + b n(j)) - c o(j+1)`, DC gain
//! one) with a global feedback: the output goes through the soft clip table `T`, a DC blocker
//! (`h0 s0 + h1 t - h2 s2`) and is subtracted from the input. The stage computes the four
//! coefficient words of the **next** call from the cutoff position `p` (the strip cutoff word
//! `R`, a float `0..1`) and the resonance word `q`:
//!
//! ```text
//! b = row0(p)   a = row1(p)   c = row2(p)   kf = row4(p) * q / 4       q = (idx + fr) / 128
//! ```

use super::fw_filter::TablePos;
use super::fw_ladder_tables::{ROW0, ROW1, ROW2, ROW4, SHAPER};

/// VERIFIED: image words `DM(0xC28F6 / 0xC28F4 / 0xC28F8)` (`h0`, `h1`, `h2` of the DC blocker),
/// the operands of the listing `0x24A98C..0x24A9C0`, checked by the oracle (words equal).
const H0: f32 = -0.999_457_4;
const H1: f32 = 0.999_457_4;
const H2: f32 = -0.998_914_84;
/// VERIFIED: `DM(0xC28DA) = 0.25`, the scale of the resonance in `kf`.
const KF_SCALE: f32 = 0.25;

/// Coefficients of one call (the words `-18, -16, -14, -12` of the frame).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LadderCoefs {
    /// Section input weight `b` (row 0).
    pub b: f32,
    /// Section previous-input weight `a` (row 1).
    pub a: f32,
    /// Section pole term `c` (row 2, negative: the pole is `-c`).
    pub c: f32,
    /// Feedback gain `kf` in front of the shaper.
    pub kf: f32,
}

#[inline]
fn lerp_row(row: &[u32; 129], idx: usize, fr: f32) -> f32 {
    let lo = f32::from_bits(row[idx]);
    let hi = f32::from_bits(row[idx + 1]);
    lo * (1.0 - fr) + hi * fr
}

impl LadderCoefs {
    /// Coefficients for the cutoff position `p` and the resonance position `res`.
    ///
    /// VERIFIED: equal to `tools/synth_filter_ladder.py::coefficients` within float32 rounding
    /// (test `coefficient_words_match_the_model`). `p` is clamped to `0..0.99` (the firmware word
    /// is the strip cutoff `R`, below 1).
    #[inline]
    pub fn new(p: f32, res: TablePos) -> Self {
        let p = if p.is_finite() {
            p.clamp(0.0, 0.992)
        } else {
            0.0
        };
        let fixed = (f64::from(p) * 2_147_483_648.0).round_ties_even() as i64;
        let idx = ((fixed >> 24) & 0x7F) as usize;
        let fr = (fixed & 0xFF_FFFF) as f32 / 16_777_216.0;
        let q = (res.idx as f32 + res.fr) / 128.0;
        Self {
            b: lerp_row(&ROW0, idx, fr),
            a: lerp_row(&ROW1, idx, fr),
            c: lerp_row(&ROW2, idx, fr),
            kf: lerp_row(&ROW4, idx, fr) * (q * KF_SCALE),
        }
    }

    /// Coefficients for the strip cutoff coefficient `r = tan(pi fc / fs)` of the other filter
    /// stages: the position `p` is chosen so that the section pole `-c` equals `exp(-2 pi fc / fs)`
    /// (`2 atan r` rad per sample), the one-pole frequency that is also the resonance peak of the
    /// four-section loop.
    ///
    /// ASSUMED: the firmware strip word that the ARM writes for `kFltType = 2` and its relation to
    /// the table position `p` were not traced (`synth-stage-models-9.md` section 4); only the
    /// table (position to pole) is measured. The search inverts the 129 entry row 2 (monotone).
    #[inline]
    pub fn from_r(r: f32, res: TablePos) -> Self {
        let r = if r.is_finite() { r.max(0.0) } else { 0.0 };
        let target = -(-2.0 * r.atan()).exp();
        let (mut lo, mut hi) = (0usize, 128usize);
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if f32::from_bits(ROW2[mid]) <= target {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let (c0, c1) = (f32::from_bits(ROW2[lo]), f32::from_bits(ROW2[lo + 1]));
        let fr = ((target - c0) / (c1 - c0)).clamp(0.0, 1.0);
        Self::new((lo as f32 + fr) / 128.0, res)
    }

    /// Coefficients from the frame words (`w30` cutoff float word, `w28` resonance word).
    #[inline]
    pub fn from_words(w30: u32, w28: u32) -> Self {
        Self::new(f32::from_bits(w30), TablePos::from_word(w28))
    }
}

/// Soft clip `T(x)` of the feedback: linear interpolation of the 1025 word table at
/// `FIX(x, 31) >> 22` (input saturates at the 32-bit range).
#[inline]
fn shaper(x: f32) -> f32 {
    let p = (f64::from(x) * 2_147_483_648.0)
        .round_ties_even()
        .clamp(-2_147_483_648.0, 2_147_483_647.0) as i64;
    let i = (p >> 22) as i32;
    let fr = (p & 0x3F_FFFF) as f32 / 4_194_304.0;
    let lo = f32::from_bits(SHAPER[(i + 512) as usize]);
    let hi = f32::from_bits(SHAPER[(i + 513) as usize]);
    lo + (hi - lo) * fr
}

/// Flushes values too small to matter to zero so the states never become denormal.
#[inline]
fn flush(x: f32) -> f32 {
    if x.abs() < 1e-24 {
        0.0
    } else {
        x
    }
}

/// State of one Ladder24 lane. `Copy`, no heap.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FwLadder {
    /// Section states (frame words `-10, -8, -6, -4`).
    sect: [f32; 4],
    /// Last output (`-2`), shaper output (`0`), feedback state (`2`).
    y: f32,
    t: f32,
    fb: f32,
}

impl FwLadder {
    /// Filters one sample. A non-finite input is treated as silence.
    #[inline]
    pub fn process(&mut self, x: f32, co: &LadderCoefs) -> f32 {
        let x = if x.is_finite() { x } else { 0.0 };
        let t = shaper(co.kf * self.y);
        let fb = (H0 * self.t + H1 * t) - H2 * self.fb;
        self.t = flush(t);
        self.fb = flush(fb);
        let u = x - fb;
        let old = [
            self.sect[0],
            self.sect[1],
            self.sect[2],
            self.sect[3],
            self.y,
        ];
        let mut n = u;
        let mut new = [u; 4];
        for j in 0..4 {
            n = (co.a * old[j] + co.b * new[j]) - co.c * old[j + 1];
            if j < 3 {
                new[j + 1] = n;
            }
        }
        for (s, v) in self.sect.iter_mut().zip(new) {
            *s = flush(v);
        }
        self.y = if n.is_finite() { flush(n) } else { 0.0 };
        self.y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dc_gain_is_one_without_feedback() {
        let co = LadderCoefs::new(0.3, TablePos { idx: 0, fr: 0.0 });
        assert_eq!(co.kf, 0.0);
        let mut f = FwLadder::default();
        let mut y = 0.0;
        for _ in 0..20_000 {
            y = f.process(0.5, &co);
        }
        assert!((y - 0.5).abs() < 2e-3, "dc {y}");
    }

    #[test]
    fn coefficients_follow_the_rows_and_the_resonance_scale() {
        let co = LadderCoefs::new(0.0, TablePos { idx: 64, fr: 0.0 });
        assert_eq!(co.b.to_bits(), ROW0[0]);
        assert_eq!(co.c.to_bits(), ROW2[0]);
        let want = f32::from_bits(ROW4[0]) * 0.5 * 0.25;
        assert!((co.kf - want).abs() < 1e-6);
        // the pole `-c` rises with the cutoff position
        let lo = LadderCoefs::new(0.1, TablePos { idx: 0, fr: 0.0 });
        let hi = LadderCoefs::new(0.8, TablePos { idx: 0, fr: 0.0 });
        assert!(-lo.c > -hi.c);
    }

    #[test]
    fn cutoff_coefficient_sets_the_section_pole() {
        let res = TablePos { idx: 0, fr: 0.0 };
        for fc_over_fs in [0.001f32, 0.01, 0.05, 0.15, 0.3] {
            let r = (std::f32::consts::PI * fc_over_fs).tan();
            let co = LadderCoefs::from_r(r, res);
            let want = (-std::f32::consts::TAU * fc_over_fs).exp();
            assert!(
                (-co.c - want).abs() < 2e-3 * want.max(0.05),
                "{fc_over_fs}: {} vs {want}",
                -co.c
            );
        }
        // outside the table the position saturates
        assert_eq!(LadderCoefs::from_r(1e-9, res).b.to_bits(), ROW0[0]);
        assert!(LadderCoefs::from_r(f32::NAN, res).b.is_finite());
    }

    #[test]
    fn feedback_stays_bounded_and_guards_non_finite_input() {
        let co = LadderCoefs::new(0.5, TablePos { idx: 127, fr: 0.9 });
        let mut f = FwLadder::default();
        for n in 0..50_000 {
            let x = if n % 977 == 0 { 1.0 } else { 0.0 };
            let y = f.process(x, &co);
            assert!(y.is_finite() && y.abs() < 50.0, "{n}: {y}");
        }
        assert!(f.process(f32::NAN, &co).is_finite());
    }

    #[test]
    fn shaper_is_odd_linear_near_zero_and_saturates() {
        assert_eq!(shaper(0.0), 0.0);
        assert!((shaper(0.01) + shaper(-0.01)).abs() < 1e-7);
        assert!((shaper(0.001) - 6.0 * 0.001).abs() < 1e-3);
        assert!((shaper(1e9) - 0.6316).abs() < 1e-3);
        assert!((shaper(-1e9) + 0.6316).abs() < 1e-3);
    }
}
