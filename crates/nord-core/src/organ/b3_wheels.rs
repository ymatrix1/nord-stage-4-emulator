//! B3 tonewheel source: the pitch law and wheel tables of the PNO core 1 generator `0x121BBE`.
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1.6. Research: `docs/research/b3-generator.md`
//! (oracle fits of 81 ring words, `tools/organ_b3_gen.py`).
//!
//! One wheel per MIDI note: a `u32` phase (1 cycle = 2^32) stepped by the firmware ladder
//! ([`note_step`]) and read through a 128 + 1 point cyclic table with linear interpolation, the
//! table chosen by note ([`wheel_table`]). Wheel `w` of the 91-wheel bus is note `24 + w`.

use super::lane_mix::FIRMWARE_FRAME_RATE_HZ;
use super::types::NUM_TONEWHEELS;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// MIDI note of wheel 0 (the project mapping, `drawbar::foldback_tonewheel`).
pub const WHEEL0_NOTE: usize = 24;

/// One octave of phase steps of the generator.
///
/// VERIFIED: `DM 0x95172..0x9517D` of the PNO core 1 image; the products with
/// [`FINE_WORD`] reproduce 81 oracle-fitted ring frequencies to 2.5e-11 relative
/// (`python tools/organ_b3_gen.py scan`, `docs/research/data/b3-generator-gains.csv`).
pub const PHASE_INC: [u32; 12] = [
    0x31AD21, 0x34A01A, 0x37BD05, 0x3B0C45, 0x3E8DDA, 0x424A27, 0x4638C9, 0x4A6A87, 0x4ED6FD,
    0x53868F, 0x58793D, 0x5DBFCE,
];

/// Fine tune word `DM(0xB33B8)`, a fractional multiplier (`r8 = r0 * r7 (ssfr)`, 63/128).
///
/// VERIFIED: image value (the oracle replays ran with it; [`ARM_FINE_WORD`] replaces it once the
/// ARM writes Fine Tune); `round` instead of `floor` in [`base_step`] is what the oracle frequencies
/// require (5.9e-7 relative difference on pitch classes 1, 3, 5, 6, 9, 11). Read with this word and
/// note 60 = C4 the frame rate would be 43,829 Hz, the old b3-generator figure.
pub const FINE_WORD: u64 = 0x3F00_0000;

/// Fine tune word the ARM writes to `DM(0xB33B8)` at the default Fine Tune: `table[0x1F3] << 8`
/// (the same ARM table and index as the Pipe bank rate factor, handler `0xC011D1F4`).
///
/// VERIFIED: `0x3FA02A00` from `tools/organ_bank_ref.py defaults`. With it note 60 steps
/// 51,784,672 units per call, which at 2 frames per call and `FIRMWARE_FRAME_RATE_HZ` is 261.652 Hz
/// (+0.18 cent from C4; `tests::frame_rate_follows_from_the_arm_fine_tune_word`,
/// `docs/research/dsp-frame-rate.md`).
pub const ARM_FINE_WORD: u64 = 0x3FA0_2A00;

/// DSP frames per call of the generator.
///
/// VERIFIED by agreement: only with 2 frames per call does the 8' of key 60 sound at C4 for the
/// frame rate that the ARM literal, the SY constants, the FX delays and the Pipe bank give (one
/// frame would put it an octave up).
pub const FRAMES_PER_CALL: f64 = 2.0;

/// Wheel-table length (128 points plus the repeated first point).
pub const TABLE_POINTS: usize = 129;

/// Harmonics kept of each firmware wheel table.
const HARMONICS: usize = 12;

