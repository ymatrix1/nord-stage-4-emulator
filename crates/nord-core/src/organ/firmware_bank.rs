//! Tonewheel oscillator bank with the arithmetic of the PNO Core1 routine at pc `0x122620`.
//!
//! Per frame and element: `phase += step` (u32, wraps at 2^32), `idx = phase >> 20`,
//! `out = T[idx]` from a 4096-point table, no interpolation. Research and oracle run:
//! `docs/research/organ-tonewheel-bank.md` (sections 1, 2.1, 4); Python model
//! `tools/organ_bank_ref.py`.
//!
//! Real-time: `advance` only touches pre-allocated arrays. The table is built once in `new`.

use super::types::NUM_TONEWHEELS;
use super::vibrato_bank::{
    copy_ratio, copy_weights, lane_matrix, BANK_WHEELS, GAINS_DETUNED, GAINS_EXACT,
};
use std::f64::consts::TAU;

/// Number of table entries (`2^12`).
///
/// VERIFIED: 4096 (x, y) pairs at DM `0xC042B`, `FEXT` of 12 bits at bit 20 (oracle, 29,440 words
/// equal to `tools/organ_bank_ref.py compare`, 0 mismatches).
pub const TABLE_LEN: usize = 4096;

/// Right shift from the u32 phase to the table index.
///
/// VERIFIED: `idx = phase >> 20` (same oracle run as `TABLE_LEN`).
pub const INDEX_SHIFT: u32 = 20;

/// Levels in dB of harmonics 2..=5 of the firmware x waveform relative to the fundamental.
///
/// VERIFIED: a least-squares fit of the 4096-point firmware table (DM `0xC042B`, x entries) to
/// `sin t + sum c_h cos(h t)`, h = 2..8, gives `c = -0.112202, -0.0158489, -0.00630957,
/// -0.00398107` for h = 2..5 and below 1e-8 for h = 6..8 and every sine term, i.e. exactly
/// `-10^(dB/20)` with these levels; max error of the fit against the table 7.8e-8 (f32 rounding).
/// The earlier 4-digit coefficients (0.1122, 0.01585, 0.00631, 0.00398) were within 3.7e-6.
/// Reproduce: `python tools/organ_oracle_audio.py table SRAM/core1_sram.bin`. The phase origin
/// (index 0 = `t = 0`) is VERIFIED by the same fit (all sine terms vanish for h >= 2).
const X_HARMONIC_DB: [f64; 4] = [-19.0, -36.0, -44.0, -48.0];

/// Closed form of the firmware x waveform at phase `t` (radians).
pub fn firmware_wave_x(t: f64) -> f64 {
    let mut v = t.sin();
    for (k, db) in X_HARMONIC_DB.iter().enumerate() {
        v -= 10f64.powf(db / 20.0) * ((k as f64 + 2.0) * t).cos();
    }
    v
}

/// Builds the 4096-point x table once: `T[i] = x(2 pi i / 4096)`.
pub fn build_x_table() -> Box<[f32]> {
    (0..TABLE_LEN)
        .map(|i| firmware_wave_x(TAU * i as f64 / TABLE_LEN as f64) as f32)
        .collect()
}

/// Signed fractional multiply of the SHARC `SSFR` form: `a * b / 2^31` rounded to nearest.
///
/// VERIFIED: `R1 = PM(I12); R1 = R1 * R4 (SSFR)` at `0x122643`, equal to the oracle on odd words
/// (`tools/organ_bank_ref.py fracmul`). Operands and result are two's-complement `u32` words.
pub fn fracmul(a: u32, b: u32) -> u32 {
    let prod = i64::from(a as i32) * i64::from(b as i32);
    ((prod * 2 + (1i64 << 31)) >> 32) as u32
}

/// Phase step per frame for a wheel of `freq_hz` at `fs`: `round(f / fs * 2^32)`.
///
/// VERIFIED relation: `f = fs * step / 2^32` (`docs/research/organ-tonewheel-bank.md` 2.1).
pub fn phase_step(freq_hz: f64, fs: f64) -> u32 {
    (freq_hz / fs * 4_294_967_296.0).round() as u64 as u32
}

