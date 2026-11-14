//! Firmware ANALOG SUPER waves 2..4 (Core2 stage `0x124985`) as an opt-in model.
//!
//! Spec: `docs/specs/synth-engine.md` section 4.3; evidence `docs/research/synth-stage-models-12.md`;
//! bit-exact Python model `tools/synth_stage_super2.py` (320 of 320 oracle words), golden vectors
//! `tools/synth_stage_golden10.py` -> `tests_fw_stage_golden10.rs`.
//!
//! Four detuned voices read a band-limited single-cycle table (two int16 samples per word), a
//! 512-phase 8-tap polyphase FIR interpolates, the voices are mixed, held for two samples
//! (`a a b b`) and smoothed by three identical biquads. The table words (octave band rows, wave,
//! FIR phases, biquad coefficients) are firmware-derived numbers parsed once from
//! `docs/research/data/synth-super24-tables.txt` (fingerprints in the golden file).
//!
//! One lane of the stage; the cross-lane weight mix is folded to equal voice weights (the P
//! block is open, OQ22).

use std::sync::OnceLock;

use super::fw_stages::{fade, SUPER_SPREAD_MAX};

const TABLE_TEXT: &str = include_str!("../../../../docs/research/data/synth-super24-tables.txt");

/// VERIFIED (listing `0x249326..0x249348`, image words): spread law `v = 0.75 (t + 2 t^4)`,
/// `t = s5 + 0.06`.
const SPREAD_OFFSET: f32 = 0.06;
const SPREAD_SCALE: f32 = 0.75;
/// VERIFIED (image word `0x3F7FF972`): the doubled pitch is clipped here before the band index.
const BAND_CLIP: f32 = 0.999_889_85;
/// ASSUMED: the four voice ratios (P block, unknown): a narrow symmetric spread around the pitch.
pub const SUPER24_RATIOS: [f32; 4] = [0.97, 0.99, 1.01, 1.03];
/// VERIFIED (listing `0x249456`, `LCNTR = 4`): voices of the loop.
pub const SUPER24_VOICES: usize = 4;
/// VERIFIED (`R11 = 0x257`, `FEXT` 9 bits from bit 23): phases of the polyphase FIR.
const FIR_PHASES: u32 = 512;
/// Sample offsets of the four taps of one multiplier register (lane x; lane y adds one).
const TAP_OFFSETS: [usize; 4] = [0, 1, 4, 5];

/// Firmware-derived tables of the stage.
#[derive(Debug)]
pub struct Super24Tables {
    /// PM `0x9631F`: `[offset, count, spare, spare]` per octave band.
    pub rows: Vec<u32>,
    /// DM `0x08030000`: the table words (two int16 samples each, low half first).
    pub wave: Vec<u32>,
    /// PM `0x9531F`: `FIR_PHASES x 8` coefficients (24-bit values in the top bits).
    pub fir: Vec<u32>,
    /// PM `0x95000`: biquad `b0 b1 b2 a1 a2` as float words.
    pub lattice: [f32; 5],
}

fn parse(text: &str) -> Super24Tables {
    let (mut rows, mut wave, mut fir, mut lat) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut cur: Option<&mut Vec<u32>> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("# ") {
            cur = match rest.split_whitespace().next() {
                Some("rows") => Some(&mut rows),
                Some("wave") => Some(&mut wave),
                Some("fir") => Some(&mut fir),
                _ => Some(&mut lat),
            };
        } else if let Some(v) = cur.as_deref_mut() {
            v.extend(
                line.split_whitespace()
                    .filter_map(|t| u32::from_str_radix(t, 16).ok()),
            );
        }
    }
    let mut lattice = [0.0f32; 5];
    for (d, w) in lattice.iter_mut().zip(&lat) {
        *d = f32::from_bits(*w);
    }
    Super24Tables {
        rows,
        wave,
        fir,
        lattice,
    }
}

/// The tables, parsed on first use. Call [`prepare`] outside the audio thread (the stage
/// selection does).
pub fn tables() -> &'static Super24Tables {
    static T: OnceLock<Super24Tables> = OnceLock::new();
    T.get_or_init(|| parse(TABLE_TEXT))
}

