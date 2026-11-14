//! Farfisa tone source: the top-octave divider of the PNO core 1 generator `0x122A71`.
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1.8. Research: `docs/research/organ-farfisa-bass.md`
//! (listing, bit-exact Python model `tools/organ_farfisa_ref.py` checked against the SHARC
//! oracle, closed-form tables fitted in `tools/organ_farfisa_oracle.py tables`).
//!
//! 12 phase accumulators (one per semitone of the octave C0..B0) each feed a ladder of 8 rows
//! (9 for the two lowest). Row `r` reads the doubled phase `phase_(r+1) = 2 phase_r + S`, shapes it
//! into a clipped soft pulse and keeps a one-pole "release" from the previous sample:
//!
//! ```text
//! u = (|phase_r| + OFF) slope_r        y = max(b_r clip(u) + a_r y_prev, clip(u))
//! ```
//!
//! The wheels are free running: the keys act in the processor `0x122B85` (weights over the ring),
//! not here, so this source plugs into the existing drawbar bus like the other wheel sources.
//! Wheel `w` of the 91-wheel bus is MIDI note `24 + w`; the generator's row 0 (notes 12..23) lies
//! below the bus and is dropped, notes 110..114 do not exist in the divider chain (silent).

use super::types::NUM_TONEWHEELS;

/// Oscillators (semitones) of the top octave.
pub const OSCILLATORS: usize = 12;
/// Rows of the longest ladder (oscillators 0 and 1; the others have 8).
pub const ROWS: usize = 9;
/// MIDI note of oscillator 0, row 0.
pub const BASE_NOTE: usize = 12;
/// MIDI note of wheel 0 of the bus.
pub const WHEEL0_NOTE: usize = 24;

/// Phase increment word of oscillator 0 (the stream `S` of the generator, `0xC2AD8`).
///
/// VERIFIED: image word; `round(INC0 2^(n/12))` equals the 12 stream words to one unit (4 of 12
/// differ by +-1, `tools/organ_farfisa_oracle.py tables`), 12-TET to 0.0005 cent.
pub const INC0: f64 = 3_236_192.0;
/// Phase offset added after every doubling, as an unsigned word (`-0.6667` of a half cycle).
///
/// VERIFIED: the single value of the 8 or 9 offset words of every pair in the stream.
pub const PHASE_OFFSET: u32 = 0xAAAA_AC11;
/// Edge offset `OFF` of every row (float word `0xBF2AAAA7`).
///
/// VERIFIED: constant in all 108 table rows.
pub const EDGE_OFFSET: f32 = -0.666_666_45;
/// Edge slope of oscillator 0, row 0 (float word `0xC4A5E557`); rows halve it, semitones scale it
/// by `2^(-n/12)`.
///
/// VERIFIED: 108 rows, max relative error of the closed form 4.0e-8.
pub const SLOPE0: f64 = -1_327.166_9;
/// Release constant: `a_r = exp(-DECAY_C0 2^r 2^(n/12))` for rows 0..=6.
///
/// EMPIRICAL: least squares over 84 rows, spread 1e-5 relative, max `a` error 1.7e-6.
pub const DECAY_C0: f64 = 0.004_734_3;
/// Row 7 uses the exponent of row 11 (`a_7 = a_6^32`), row 8 has `a = 0`.
///
/// EMPIRICAL: rows 7 of all 12 oscillators agree to 1e-5 relative in the exponent.
const LAST_ROW_EXPONENT: i32 = 11;
/// Frame rate of the DSP that puts oscillator 0 on C0 = 16.3516 Hz (12-TET, A = 440 Hz).
///
/// ASSUMED anchor: 3.76737e-4 cycles per frame (`0x18B0B0` / 2^32) x this rate = C0. The same ratio to the 43 156.4 Hz
/// of the bank derivation (0.9943, -9.9 cent) is the Vox offset of `docs/research/pno-full-pass.md`.
pub const DSP_FRAME_RATE: f64 = 43_402.6;

/// Rows of oscillator `osc`.
pub const fn rows_of(osc: usize) -> usize {
    if osc < 2 {
        ROWS
    } else {
        ROWS - 1
    }
}

/// Phase increment per call of oscillator `osc` (the firmware stream word).
pub fn inc_word(osc: usize) -> u32 {
    (INC0 * (osc as f64 / 12.0).exp2()).round() as u32
}

/// Phase step per DSP frame: `ssf(0.5, inc)` = `inc / 2` (K = 0.5 with the image fine word).
pub fn frame_step(osc: usize) -> u32 {
    inc_word(osc) >> 1
}

/// MIDI note of oscillator `osc`, row `row`.
pub const fn note_of(osc: usize, row: usize) -> usize {
    BASE_NOTE + osc + 12 * row
}

