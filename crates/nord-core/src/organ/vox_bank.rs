//! Vox note oscillator bank: the PNO Core1 post stage `0x12272D` (pitch LFO, 84 note oscillators
//! with waveshaper, asymmetry and note-tracking low-pass).
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1.7; method `docs/research/organ-post-stage.md`
//! section 3. The structure is VERIFIED bit-exact against the SHARC oracle
//! (`tools/organ_post_ref.py`); the constants below say which parts are read from the image and
//! which are closed forms. Per frame (the firmware frame rate `F`, converted to the engine rate by
//! `scale = F / fs`):
//!
//! ```text
//! lfo     phase += 0x905A3;  tri = clip(4 (|s| - 0.5), 1);  tri' = clip(tri - 0.08, 0.91) + 0.08
//! mod     tri' * depth + 2 (fine - 0.5);   m = 0.5 + mod / 2
//! note k  ph += m * rate_k;  x = clip((|s| - 0.5) coef_k, 1);  A = T[fix(x 512)]
//!         k in 12..47: A *= (A > 0 ? 1 : 0.9 + 0.2 s)               (s = signed phase, -1..1)
//!         Y = A cA + A' cB + Y' cC,   cA = cB = c_k,  cC = 1 - 2 c_k
//! ```
//!
//! Everything is allocated in `new()`; `step` only reads and writes the fixed arrays.

use super::lane_mix::FIRMWARE_FRAME_RATE_HZ;
use super::vox_image as img;
use super::vox_shaper::{blep_table, shape, TABLE_POINTS};
use std::f64::consts::PI;

/// Number of note oscillators.
pub const NOTES: usize = 84;
/// VERIFIED: phase advance of note 0 per frame at `m = 1`: `DM(0xE22B2)` of the PNO image,
/// `rate_k = 0x62C2E7 * 2^(k/12)` (max relative error 1.8e-6 over 84 entries,
/// `docs/research/organ-post-stage.md` section 3.2, `tools/organ_post_oracle.py`).
pub const RATE_BASE: f64 = 0x62C2E7 as f64;
/// VERIFIED: LFO phase increment per frame, `DM(0xE21F0)` update `+= 0x905A3`
/// (`tools/organ_post_ref.py` `LFO_STEP`, bit-exact against the oracle).
pub const LFO_STEP: f64 = 0x0009_05A3 as f64;
/// VERIFIED: note steepness `coef_k = 128.8 * 2^(-k/12)` (`DM(0x95CC6)`, fit to the image).
const COEF_BASE: f64 = 128.8;
/// VERIFIED: first and last note oscillator with the negative-half asymmetry (`LCNTR = 8` loop of
/// pc `0x122853`, notes 12..47).
pub const ASYM_NOTES: std::ops::Range<usize> = 12..48;
const COUNTS: f64 = 4_294_967_296.0;
/// One cycle of the 32.32 fixed-point phases.
#[cfg(test)]
const CYCLE: f64 = COUNTS * COUNTS;

/// EMPIRICAL: cutoff of the note low-pass of group `g` (notes `4g..4g+3` share one coefficient),
/// in units of the cycles per frame `f0` of the group's lowest note at `m = 0.5`. Derived from
/// `cA = DM(0x95D26 + 12 g + 4)` as `atan(c / (1 - c)) / pi / f0` and rounded to 4 digits; the
/// rebuilt coefficients agree with the image within 4.1e-5 relative
/// (`docs/research/organ-post-stage.md` section 3.3, `tools/organ_vox_golden.py cutoffs`).
const CUTOFF_RATIO: [f32; 21] = [
    1.9119, 1.8968, 1.8066, 1.9119, 1.8968, 1.8066, 1.4339, 1.3879, 1.3284, 1.0543, 1.2107, 1.3686,
    1.0863, 1.2933, 1.5055, 1.1949, 1.4226, 1.6605, 1.3179, 1.5134, 1.7108,
];

/// Cycles per frame of note `k` at `m = 0.5` (no vibrato, no fine tune).
pub fn cycles_per_frame(k: usize) -> f64 {
    0.5 * RATE_BASE * (k as f64 / 12.0).exp2() / COUNTS
}