/// Forces the table parse (allocation) before the first rendered sample.
pub fn prepare() {
    let _ = tables();
}

/// State of one SUPER 2..4 voice oscillator (no heap).
#[derive(Debug, Clone, Copy, Default)]
pub struct Super24State {
    /// Voice phases (2^32 = one cycle).
    pub phase: [u32; SUPER24_VOICES],
    /// The mixed value held for two output samples.
    pub held: f32,
    /// Second sample of the hold.
    pub second: bool,
    /// Biquad states `s1, s2` of the three sections.
    pub bq: [f32; 6],
}

/// Octave band `idx` of the pitch word `f` (listing `0x2493BA..0x249404`): doubled pitch,
/// `FIX` to 31 bits, minus the exponent (at most -15) times eight and the top three mantissa bits.
pub fn band_index(f: f32) -> usize {
    let u = (2.0 * f.min(0.5)).min(BAND_CLIP);
    let a = ((u as f64) * 2_147_483_648.0).round_ties_even() as i64;
    let a = a.unsigned_abs() as u32;
    if a == 0 {
        return 0;
    }
    let e = -(31 - (32 - a.leading_zeros() as i32)); // EXP: minus the redundant sign bits
    let norm = a << (-e) as u32;
    let top3 = (norm.wrapping_sub(0x4000_0000) >> 27) as i32;
    (-(8 * e.max(-15) + top3)).max(0) as usize
}

/// One lane sum `MRF_x + MRF_y` of the FIR (listing `0x249528..0x249570`): four taps at the
/// sample offsets `0 1 4 5` (lane y one sample later) with the even / odd coefficients of the
/// phase starting at `c0`, `2 d c >> 32` each. `signed` sign-extends the int16 samples.
pub fn fir_lane(words: &[u32; 6], rel: usize, c0: usize, fir: &[u32], signed: bool) -> i32 {
    let sample = |at: usize| -> i64 {
        let Some(w) = words.get(at / 2) else { return 0 };
        let h = (w >> (16 * (at % 2))) as u16;
        if signed {
            h as i16 as i64
        } else {
            h as i64
        }
    };
    let mut total = 0i32;
    for lane in 0..2 {
        let mut acc = 0i64;
        for (k, off) in TAP_OFFSETS.iter().enumerate() {
            let c = fir[c0 + 2 * k + lane] as i32 as i64;
            acc += 2 * sample(rel + off + lane) * c;
        }
        total = total.wrapping_add((acc >> 32).clamp(i32::MIN as i64, i32::MAX as i64) as i32);
    }
    total
}

/// One interpolated table value of one voice at `phase`: the band row `(offset, count)` picks
/// the cycle, `phase * count` the position, its fraction the FIR phase.
fn voice_value(t: &Super24Tables, phase: u32, offset: usize, count: usize) -> f32 {
    let prod = phase as u64 * count as u64;
    let idx = (prod >> 32) as usize + offset;
    let fir_phase = ((prod as u32) >> 23) as usize % FIR_PHASES as usize;
    let first = idx / 2;
    let mut words = [0u32; 6];
    for (n, w) in words.iter_mut().enumerate() {
        *w = t.wave.get(first + n).copied().unwrap_or(0);
    }
    // ASSUMED: the hardware sign-extends the int16 samples; the oracle (`sharc-org`) loads the
    // short-word window zero-extended, which would add a step of 4.0 at every zero crossing.
    let sum = fir_lane(&words, idx % 2, 8 * fir_phase, &t.fir, true);
    sum as f32 * (1.0 / 16_384.0)
}