/// Amplitudes of harmonics 1..12 of the firmware table `DM 0xB314B` (notes 36..39).
///
/// VERIFIED (derived): FFT of the 128 table points, reconstruction error 4.2e-3 (max, the table
/// has a kink), `python` fit in `docs/research/b3-generator.md` section 1.
const AMP_T1: [f64; HARMONICS] = [
    0.902800, 0.148024, 0.024071, 0.005588, 0.000974, 0.000255, 0.000484, 0.001271, 0.000140,
    0.000125, 0.000001, 0.000046,
];
/// Phases (rad, `cos(2 pi h p + phase)`) of [`AMP_T1`].
const PHASE_T1: [f64; HARMONICS] = [
    0.28822, -0.60934, -1.09631, -1.67530, -0.98026, -0.83519, -0.28432, -2.88054, -0.09022,
    1.06583, 0.02670, 0.44530,
];
/// Amplitudes of the table `DM 0xB31CC` (notes 40..47); reconstruction error 6.3e-7.
const AMP_T2: [f64; HARMONICS] = [
    0.912761, 0.102303, 0.013053, 0.005274, 0.000488, 0.000397, 0.000169, 0.002714, 0.000058,
    0.000034, 0.0, 0.0,
];
/// Phases of [`AMP_T2`].
const PHASE_T2: [f64; HARMONICS] = [
    0.24941, -0.38744, -0.52371, -0.98464, 0.12435, 0.71689, 1.43619, -0.52745, 2.09046, 2.96041,
    -1.04092, -0.71610,
];
/// Amplitudes of the table `DM 0xB324D` (notes 48 and up; notes below 36 are not summed by the
/// processor `0x121E2B` and reuse the table of note 36, ASSUMED: the firmware uses `DM 0xB3049`): `0.9825 sin - 0.0175 cos 2th -
/// 0.00175 cos 3th`; reconstruction error 6.8e-7.
const AMP_T3: [f64; HARMONICS] = [
    0.982516, 0.017472, 0.001747, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
];
/// Phases of [`AMP_T3`].
const PHASE_T3: [f64; HARMONICS] = [
    -FRAC_PI_2, -PI, PI, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
];

/// Per-note ring amplitude `A` written by the ARM key-down at `DM 0x95122 + (note - 36)`, notes
/// 36..115.
///
/// EMPIRICAL: median of the oracle fits over footages that are not percussion-overwritten; gaps
/// (74, 80..83, 109..115) interpolated; the value varies by up to 10 % with the key. Not applied by
/// the bus: the matching weights `W(key, footage)` (taper bytes) are not tabulated, so a flat
/// level is the neutral choice (`docs/research/b3-generator.md` section 4).
pub const WHEEL_GAIN: [f32; 80] = [
    0.4858, 0.5192, 0.4802, 0.4829, 0.4336, 0.4646, 0.4344, 0.3784, 0.4075, 0.3669, 0.3902, 0.3823,
    0.3755, 0.3758, 0.3680, 0.3569, 0.3758, 0.3632, 0.3630, 0.3406, 0.3502, 0.3451, 0.3465, 0.3439,
    0.2940, 0.2824, 0.2904, 0.3298, 0.2879, 0.4095, 0.3988, 0.3425, 0.3510, 0.3498, 0.3845, 0.3739,
    0.3704, 0.3805, 0.3901, 0.3998, 0.3998, 0.4093, 0.3842, 0.4501, 0.4627, 0.4753, 0.4879, 0.5004,
    0.5130, 0.5015, 0.5148, 0.5059, 0.5465, 0.4932, 0.5497, 0.6399, 0.5505, 0.5713, 0.5731, 0.5833,
    0.4720, 0.7005, 0.5863, 0.7624, 0.6681, 0.5948, 0.4731, 0.5536, 0.6951, 0.7180, 0.7569, 0.7214,
    0.5104, 0.5515, 0.5926, 0.6336, 0.6747, 0.6747, 0.6747, 0.6747,
];

/// Phase step of the base oscillator `pitch_class` (octave 0) in 2^-32 cycle per DSP call:
/// `(inc * 0x3F000000 + 2^30) >> 31` (SHARC `ssfr` rounding).
pub const fn base_step(pitch_class: usize) -> u32 {
    base_step_at(pitch_class, FINE_WORD)
}

/// [`base_step`] for an arbitrary fine tune word.
pub const fn base_step_at(pitch_class: usize, fine_word: u64) -> u32 {
    ((PHASE_INC[pitch_class % 12] as u64 * fine_word + (1 << 30)) >> 31) as u32
}