/// Bilinear one-pole `Y = c (A + A') + (1 - 2c) Y'` of one note.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct NoteLowPass {
    pub(super) c: f32,
    pub(super) keep: f32,
    prev_in: f32,
    prev_out: f32,
}

impl NoteLowPass {
    fn process(&mut self, a: f32) -> f32 {
        let y = a * self.c + self.prev_in * self.c + self.prev_out * self.keep;
        self.prev_in = a;
        self.prev_out = y;
        y
    }
}

/// The 84 note oscillators with the pitch LFO (state preallocated, no allocation per sample).
#[derive(Debug, Clone)]
pub struct VoxBank {
    table: Box<[f32; TABLE_POINTS]>,
    /// Phases, 32 integer + 32 fractional bits (a wrap of the integer part is one cycle).
    phase: [u64; NOTES],
    lfo_phase: u64,
    rate: [f64; NOTES],
    coef: [f32; NOTES],
    pub(super) low_pass: [NoteLowPass; NOTES],
    /// `F / fs`: firmware frames per engine sample.
    scale: f64,
    fs: f64,
}

impl VoxBank {
    /// Bank for engine sample rate `fs` (firmware frame rate `FIRMWARE_FRAME_RATE_HZ`, VERIFIED).
    pub fn new(fs: f32) -> Self {
        let mut rate = [0.0; NOTES];
        let mut coef = [0.0f32; NOTES];
        for (k, (r, c)) in rate.iter_mut().zip(coef.iter_mut()).enumerate() {
            let semis = k as f64 / 12.0;
            *r = RATE_BASE * semis.exp2();
            *c = (COEF_BASE * (-semis).exp2()) as f32;
        }
        let mut bank = Self {
            table: blep_table(),
            phase: [0; NOTES],
            lfo_phase: 0,
            rate,
            coef,
            low_pass: [NoteLowPass::default(); NOTES],
            scale: 1.0,
            fs: 0.0,
        };
        bank.set_sample_rate(fs);
        bank
    }

    /// Re-derives the rate conversion and the low-pass coefficients (cutoffs stay at the same
    /// frequency in Hz) for engine rate `fs`.
    pub fn set_sample_rate(&mut self, fs: f32) {
        let fs = f64::from(fs.max(8000.0));
        self.fs = fs;
        self.scale = FIRMWARE_FRAME_RATE_HZ / fs;
        for (k, lp) in self.low_pass.iter_mut().enumerate() {
            let g = k / 4;
            let fc_cycles = f64::from(CUTOFF_RATIO[g]) * cycles_per_frame(4 * g);
            let fc_hz = (fc_cycles * FIRMWARE_FRAME_RATE_HZ).min(0.45 * fs);
            let tan = (PI * fc_hz / fs).tan();
            let c = tan / (1.0 + tan);
            lp.c = c as f32;
            lp.keep = (1.0 - 2.0 * c) as f32;
        }
    }

    /// Engine sample rate the bank was set for.
    pub fn sample_rate(&self) -> f32 {
        self.fs as f32
    }

    /// Advances the pitch LFO and returns `tri'` (about -0.83..0.99).
    fn lfo(&mut self) -> f32 {
        self.lfo_phase = self
            .lfo_phase
            .wrapping_add((LFO_STEP * self.scale * COUNTS) as u64);
        let s = signed_phase(self.lfo_phase).abs();
        let tri = (4.0 * (s - 0.5)).clamp(-1.0, 1.0);
        (tri - 0.08).clamp(-0.91, 0.91) + 0.08
    }

    /// One frame: the filtered note signals `Y_k` into `out`.
    ///
    /// `depth` is the vibrato depth `DM(0xE20E3)` as a pitch fraction, `pitch` the fine-tune
    /// factor `1 + 2 (DM(0xE20E0) - 0.5)` (1.0 = in tune): `m = 0.5 pitch + 0.5 depth tri'`.
    ///
    /// Returns `2 m` = `1 + mod`, the factor the sine bank's base word follows.
    pub fn step(&mut self, depth: f32, pitch: f32, out: &mut [f32; NOTES]) -> f32 {
        let tri = self.lfo();
        let m = (0.5 * pitch + 0.5 * depth * tri).max(0.0);
        let per_rate = f64::from(m) * self.scale * COUNTS;
        for (k, y) in out.iter_mut().enumerate() {
            let ph = self.phase[k].wrapping_add((per_rate * self.rate[k]) as u64);
            self.phase[k] = ph;
            let s = signed_phase(ph);
            let x = (s.abs() - 0.5) * self.coef[k];
            let mut a = shape(&self.table, x);
            if ASYM_NOTES.contains(&k) && a <= 0.0 {
                a *= 0.9 + 0.2 * s;
            }
            *y = self.low_pass[k].process(a);
        }
        2.0 * m
    }