/// Default value of the global rate factor `DM(0xC27C5)`: 0.5 as a Q31 word.
///
/// VERIFIED: image initial data (oracle boot, `sharc-bank --dump 0xC27C5:1`); the ARM rewrites it
/// from the Fine Tune table (`organ-tonewheel-bank.md` section 1).
pub const DEFAULT_TUNE_WORD: u32 = 0x4000_0000;

/// Cycles per bank frame of an element with ARM increment word `inc` (`value << 8`) at rate
/// factor `tune`: `fracmul(inc, tune) / 2^32`, i.e. `inc / 2^33` at the default tune.
///
/// VERIFIED: `phase += fracmul(inc, tune)` per frame (oracle, 29,440 words); with a frame rate
/// `fs` this is `f = fs * v / 2^25` for `inc = v << 8`. The frame rate itself is not known from
/// the image, see [`implied_frame_rate`].
pub fn cycles_per_frame(inc: u32, tune: u32) -> f64 {
    f64::from(fracmul(inc, tune) as i32) / 4_294_967_296.0
}

/// Frame rate at which element 0 of the exact ARM table (`0x00635300`) sounds at `wheel0_hz`.
///
/// Open: the absolute pitch needs the real frame rate and tune index. At 48 kHz and the default
/// tune, wheel 0 is 36.37 Hz (D1 minus 16 cent); with the engine's ASSUMED wheel 0 = C1
/// (32.703 Hz) the implied frame rate is 43,156.4 Hz.
pub fn implied_frame_rate(wheel0_hz: f64) -> f64 {
    wheel0_hz / cycles_per_frame(0x0063_5300, DEFAULT_TUNE_WORD)
}

/// 92 wheels x 2 copies of free-running u32 phase accumulators sharing one x table.
///
/// The firmware mixes 92 weights (`BANK_WHEELS`); the engine outputs the first 91 to its drawbar
/// buses ([`ToneBank::advance`]) and all 92 through [`ToneBank::advance_lanes`].
#[derive(Debug, Clone)]
pub struct ToneBank {
    table: Box<[f32]>,
    steps: [[u32; BANK_WHEELS]; 2],
    phases: [[u32; BANK_WHEELS]; 2],
    copies: bool,
    weights: [f32; 2],
    fs: f32,
}

impl ToneBank {
    /// Allocates the table and zeroed accumulators (the only allocation of the bank).
    pub fn new() -> Self {
        Self {
            table: build_x_table(),
            steps: [[0; BANK_WHEELS]; 2],
            phases: [[0; BANK_WHEELS]; 2],
            copies: false,
            weights: copy_weights(false),
            fs: 0.0,
        }
    }

    /// Sample rate the steps were computed for (`0.0` before the first `retune`).
    pub fn sample_rate(&self) -> f32 {
        self.fs
    }

    /// Whether the second detuned copy runs.
    pub fn copies(&self) -> bool {
        self.copies
    }

    /// Recomputes every step from the wheel frequencies. With `copies` (vibrato enable on) each
    /// element runs at its table detune (`vibrato_bank::DETUNE_CENTS`, element `2 w + c`) and the
    /// two copies are mixed with `copy_weights(true)`; otherwise only copy 0 runs at the nominal
    /// frequency (the firmware exact table has identical copies). Phases are kept. The 92nd wheel
    /// continues the 12-TET ladder one semitone above wheel 90.
    pub fn retune(&mut self, freqs_hz: &[f64; NUM_TONEWHEELS], fs: f32, copies: bool) {
        self.fs = fs;
        self.copies = copies;
        self.weights = copy_weights(copies);
        let top = freqs_hz[NUM_TONEWHEELS - 1] * 2f64.powf(1.0 / 12.0);
        let [steps_a, steps_b] = &mut self.steps;
        for (w, (sa, sb)) in steps_a.iter_mut().zip(steps_b.iter_mut()).enumerate() {
            let f = freqs_hz.get(w).copied().unwrap_or(top);
            *sa = phase_step(f * copy_ratio(w, 0, copies), f64::from(fs));
            *sb = if copies {
                phase_step(f * copy_ratio(w, 1, copies), f64::from(fs))
            } else {
                0
            };
        }
    }