/// Frequency of `note` in Hz as the firmware sounds it: the step at the ARM's fine tune word
/// ([`ARM_FINE_WORD`]), [`FRAMES_PER_CALL`] frames per call, at `FIRMWARE_FRAME_RATE_HZ`.
///
/// VERIFIED (note 60 = C4 +0.18 cent, the ladder at most 1.3 cent from 12-TET); [`note_hz`] keeps
/// the engine's exact-C anchor.
pub fn firmware_note_hz(note: usize) -> f64 {
    let step = base_step_at(note % 12, ARM_FINE_WORD) << (note / 12);
    f64::from(step) / 4_294_967_296.0 * FIRMWARE_FRAME_RATE_HZ / FRAMES_PER_CALL
}

/// Phase step of MIDI `note`: the base step shifted up by the octave (separate accumulators).
pub const fn note_step(note: usize) -> u32 {
    base_step(note % 12) << (note / 12)
}

/// Frequency of `note` in Hz: the C of every octave is exact 12-TET (`A4 = 440` Hz), the other
/// pitch classes follow the ladder ([`PHASE_INC`], 0.94 cent from 12-TET at most).
///
/// The absolute anchor is the engine's exact C: [`firmware_note_hz`] (frame rate and Fine Tune word
/// VERIFIED, `docs/research/dsp-frame-rate.md`) differs from it by 0.18 cent at note 60 and by at
/// most 1.3 cent over 24..115; the ladder ratios are VERIFIED.
pub fn note_hz(note: usize) -> f64 {
    let c4 = 440.0 * (-9.0f64 / 12.0).exp2();
    c4 * f64::from(note_step(note)) / f64::from(note_step(60))
}

/// Which firmware table serves `note`: 0 = `0xB314B` (36..39 and below), 1 = `0xB31CC` (40..47),
/// 2 = `0xB324D` (48 and up).
pub const fn wheel_table(note: usize) -> usize {
    if note < 40 {
        0
    } else if note < 48 {
        1
    } else {
        2
    }
}

/// 129-point table of one wheel class from its harmonics.
fn build_table(amp: &[f64; HARMONICS], phase: &[f64; HARMONICS]) -> [f32; TABLE_POINTS] {
    let mut t = [0.0f32; TABLE_POINTS];
    for (i, v) in t.iter_mut().enumerate() {
        let p = i as f64 / (TABLE_POINTS - 1) as f64;
        *v = amp
            .iter()
            .zip(phase)
            .enumerate()
            .map(|(h, (a, ph))| a * (TAU * (h + 1) as f64 * p + ph).cos())
            .sum::<f64>() as f32;
    }
    t
}

/// Bank of one phase-stepped wheel per tonewheel index (`note = 24 + w`).
#[derive(Debug, Clone)]
pub struct B3Wheels {
    tables: [[f32; TABLE_POINTS]; 3],
    phase: [u32; NUM_TONEWHEELS],
    step: [u32; NUM_TONEWHEELS],
    class: [u8; NUM_TONEWHEELS],
}

impl B3Wheels {
    /// Builds the three wheel tables; wheels start at phase 0 with zero step until [`Self::retune`].
    pub fn new() -> Self {
        let mut class = [0u8; NUM_TONEWHEELS];
        for (w, c) in class.iter_mut().enumerate() {
            *c = wheel_table(WHEEL0_NOTE + w) as u8;
        }
        Self {
            tables: [
                build_table(&AMP_T1, &PHASE_T1),
                build_table(&AMP_T2, &PHASE_T2),
                build_table(&AMP_T3, &PHASE_T3),
            ],
            phase: [0; NUM_TONEWHEELS],
            step: [0; NUM_TONEWHEELS],
            class,
        }
    }

    /// Per-sample phase steps for `sample_rate` and `fine_cents` of Fine Tune.
    pub fn retune(&mut self, sample_rate: f32, fine_cents: f32) {
        let fine = (f64::from(fine_cents) / 1200.0).exp2();
        let inv_fs = 1.0 / f64::from(sample_rate);
        for (w, s) in self.step.iter_mut().enumerate() {
            let cycles = note_hz(WHEEL0_NOTE + w) * fine * inv_fs;
            *s = (cycles * 4_294_967_296.0).round() as u32;
        }
    }