    /// Loads the image start state: the 84 note phases and the LFO phase (`vox_image`).
    pub fn load_start_state(&mut self) {
        for (p, &w) in self.phase.iter_mut().zip(&img::NOTE_START) {
            *p = u64::from(w) << 32;
        }
        self.lfo_phase = u64::from(img::LFO_START) << 32;
    }

    /// Phase of note `k` as a signed fraction `-1..1` of half a cycle (test view).
    #[cfg(test)]
    pub(super) fn signed_phase(&self, k: usize) -> f32 {
        signed_phase(self.phase[k])
    }
}

/// The signed 32-bit phase `ph / 2^31` (`-1..1`) of a 32.32 fixed-point phase.
#[inline]
fn signed_phase(p: u64) -> f32 {
    ((p >> 32) as u32 as i32) as f32 / 2_147_483_648.0
}

#[cfg(test)]
mod tests {
    use super::super::vox_golden as g;
    use super::*;

    fn frame_rate_bank() -> VoxBank {
        VoxBank::new(FIRMWARE_FRAME_RATE_HZ as f32)
    }

    #[test]
    fn pitch_ladder_is_twelve_tet_from_the_image_base() {
        for k in 0..NOTES {
            let ratio = cycles_per_frame(k) / cycles_per_frame(0);
            assert!((ratio - (k as f64 / 12.0).exp2()).abs() < 1e-12);
        }
        // 36.2 Hz to 4.37 kHz at 48 kHz (docs/research/organ-post-stage.md section 3.2)
        let hz = |k| cycles_per_frame(k) * 48_000.0;
        assert!((hz(0) - 36.17).abs() < 0.1 && (hz(83) - 4_369.8).abs() < 1.0);
    }

    /// `(max, rms)` of `Rust - golden` over the records of one scenario.
    fn golden_error(depth: f32, notes: &[usize], golden: &[&[f32]]) -> (f32, f32) {
        let mut bank = frame_rate_bank();
        let mut out = [0.0f32; NOTES];
        let (mut max, mut sum, mut n) = (0.0f32, 0.0f64, 0usize);
        for f in 0..g::FRAMES {
            bank.step(depth, 1.0, &mut out);
            if f % g::STRIDE != 0 {
                continue;
            }
            for (i, &k) in notes.iter().enumerate() {
                let e = (out[k] - golden[f / g::STRIDE][i]).abs();
                max = max.max(e);
                sum += f64::from(e * e);
                n += 1;
            }
        }
        (max, (sum / n as f64).sqrt() as f32)
    }

    #[test]
    fn golden_note_signals_match_the_python_model() {
        // tools/organ_vox_golden.py: organ_post_ref.scanner + smooth (bit-exact SHARC model) on
        // closed-form tables, no vibrato; one waveshaper index flip from f32 against 40-bit
        // rounding is the largest difference
        let golden: Vec<&[f32]> = g::Y_PLAIN.iter().map(|r| &r[..]).collect();
        let (max, rms) = golden_error(0.0, &g::NOTES_PLAIN, &golden);
        assert!(max < 1e-4 && rms < 1e-5, "max {max} rms {rms}");
    }

    #[test]
    fn golden_note_signals_with_vibrato_match_the_python_model() {
        let golden: Vec<&[f32]> = g::Y_VIB.iter().map(|r| &r[..]).collect();
        let (max, rms) = golden_error(g::DEPTH, &g::NOTES_VIB, &golden);
        assert!(max < 1e-3 && rms < 1e-4, "max {max} rms {rms}");
    }