    /// Sets one raw step (used by tests and by increment-table experiments).
    pub fn set_step(&mut self, copy: usize, wheel: usize, step: u32) {
        self.steps[copy][wheel] = step;
    }

    /// Sets the step of an element from an ARM increment word and a rate factor, exactly as the
    /// firmware does (`fracmul(inc, tune)`, VERIFIED).
    pub fn set_increment(&mut self, copy: usize, wheel: usize, inc: u32, tune: u32) {
        self.steps[copy][wheel] = fracmul(inc, tune);
    }

    /// Advances every accumulator one frame; `a` gets copy 0, `b` copy 1 (zero when the second
    /// copy is not running). The first output is already advanced by one step (VERIFIED:
    /// accumulate before the read).
    fn step_all(&mut self, a: &mut [f32; BANK_WHEELS], b: &mut [f32; BANK_WHEELS]) {
        let table = &self.table;
        let read = |p: u32| table[(p >> INDEX_SHIFT) as usize];
        for (w, (oa, ob)) in a.iter_mut().zip(b.iter_mut()).enumerate() {
            self.phases[0][w] = self.phases[0][w].wrapping_add(self.steps[0][w]);
            *oa = read(self.phases[0][w]);
            *ob = if self.copies {
                self.phases[1][w] = self.phases[1][w].wrapping_add(self.steps[1][w]);
                read(self.phases[1][w])
            } else {
                0.0
            };
        }
    }

    /// Advances all accumulators by one frame and writes the first 91 wheel outputs (copies
    /// mixed with `copy_weights`: exact table = copy 0 alone, detuned = the mono mean of the
    /// firmware lane matrix).
    pub fn advance(&mut self, out: &mut [f32; NUM_TONEWHEELS]) {
        let (mut a, mut b) = ([0.0; BANK_WHEELS], [0.0; BANK_WHEELS]);
        self.step_all(&mut a, &mut b);
        let [wa, wb] = self.weights;
        for ((o, &x), &y) in out.iter_mut().zip(a.iter()).zip(b.iter()) {
            *o = if self.copies { wa * x + wb * y } else { x };
        }
    }

    /// Advances one frame and writes the raw outputs of copy 0 (`a`) and copy 1 (`b`, zero while
    /// the second copy is not running) of the first 91 wheels, for the smoothed lane mixer
    /// (`lane_mix::LaneMixer`) which applies the group gains itself.
    pub fn advance_copies(&mut self, a: &mut [f32; NUM_TONEWHEELS], b: &mut [f32; NUM_TONEWHEELS]) {
        let (mut fa, mut fb) = ([0.0; BANK_WHEELS], [0.0; BANK_WHEELS]);
        self.step_all(&mut fa, &mut fb);
        a.copy_from_slice(&fa[..NUM_TONEWHEELS]);
        b.copy_from_slice(&fb[..NUM_TONEWHEELS]);
    }

    /// Advances one frame and writes the two output lanes of the firmware mix stage per wheel:
    /// `x_lane = g5 a + g8 b`, `y_lane = g6 b + g7 a` with the lane matrix of the current vibrato
    /// table (`vibrato_bank::lane_matrix`). Summing `weight[w] * lane[w]` over the 92 wheels and
    /// applying the smoothed level gives the oracle output (`tools/organ_oracle_audio.py verify`).
    pub fn advance_lanes(
        &mut self,
        x_lane: &mut [f32; BANK_WHEELS],
        y_lane: &mut [f32; BANK_WHEELS],
    ) {
        let (mut a, mut b) = ([0.0; BANK_WHEELS], [0.0; BANK_WHEELS]);
        self.step_all(&mut a, &mut b);
        let m = lane_matrix(if self.copies {
            GAINS_DETUNED
        } else {
            GAINS_EXACT
        });
        for (((xl, yl), &p), &q) in x_lane
            .iter_mut()
            .zip(y_lane.iter_mut())
            .zip(a.iter())
            .zip(b.iter())
        {
            *xl = m[0][0] * p + m[0][1] * q;
            *yl = m[1][0] * p + m[1][1] * q;
        }
    }
}

