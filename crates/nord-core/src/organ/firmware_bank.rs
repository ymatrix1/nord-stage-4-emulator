//! Tonewheel oscillator bank with the arithmetic of the PNO Core1 routine at pc `0x122620`.
//!
//! Per frame and element: `phase += step` (u32, wraps at 2^32), `idx = phase >> 20`,
//! `out = T[idx]` from a 4096-point table, no interpolation. Research and oracle run:
//! `docs/research/organ-tonewheel-bank.md` (sections 1, 2.1, 4); Python model
//! `tools/organ_bank_ref.py`.
//!
//! Real-time: `advance` only touches pre-allocated arrays. The table is built once in `new`.

use super::types::NUM_TONEWHEELS;
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

/// Spread between the two detuned copies of a wheel, in cents (rms of `1200 log2(v[2k+1]/v[2k])`).
///
/// EMPIRICAL: 1.89 cent rms over 92 pairs of the ARM default increment table
/// (`python tools/organ_bank_ref.py defaults`, `bank0_CODE@0x443EA8`), mean -0.16 cent, random
/// sign per wheel. The Rust model applies it as a constant `-spread/2` and `+spread/2` pair
/// (ASSUMED simplification); the role of the copies (stereo decorrelation or unison strings) is
/// ASSUMED.
pub const COPY_SPREAD_CENTS: f64 = 1.89;

/// Harmonic coefficients of the firmware x waveform: `sin t` plus `c[k] cos((k+2) t)`.
///
/// VERIFIED: harmonics 0, -19.0, -36.0, -44.0, -48.0 dB measured through the oracle on a running
/// bank (`python tools/organ_bank_ref.py spectrum`), i.e. `0.1122`, `0.01585`, `0.00631`,
/// `0.00398`; the signs are the fit of `docs/research/organ-tonewheel-bank.md` section 1. The
/// phase origin of the table (index 0 = `t = 0`) is ASSUMED. Checked by
/// `tests::table_harmonics_match_firmware_levels`.
const X_COS_COEFFS: [f64; 4] = [-0.1122, -0.01585, -0.00631, -0.00398];

/// Closed form of the firmware x waveform at phase `t` (radians).
pub fn firmware_wave_x(t: f64) -> f64 {
    let mut v = t.sin();
    for (k, c) in X_COS_COEFFS.iter().enumerate() {
        v += c * ((k as f64 + 2.0) * t).cos();
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

/// 91 wheels x 2 copies of free-running u32 phase accumulators sharing one x table.
#[derive(Debug, Clone)]
pub struct ToneBank {
    table: Box<[f32]>,
    steps: [[u32; NUM_TONEWHEELS]; 2],
    phases: [[u32; NUM_TONEWHEELS]; 2],
    copies: bool,
    fs: f32,
}

impl ToneBank {
    /// Allocates the table and zeroed accumulators (the only allocation of the bank).
    pub fn new() -> Self {
        Self {
            table: build_x_table(),
            steps: [[0; NUM_TONEWHEELS]; 2],
            phases: [[0; NUM_TONEWHEELS]; 2],
            copies: false,
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

    /// Recomputes every step from the wheel frequencies. With `copies`, copy 0 sits
    /// `COPY_SPREAD_CENTS / 2` below and copy 1 above each frequency; otherwise only copy 0 runs,
    /// at the nominal frequency. Phases are kept.
    pub fn retune(&mut self, freqs_hz: &[f64; NUM_TONEWHEELS], fs: f32, copies: bool) {
        self.fs = fs;
        self.copies = copies;
        let half = COPY_SPREAD_CENTS / 2.0;
        let lo = (-half / 1200.0).exp2();
        let hi = (half / 1200.0).exp2();
        for (w, &f) in freqs_hz.iter().enumerate() {
            if copies {
                self.steps[0][w] = phase_step(f * lo, f64::from(fs));
                self.steps[1][w] = phase_step(f * hi, f64::from(fs));
            } else {
                self.steps[0][w] = phase_step(f, f64::from(fs));
                self.steps[1][w] = 0;
            }
        }
    }

    /// Sets one raw step (used by tests and by increment-table experiments).
    pub fn set_step(&mut self, copy: usize, wheel: usize, step: u32) {
        self.steps[copy][wheel] = step;
    }

    /// Advances all accumulators by one frame and writes the wheel outputs (copies averaged).
    ///
    /// The first output is already advanced by one step (VERIFIED: accumulate before the read).
    pub fn advance(&mut self, out: &mut [f32; NUM_TONEWHEELS]) {
        let table = &self.table;
        let read = |p: u32| table[(p >> INDEX_SHIFT) as usize];
        let (pa, pb) = self.phases.split_at_mut(1);
        let (sa, sb) = (&self.steps[0], &self.steps[1]);
        if self.copies {
            for (((o, a), b), (&step_a, &step_b)) in out
                .iter_mut()
                .zip(pa[0].iter_mut())
                .zip(pb[0].iter_mut())
                .zip(sa.iter().zip(sb.iter()))
            {
                *a = a.wrapping_add(step_a);
                *b = b.wrapping_add(step_b);
                *o = 0.5 * (read(*a) + read(*b));
            }
        } else {
            for ((o, a), &step_a) in out.iter_mut().zip(pa[0].iter_mut()).zip(sa.iter()) {
                *a = a.wrapping_add(step_a);
                *o = read(*a);
            }
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
        let x0 = -(0.1122 + 0.01585 + 0.00631 + 0.00398);
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
    fn detuned_copies_average_two_tables_reads() {
        let mut freqs = [0.0f64; NUM_TONEWHEELS];
        freqs[0] = 440.0;
        let mut single = ToneBank::new();
        single.retune(&freqs, 48_000.0, false);
        let mut dual = ToneBank::new();
        dual.retune(&freqs, 48_000.0, true);
        assert_eq!(single.steps[1][0], 0);
        let ratio = f64::from(dual.steps[1][0]) / f64::from(dual.steps[0][0]);
        let cents = 1200.0 * ratio.log2();
        assert!((cents - COPY_SPREAD_CENTS).abs() < 1e-3, "{cents}");
        let (mut a, mut b) = ([0.0; NUM_TONEWHEELS], [0.0; NUM_TONEWHEELS]);
        let mut diff = 0.0f32;
        for _ in 0..4000 {
            single.advance(&mut a);
            dual.advance(&mut b);
            diff += (a[0] - b[0]).abs();
            assert!(b[0].is_finite() && b[0].abs() <= 1.2);
        }
        assert!(diff > 0.0);
    }
}