/// Bus wheel of oscillator `osc`, row `row`, or `None` below wheel 0 (row 0) or above the chain.
pub const fn wheel_of(osc: usize, row: usize) -> Option<usize> {
    let note = note_of(osc, row);
    if note < WHEEL0_NOTE || note - WHEEL0_NOTE >= NUM_TONEWHEELS {
        None
    } else {
        Some(note - WHEEL0_NOTE)
    }
}

/// Per-row shaping constants and the one-sample memory.
#[derive(Debug, Clone, Copy, Default)]
struct Row {
    slope: f32,
    /// `a`: weight of the previous output (release).
    keep: f32,
    /// `b = 1 - a`: weight of the clipped pulse.
    mix: f32,
    prev: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct Osc {
    phase: u32,
    step: u32,
    rows: [Row; ROWS],
}

/// The 12 oscillators with their ladders.
#[derive(Debug, Clone)]
pub struct FarfisaWheels {
    osc: [Osc; OSCILLATORS],
}

impl FarfisaWheels {
    /// All phases and outputs zero (the firmware's start state), tuned to the DSP frame rate.
    pub fn new() -> Self {
        let mut w = Self {
            osc: [Osc::default(); OSCILLATORS],
        };
        w.retune(DSP_FRAME_RATE as f32, 0.0);
        w
    }

    /// Retunes for `sample_rate` and `fine_cents`: phase steps keep the pitch, edge widths and
    /// release times keep their length in samples (`rho` = DSP frame rate / sample rate).
    pub fn retune(&mut self, sample_rate: f32, fine_cents: f32) {
        let rho = DSP_FRAME_RATE / f64::from(sample_rate);
        let fine = (f64::from(fine_cents) / 1200.0).exp2();
        for (n, o) in self.osc.iter_mut().enumerate() {
            let semitone = (n as f64 / 12.0).exp2();
            o.step = (f64::from(frame_step(n)) * rho * fine).round() as u32;
            for (r, row) in o.rows.iter_mut().enumerate().take(rows_of(n)) {
                let exponent = if r < 7 { r as i32 } else { LAST_ROW_EXPONENT };
                let a = if r > 7 {
                    0.0
                } else {
                    (-DECAY_C0 * f64::from(exponent).exp2() * semitone * rho).exp()
                };
                row.slope = (SLOPE0 * (-(r as f64)).exp2() / semitone / rho) as f32;
                row.keep = a as f32;
                row.mix = (1.0 - a) as f32;
            }
        }
    }

    /// Advances one sample and writes every (row, oscillator) output into `rows` (`rows[r][n]`).
    pub fn advance_rows(&mut self, rows: &mut [[f32; OSCILLATORS]; ROWS]) {
        const INV_HALF: f32 = 1.0 / 2_147_483_648.0;
        for (n, o) in self.osc.iter_mut().enumerate() {
            o.phase = o.phase.wrapping_add(o.step);
            let mut ph = o.phase;
            for (r, row) in o.rows.iter_mut().enumerate().take(rows_of(n)) {
                let x = (ph as i32 as f32 * INV_HALF).abs();
                let u = (x + EDGE_OFFSET) * row.slope;
                let clipped = u.clamp(-1.0, 1.0);
                let y = (row.mix * clipped + row.keep * row.prev).max(clipped);
                row.prev = y;
                rows[r][n] = y;
                ph = ph.wrapping_shl(1).wrapping_add(PHASE_OFFSET);
            }
        }
    }

    /// Advances one sample and writes the 91 bus wheels (silent where the divider has no note).
    pub fn advance(&mut self, out: &mut [f32; NUM_TONEWHEELS]) {
        let mut rows = [[0.0f32; OSCILLATORS]; ROWS];
        self.advance_rows(&mut rows);
        out.fill(0.0);
        for (r, row) in rows.iter().enumerate() {
            for (n, &y) in row.iter().enumerate() {
                if r < rows_of(n) {
                    if let Some(w) = wheel_of(n, r) {
                        out[w] = y;
                    }
                }
            }
        }
    }
}

impl Default for FarfisaWheels {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_follow_the_oracle_stream_words() {
        // stream words of the pass image (VERIFIED, oracle): oscillators 0, 1, 2, 3, 11
        let oracle = [
            (0usize, 0x31_6160u32),
            (1, 0x34_5112),
            (2, 0x37_6D76),
            (3, 0x3A_B936),
            (11, 0x5D_37BD),
        ];
        for (n, word) in oracle {
            let diff = i64::from(inc_word(n)) - i64::from(word);
            assert!(diff.abs() <= 1, "oscillator {n}: {diff}");
        }
        assert_eq!(frame_step(0), 0x18_B0B0);
    }