impl Default for ToneBank {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn harmonic_db(h: usize) -> f64 {
        let amp = |k: usize| {
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for (i, &v) in build_x_table().iter().enumerate() {
                let a = TAU * (k * i) as f64 / TABLE_LEN as f64;
                re += f64::from(v) * a.cos();
                im += f64::from(v) * a.sin();
            }
            (re * re + im * im).sqrt()
        };
        20.0 * (amp(h) / amp(1)).log10()
    }

    #[test]
    fn table_harmonics_match_firmware_levels() {
        let expect = [(2usize, -19.0), (3, -36.0), (4, -44.0), (5, -48.0)];
        for (h, db) in expect {
            assert!(
                (harmonic_db(h) - db).abs() < 0.1,
                "h{h}: {}",
                harmonic_db(h)
            );
        }
        assert!(harmonic_db(6) < -100.0);
    }

    #[test]
    fn table_is_closed_form() {
        let t = build_x_table();
        assert_eq!(t.len(), TABLE_LEN);
        for i in [0usize, 1, 511, 1024, 2047, 4095] {
            let want = firmware_wave_x(TAU * i as f64 / TABLE_LEN as f64) as f32;
            assert_eq!(t[i], want);
        }
        let x0 = -(0.112_202 + 0.015_848_9 + 0.006_309_57 + 0.003_981_07);
        assert!((f64::from(t[0]) - x0).abs() < 1e-6);
    }

    #[test]
    fn fracmul_rounds_to_nearest_and_is_signed() {
        assert_eq!(fracmul(1 << 20, 0x4000_0000), 1 << 19);
        assert_eq!(fracmul(3, 0x4000_0000), 2); // 1.5 rounds up
        assert_eq!(fracmul(5, 0x4000_0000), 3); // 2.5 rounds up
        assert_eq!(fracmul((-3i32) as u32, 0x4000_0000), (-1i32) as u32); // -1.5 -> -1
        assert_eq!(fracmul((-4i32) as u32, 0x4000_0000), (-2i32) as u32);
    }

    #[test]
    fn phase_step_gives_frequency() {
        let s = phase_step(440.0, 48_000.0);
        let f = f64::from(s) / 4_294_967_296.0 * 48_000.0;
        assert!((f - 440.0).abs() < 1e-5);
    }

    #[test]
    fn bank_matches_closed_form_at_chosen_steps() {
        let mut bank = ToneBank::new();
        bank.set_step(0, 0, 0x0100_0000); // 1/256 cycle per frame
        bank.set_step(0, 1, 0x0123_4567);
        bank.set_step(0, 2, 0xF000_0001); // wraps often
        let mut out = [0.0f32; NUM_TONEWHEELS];
        let mut ph = [0u32; 3];
        let st = [0x0100_0000u32, 0x0123_4567, 0xF000_0001];
        for _ in 0..600 {
            bank.advance(&mut out);
            for (w, p) in ph.iter_mut().enumerate() {
                *p = p.wrapping_add(st[w]);
                let idx = (*p >> 20) as usize;
                let want = firmware_wave_x(TAU * idx as f64 / 4096.0) as f32;
                assert_eq!(out[w], want);
            }
        }
        assert_eq!(out[3], build_x_table()[0]);
    }