/// One output sample for the render increment `dt` (cycles per sample), SY increment `i_sy`
/// and shape / Osc Ctrl value `ctrl`.
pub fn super24_sample(st: &mut Super24State, dt: f32, i_sy: f32, ctrl: f32) -> f32 {
    let t = tables();
    if !st.second {
        let f = 2.0 * i_sy;
        let idx = band_index(f.min(0.5)).min(t.rows.len() / 4 - 1);
        let (offset, count) = (t.rows[4 * idx] as usize, t.rows[4 * idx + 1] as usize);
        let tt = SUPER_SPREAD_MAX * ctrl + SPREAD_OFFSET;
        let q = tt * tt;
        let v = SPREAD_SCALE * (tt + 2.0 * q * q);
        let mut sum = 0.0;
        for (ph, rho) in st.phase.iter_mut().zip(SUPER24_RATIOS) {
            // two output samples per held value: the phase advances by twice the sample step
            let step = 2.0 * dt * (rho * v + 1.0 - v);
            *ph = ph.wrapping_add((step.fract() as f64 * 4_294_967_296.0) as u64 as u32);
            sum += voice_value(t, *ph, offset, count);
        }
        // ASSUMED: equal voice weights summing to one, P[16] = 1 (the P block is open, OQ22)
        st.held = sum * 0.25 * fade(2.0 * i_sy);
    }
    st.second = !st.second;
    let c = &t.lattice;
    let mut x = st.held;
    for n in 0..3 {
        let (s1, s2) = (st.bq[2 * n], st.bq[2 * n + 1]);
        let y = c[0] * x + s1;
        st.bq[2 * n] = c[1] * x + s2 - c[3] * y;
        st.bq[2 * n + 1] = c[2] * x - c[4] * y;
        x = y;
    }
    x
}

#[cfg(test)]
mod tests {
    use super::super::tests_fw_stage_golden10 as gold;
    use super::*;

    fn fnv(words: &[u32]) -> u64 {
        words.iter().fold(0xCBF2_9CE4_8422_2325u64, |h, w| {
            w.to_le_bytes().iter().fold(h, |h, b| {
                (h ^ *b as u64).wrapping_mul(0x0000_0100_0000_01B3)
            })
        })
    }

    #[test]
    fn tables_match_the_python_fingerprints() {
        let t = tables();
        assert_eq!((t.rows.len(), t.wave.len(), t.fir.len()), (3072, 985, 4096));
        assert_eq!(fnv(&t.rows), gold::FNV_ROWS);
        assert_eq!(fnv(&t.wave), gold::FNV_WAVE);
        assert_eq!(fnv(&t.fir), gold::FNV_FIR);
        let lat: Vec<u32> = t.lattice.iter().map(|f| f.to_bits()).collect();
        assert_eq!(fnv(&lat), gold::FNV_LATTICE);
    }

    #[test]
    fn band_index_matches_the_python_model() {
        for (bits, want) in gold::PITCH.iter().zip(gold::BAND) {
            assert_eq!(
                band_index(f32::from_bits(*bits)),
                want as usize,
                "{bits:#x}"
            );
        }
    }

    #[test]
    fn fir_matches_the_python_model_zero_and_sign_extended() {
        let t = tables();
        for n in 0..48 {
            let mut words = [0u32; 6];
            words.copy_from_slice(&gold::FIR_WORDS[6 * n..6 * n + 6]);
            let (rel, c0) = (gold::FIR_REL[n] as usize, 8 * gold::FIR_PHASE[n] as usize);
            assert_eq!(fir_lane(&words, rel, c0, &t.fir, false), gold::FIR_ZERO[n]);
            assert_eq!(fir_lane(&words, rel, c0, &t.fir, true), gold::FIR_SIGNED[n]);
        }
    }

    #[test]
    fn the_stage_is_finite_bounded_deterministic_and_pitched() {
        let (mut a, mut b) = (Super24State::default(), Super24State::default());
        let dt = 220.0 / 48_000.0;
        let mut out = Vec::new();
        for _ in 0..4000 {
            let x = super24_sample(&mut a, dt, 220.0 / 43_402.34, 0.5);
            assert!(x.is_finite() && x.abs() < 8.0, "{x}");
            assert_eq!(
                x.to_bits(),
                super24_sample(&mut b, dt, 220.0 / 43_402.34, 0.5).to_bits()
            );
            out.push(x);
        }
        let rms = (out.iter().map(|x| x * x).sum::<f32>() / out.len() as f32).sqrt();
        assert!(rms > 1e-3, "silent: {rms}");
        // the table of the lowest band holds a fundamental plus its octave: one to two rising
        // zero crossings per cycle
        let crossings = out[1000..]
            .windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count() as f32;
        let cycles = 3000.0 * 220.0 / 48_000.0;
        assert!(
            crossings >= 0.8 * cycles && crossings <= 2.3 * cycles,
            "{crossings}"
        );
    }
}