    /// Advances every wheel by one sample and writes the wheel outputs.
    pub fn advance(&mut self, out: &mut [f32; NUM_TONEWHEELS]) {
        for (((o, ph), st), &c) in out
            .iter_mut()
            .zip(self.phase.iter_mut())
            .zip(self.step.iter())
            .zip(self.class.iter())
        {
            let table = &self.tables[usize::from(c)];
            let idx = (*ph >> 25) as usize;
            let frac = (*ph & 0x01FF_FFFF) as f32 * (1.0 / 33_554_432.0);
            *o = table[idx] + frac * (table[idx + 1] - table[idx]);
            *ph = ph.wrapping_add(*st);
        }
    }

    /// Current phase of wheel `w` (units of 2^-32 cycle), for tests.
    #[cfg(test)]
    fn phase_of(&self, w: usize) -> u32 {
        self.phase[w]
    }
}

impl Default for B3Wheels {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_match_the_oracle_fits() {
        // Oracle ring words (organ_b3_gen.py scan): note 60 0.011938497, note 67 0.017884120,
        // note 91 0.071536481 cycles per call.
        for (note, fit) in [
            (60usize, 0.011_938_497f64),
            (67, 0.017_884_120),
            (91, 0.071_536_481),
        ] {
            let f = f64::from(note_step(note)) / 4_294_967_296.0;
            assert!((f / fit - 1.0).abs() < 1e-7, "note {note}: {f} vs {fit}");
        }
        assert_eq!(note_step(60), 51_275_456);
        assert_eq!(note_step(72), 2 * note_step(60));
    }

    #[test]
    fn rounding_is_ssfr_not_floor() {
        // pitch class 1: `floor` would give a step one unit lower
        let prod = u64::from(PHASE_INC[1]) * FINE_WORD;
        assert_eq!(u64::from(base_step(1)), (prod + (1 << 30)) >> 31);
        assert_ne!(u64::from(base_step(1)), prod >> 31);
    }

    #[test]
    fn pitch_is_12tet_within_a_cent_and_c_is_exact() {
        assert!((note_hz(60) - 261.625_565).abs() < 1e-4);
        assert!((note_hz(69) / 440.0 - 1.0).abs() < 4e-4); // ladder: A is 0.41 cent flat
        for note in 24..115usize {
            let ideal = 440.0 * ((note as f64 - 69.0) / 12.0).exp2();
            let cents = 1200.0 * (note_hz(note) / ideal).log2();
            assert!(cents.abs() < 1.0, "note {note}: {cents} cent");
        }
    }

    #[test]
    fn frame_rate_follows_from_the_arm_fine_tune_word() {
        let cents = |a: f64, b: f64| 1200.0 * (a / b).log2();
        let c4 = 440.0 * (-9.0f64 / 12.0).exp2();
        // frame rate that makes note 60 = C4 for a fine tune word (2 frames per call)
        let implied = |w: u64| {
            let step = f64::from(base_step_at(0, w) << 5);
            c4 * FRAMES_PER_CALL * 4_294_967_296.0 / step
        };
        // image default word: the old 43,829 Hz
        let old = implied(FINE_WORD);
        assert!((old - 43_829.0).abs() < 1.0, "{old}");
        // ARM-written word: the ARM's sample-rate literal within 0.3 cent
        let f = implied(ARM_FINE_WORD);
        assert!(cents(f, FIRMWARE_FRAME_RATE_HZ).abs() < 0.3, "{f}");
        assert_eq!(base_step_at(0, ARM_FINE_WORD) << 5, 51_784_672);
        // the two words differ by 17.11 cent: the +0.18 cent of note 60 plus the -16.93 cent it has at
        // the image word
        let r = cents(ARM_FINE_WORD as f64, FINE_WORD as f64);
        assert!((r - 17.11).abs() < 0.01, "{r}");
        // firmware pitch: note 60 within 0.2 cent of C4, every wheel within 1.3 cent of 12-TET
        assert!(cents(firmware_note_hz(60), c4).abs() < 0.2);
        for note in 24..115usize {
            let ideal = 440.0 * ((note as f64 - 69.0) / 12.0).exp2();
            let c = cents(firmware_note_hz(note), ideal);
            assert!(c.abs() < 1.3, "note {note}: {c} cent");
        }
    }