    #[test]
    fn frame_rate_puts_note_zero_on_c1() {
        // rate_0 = 0x62C2E7 at m = 0.5 is C1 (MIDI 24, the ASSUMED key map `note - 24`) to 0.001
        // cent at the DSP frame rate (the word is round(C1 / F * 2^33)); at the old 43,156.4 Hz it was 9.8 cent flat, the "9.9 cent below
        // the Pipe bank" of OQ17 (the Pipe bank at unity tune is 9.6 cent sharp of C1 there, at the
        // ARM's Fine Tune word it is on C1 as well).
        let c1 = 440.0 * (-45.0f64 / 12.0).exp2();
        let cents = |hz: f64| 1200.0 * (hz / c1).log2();
        let at_fw = cycles_per_frame(0) * FIRMWARE_FRAME_RATE_HZ;
        assert!(cents(at_fw).abs() < 1e-3, "{}", cents(at_fw));
        assert!((cents(cycles_per_frame(0) * 43_156.4) + 9.84).abs() < 0.1);
        // octaves stay exact: note 12 is C2
        assert!((cycles_per_frame(12) / cycles_per_frame(0) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn lfo_runs_at_the_firmware_rate_in_hz() {
        for fs in [48_000.0f32, 44_100.0, FIRMWARE_FRAME_RATE_HZ as f32] {
            let mut bank = VoxBank::new(fs);
            let n = (fs * 4.0) as usize; // 4 s
            let (mut crossings, mut prev) = (0, bank.lfo());
            let (mut lo, mut hi) = (f32::MAX, f32::MIN);
            for _ in 0..n {
                let t = bank.lfo();
                lo = lo.min(t);
                hi = hi.max(t);
                if prev < 0.08 && t >= 0.08 {
                    crossings += 1;
                }
                prev = t;
            }
            let hz = f64::from(crossings) / 4.0;
            let want = LFO_STEP / COUNTS * FIRMWARE_FRAME_RATE_HZ;
            assert!((want - 5.975).abs() < 0.01, "{want}"); // 0x905A3 / 2^32 * 43,402.34
            assert!((hz - want).abs() < 0.3, "{fs}: {hz} Hz");
            assert!(
                (lo + 0.83).abs() < 0.01 && (hi - 0.99).abs() < 0.01,
                "{lo} {hi}"
            );
        }
    }

    #[test]
    fn vibrato_depth_is_the_peak_pitch_deviation() {
        let depth = 0.0176f32;
        let mut bank = frame_rate_bank();
        let mut out = [0.0f32; NOTES];
        let mut ratios = Vec::new();
        for _ in 0..8000 {
            let before = bank.phase[40];
            bank.step(depth, 1.0, &mut out);
            let adv = bank.phase[40].wrapping_sub(before) as f64 / CYCLE;
            ratios.push(adv / cycles_per_frame(40));
        }
        let hi = ratios.iter().cloned().fold(f64::MIN, f64::max);
        let lo = ratios.iter().cloned().fold(f64::MAX, f64::min);
        assert!((hi - 1.0 - 0.99 * f64::from(depth)).abs() < 1e-4, "{hi}");
        assert!((lo - 1.0 + 0.83 * f64::from(depth)).abs() < 1e-4, "{lo}");
    }

    #[test]
    fn note_pitch_follows_the_ladder_at_any_engine_rate() {
        for fs in [48_000.0f32, 44_100.0] {
            let mut bank = VoxBank::new(fs);
            let mut out = [0.0f32; NOTES];
            bank.step(0.0, 1.0, &mut out);
            for k in [0usize, 12, 36, 60, 83] {
                let before = bank.phase[k];
                bank.step(0.0, 1.0, &mut out);
                let cycles = bank.phase[k].wrapping_sub(before) as f64 / CYCLE;
                let hz = cycles * f64::from(fs);
                let want = cycles_per_frame(k) * FIRMWARE_FRAME_RATE_HZ;
                assert!((hz / want - 1.0).abs() < 1e-6, "{fs} {k}: {hz} {want}");
            }
        }
    }

    #[test]
    fn outputs_are_finite_and_bounded() {
        let mut bank = VoxBank::new(48_000.0);
        let mut out = [0.0f32; NOTES];
        for _ in 0..50_000 {
            bank.step(0.0176, 1.0, &mut out);
            assert!(out.iter().all(|v| v.is_finite() && v.abs() < 1.5));
        }
    }
}