    #[test]
    fn detuned_copies_follow_the_cent_table() {
        use super::super::vibrato_bank::{wheel_hz, DETUNE_CENTS};
        let mut freqs = [0.0f64; NUM_TONEWHEELS];
        for (w, f) in freqs.iter_mut().enumerate() {
            *f = wheel_hz(w, 0.0);
        }
        let mut single = ToneBank::new();
        single.retune(&freqs, 48_000.0, false);
        let mut dual = ToneBank::new();
        dual.retune(&freqs, 48_000.0, true);
        assert_eq!(single.steps[1][0], 0);
        assert!(!single.copies() && dual.copies());
        for w in [0usize, 1, 45, 90] {
            for c in 0..2 {
                let ratio = f64::from(dual.steps[c][w]) / f64::from(single.steps[0][w]);
                let cents = 1200.0 * ratio.log2();
                let want = f64::from(DETUNE_CENTS[2 * w + c]);
                assert!((cents - want).abs() < 1e-3, "w{w} c{c}: {cents} vs {want}");
            }
        }
        let (mut a, mut b) = ([0.0; NUM_TONEWHEELS], [0.0; NUM_TONEWHEELS]);
        let mut diff = 0.0f32;
        for _ in 0..4000 {
            single.advance(&mut a);
            dual.advance(&mut b);
            diff += (a[0] - b[0]).abs();
            assert!(b[0].is_finite() && b[0].abs() <= 1.4);
        }
        assert!(diff > 0.0);
    }

    #[test]
    fn exact_bank_matches_single_copy_output() {
        let mut freqs = [0.0f64; NUM_TONEWHEELS];
        freqs[3] = 523.25;
        let mut bank = ToneBank::new();
        bank.retune(&freqs, 48_000.0, false);
        let mut out = [0.0f32; NUM_TONEWHEELS];
        let mut ph = 0u32;
        let step = phase_step(523.25, 48_000.0);
        let table = build_x_table();
        for _ in 0..500 {
            bank.advance(&mut out);
            ph = ph.wrapping_add(step);
            assert_eq!(out[3], table[(ph >> INDEX_SHIFT) as usize]);
        }
    }

    #[test]
    fn increment_words_give_the_firmware_cycles_per_frame() {
        // exact table element 0: 0x00635300 * 0.5 / 2^32
        let c = cycles_per_frame(0x0063_5300, DEFAULT_TUNE_WORD);
        assert!((c - 7.577_84e-4).abs() < 1e-8, "{c}");
        // f = fs * v / 2^25 with v = inc >> 8
        let f = 48_000.0 * f64::from(0x6353u32) / 33_554_432.0;
        assert!((c * 48_000.0 - f).abs() < 1e-9);
        // the engine's wheel 0 (C1) implies a 43.156 kHz frame rate
        assert!((implied_frame_rate(32.703_195_66) - 43_156.4).abs() < 1.0);
    }

    #[test]
    fn set_increment_uses_fracmul_and_the_92nd_wheel_runs() {
        let mut bank = ToneBank::new();
        bank.set_increment(0, 91, 0x0063_5300, DEFAULT_TUNE_WORD);
        assert_eq!(bank.steps[0][91], fracmul(0x0063_5300, DEFAULT_TUNE_WORD));
        let (mut x, mut y) = ([0.0f32; BANK_WHEELS], [0.0f32; BANK_WHEELS]);
        let t = build_x_table();
        bank.advance_lanes(&mut x, &mut y);
        let idx = (fracmul(0x0063_5300, DEFAULT_TUNE_WORD) >> INDEX_SHIFT) as usize;
        assert!((x[91] - t[idx]).abs() < 1e-6 * 2.0);
        // retune extends the ladder by one semitone for the 92nd wheel
        let mut freqs = [0.0f64; NUM_TONEWHEELS];
        freqs[NUM_TONEWHEELS - 1] = 1000.0;
        bank.retune(&freqs, 48_000.0, false);
        let want = phase_step(1000.0 * 2f64.powf(1.0 / 12.0), 48_000.0);
        assert_eq!(bank.steps[0][BANK_WHEELS - 1], want);
    }
}