    #[test]
    fn wheel_table_classes() {
        assert_eq!(wheel_table(24), 0);
        assert_eq!(wheel_table(39), 0);
        assert_eq!(wheel_table(40), 1);
        assert_eq!(wheel_table(47), 1);
        assert_eq!(wheel_table(48), 2);
        assert_eq!(wheel_table(114), 2);
    }

    #[test]
    fn tables_are_cyclic_and_the_upper_wheel_is_a_near_sine() {
        let w = B3Wheels::new();
        for t in &w.tables {
            assert!((t[0] - t[128]).abs() < 1e-6);
            assert!(t.iter().take(128).sum::<f32>().abs() < 1e-2); // no DC
        }
        let t3 = &w.tables[2];
        // 0.9825 sin(2 pi p) - 0.0175 cos(4 pi p) - 0.00175 cos(6 pi p)
        assert!((t3[32] - 1.0).abs() < 3e-3);
        let peak = t3.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!((0.99..1.01).contains(&peak), "peak {peak}");
        // low wheels carry a strong second harmonic (VERIFIED amplitudes 0.148 and 0.102)
        let a2 = |t: &[f32; TABLE_POINTS]| {
            let (mut c, mut s) = (0.0f64, 0.0f64);
            for (i, v) in t.iter().take(128).enumerate() {
                let a = TAU * 2.0 * i as f64 / 128.0;
                c += f64::from(*v) * a.cos();
                s += f64::from(*v) * a.sin();
            }
            (c * c + s * s).sqrt() / 64.0
        };
        assert!((a2(&w.tables[0]) - 0.148).abs() < 2e-3);
        assert!((a2(&w.tables[1]) - 0.102).abs() < 2e-3);
    }

    #[test]
    fn advance_produces_the_expected_frequency() {
        let fs = 48_000.0f32;
        let mut b = B3Wheels::new();
        b.retune(fs, 0.0);
        let w = 60 - WHEEL0_NOTE; // note 60
        let mut out = [0.0f32; NUM_TONEWHEELS];
        let mut crossings = 0;
        let mut prev = 0.0f32;
        for _ in 0..48_000 {
            b.advance(&mut out);
            if prev < 0.0 && out[w] >= 0.0 {
                crossings += 1;
            }
            prev = out[w];
        }
        assert!((crossings as f64 - note_hz(60)).abs() <= 1.0, "{crossings}");
    }

    #[test]
    fn fine_tune_shifts_by_cents_and_phase_wraps() {
        let mut a = B3Wheels::new();
        let mut b = B3Wheels::new();
        a.retune(48_000.0, 0.0);
        b.retune(48_000.0, 100.0);
        let w = 36;
        let ratio = f64::from(b.step[w]) / f64::from(a.step[w]);
        assert!((1200.0 * ratio.log2() - 100.0).abs() < 0.01);
        let mut out = [0.0f32; NUM_TONEWHEELS];
        for _ in 0..10_000 {
            a.advance(&mut out);
        }
        assert_eq!(a.phase_of(w), a.step[w].wrapping_mul(10_000));
    }

    #[test]
    fn gain_table_spot_values_follow_the_arm_key_down_floats() {
        // DM 0x9513A (note 60) 0x3E9689D0 = 0.2939 -> 0.2940 (fit); 0x95152 (note 84) 0.5129
        assert!((WHEEL_GAIN[60 - 36] - 0.294).abs() < 1e-3);
        assert!((WHEEL_GAIN[84 - 36] - 0.5129).abs() < 2e-3);
        assert!((WHEEL_GAIN[67 - 36] - 0.3427).abs() < 3e-3);
    }
}