    #[test]
    fn row_counts_wheel_map_and_notes() {
        assert_eq!(rows_of(0), 9);
        assert_eq!(rows_of(1), 9);
        assert_eq!(rows_of(2), 8);
        assert_eq!(note_of(0, 0), 12);
        assert_eq!(wheel_of(0, 0), None);
        assert_eq!(wheel_of(0, 1), Some(0));
        assert_eq!(wheel_of(11, 7), Some(83));
        assert_eq!(wheel_of(1, 8), Some(85));
        assert_eq!(
            wheel_of(2, 8),
            Some(86),
            "the map does not know the row counts"
        );
    }

    #[test]
    fn pitch_is_twelve_tet_at_the_dsp_rate() {
        // oscillator 0 row 0 = C0 = 16.3516 Hz: step cycles per frame x frame rate
        let hz = f64::from(frame_step(0)) / 4_294_967_296.0 * DSP_FRAME_RATE;
        assert!((hz / 16.351_598 - 1.0).abs() < 1e-5, "{hz}");
        let a = f64::from(frame_step(9)) / f64::from(frame_step(0));
        assert!((a - 2f64.powf(0.75)).abs() < 1e-6);
    }

    #[test]
    fn golden_rows_match_the_python_model() {
        // tools/organ_farfisa_oracle.py golden: the bit-exact model on the closed-form tables
        use super::super::farfisa_golden as g;
        let mut w = FarfisaWheels::new();
        let mut rows = [[0.0f32; OSCILLATORS]; ROWS];
        let mut worst = 0.0f32;
        for frame in 0..=*g::FRAMES.last().unwrap() {
            w.advance_rows(&mut rows);
            if let Some(k) = g::FRAMES.iter().position(|&f| f == frame) {
                for (r, want_row) in g::ROWS[k].iter().enumerate() {
                    for (&n, &want) in g::OSCILLATORS.iter().zip(want_row.iter()) {
                        if r < rows_of(n) {
                            worst = worst.max((rows[r][n] - want).abs());
                        }
                    }
                }
            }
        }
        assert!(worst < 2e-4, "max error {worst}");
    }

    #[test]
    fn outputs_are_bounded_pulses_with_release() {
        let mut w = FarfisaWheels::new();
        let mut rows = [[0.0f32; OSCILLATORS]; ROWS];
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for _ in 0..20_000 {
            w.advance_rows(&mut rows);
            lo = lo.min(rows[3][0]);
            hi = hi.max(rows[3][0]);
        }
        // oscillator 0 row 3 (note 48): pulse swinging between about -0.97 and 1 (the one-pole release keeps the low level from settling)
        assert!(lo >= -1.0 - 1e-6 && hi <= 1.0 + 1e-6, "{lo} {hi}");
        assert!(lo < -0.9 && hi > 0.99, "{lo} {hi}");
    }

    #[test]
    fn advance_fills_the_bus_and_leaves_the_top_silent() {
        let mut w = FarfisaWheels::new();
        let mut bus = [0.0f32; NUM_TONEWHEELS];
        let mut peak = [0.0f32; NUM_TONEWHEELS];
        for _ in 0..30_000 {
            w.advance(&mut bus);
            for (p, &b) in peak.iter_mut().zip(bus.iter()) {
                *p = p.max(b.abs());
            }
        }
        assert!(peak[..86].iter().all(|&p| p > 0.5));
        assert!(peak[86..].iter().all(|&p| p == 0.0));
    }

    #[test]
    fn sample_rate_keeps_the_pitch() {
        let count = |fs: f32| {
            let mut w = FarfisaWheels::new();
            w.retune(fs, 0.0);
            let mut rows = [[0.0f32; OSCILLATORS]; ROWS];
            let (mut prev, mut n) = (0.0f32, 0u32);
            for _ in 0..(fs as usize * 4) {
                w.advance_rows(&mut rows);
                let y = rows[4][0]; // note 12 + 48 = 60? oscillator 0 row 4 = C4
                if prev < 0.0 && y >= 0.0 {
                    n += 1;
                }
                prev = y;
            }
            f64::from(n) / 4.0
        };
        let (a, b) = (count(43_402.6), count(48_000.0));
        assert!((a - 261.63).abs() < 1.0, "{a}");
        assert!((b - 261.63).abs() < 1.0, "{b}");
    }

    #[test]
    fn fine_tune_shifts_the_steps_by_cents() {
        let (mut a, mut b) = (FarfisaWheels::new(), FarfisaWheels::new());
        a.retune(48_000.0, 0.0);
        b.retune(48_000.0, 100.0);
        let ratio = f64::from(b.osc[5].step) / f64::from(a.osc[5].step);
        assert!((1200.0 * ratio.log2() - 100.0).abs() < 0.01);
    }
}
