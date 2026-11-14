//! Frame adapter that runs the firmware Amp EQ (opt-in, `FirmwareFx::eq`): the ported tone stack
//! `0x1243F4` ([`super::FwToneStack2`], bit exact against the `sharc-fx` oracle) with the words
//! the ARM sends for `kAmpTreble`, `kAmpBass`, `kAmpMid` and `kAmpMidFreq`.
//!
//! The routine runs once per call on two stereo frames, so the stage collects two frames,
//! processes them and releases them one block later: **latency 2 frames** (46 us at 48 kHz), the
//! first two output frames are silence. Allocation free and `Copy`.
//!
//! Provenance (`docs/research/fx-eq-words.md`):
//! * VERIFIED: the routine (oracle, `tests_resonant_golden.rs`); every treble, bass and mid word
//!   at the captured counts (`fw_map` and `fw_eq_tables`, piano and organ units equal).
//! * VERIFIED (by the tests below, from the ported routine): with `L = 1` the high shelf has
//!   Nyquist gain `1 + F`, the bass feed has DC gain `1 + A`, and gain 0 is the identity.
//! * ASSUMED: smoothing `k = 0.99078` (the FX per-call smoother of `fx-dsp-map.md` s5; it only
//!   sets how fast a knob move settles), clip bound `L = 1.0` (the oracle golden context holds
//!   it), the product form of the joint mid words and the interpolation between captured
//!   indices (`fw_eq_tables`), and that the organ unit runs the same recurrence (its words equal
//!   the piano's; its routine layout is not decoded).
//! * The recurrence runs at the engine rate, the firmware at 43,402.34 Hz
//!   (`docs/research/dsp-frame-rate.md`): every corner and centre frequency is `fs / 43402.34`
//!   higher (1.106 at 48 kHz). The words are not rescaled (kept bit for bit).

use super::fw_eq_tables as mid;
use super::fw_map::{tone_bass_smoother, tone_shelf_gain, tone_shelf_pole};
use super::fw_tone_stack::{ctx, FwToneStack2, CTX_WORDS};

/// ASSUMED: per-call parameter smoother `k` (`0x3F7DA3C2`, `0.99078`, the FX smoothing constant).
const SMOOTH_K: f32 = f32::from_bits(0x3F7D_A3C2);
/// ASSUMED: clip bound and output reference `L` of the tone stack (1.0 in the oracle golden).
const LIMIT: f32 = 1.0;
/// Default of `kAmpMidFreq` (`arm-edit-params.csv`).
pub(super) const DEFAULT_FREQ_INDEX: f32 = 64.0;

/// The tone stack context words of a setting: `(offset, word)` target pairs and their smoothed
/// state copies (`+2` words above the target for the five smoothed pairs).
fn target_words(bass: f32, mid_counts: f32, treble: f32, freq: f32) -> [(i32, f32); 8] {
    let (b, c, d) = mid::words(mid_counts, freq);
    [
        (-0x20, tone_shelf_gain(bass)),
        (-0x1F, b),
        (-0x1C, tone_shelf_pole(treble)),
        (-0x1B, tone_shelf_gain(treble)),
        (-0x17, c),
        (-0x18, d),
        (-0x14, tone_bass_smoother(bass)),
        (-0x11, SMOOTH_K),
    ]
}

/// Firmware EQ stage: tone stack plus the 2-frame buffers.
#[derive(Debug, Clone, Copy)]
pub(super) struct FwEqStage {
    stack: FwToneStack2,
    input: [[f32; 2]; 2],
    output: [[f32; 2]; 2],
    pos: usize,
}

impl FwEqStage {
    /// Stage for `bass`, `mid`, `treble` counts (`-60..=60`) and mid frequency index `freq`
    /// (`0..=127`), settled (the smoothed words start at their targets).
    pub(super) fn new(bass: f32, mid: f32, treble: f32, freq: f32) -> Self {
        let mut stack = FwToneStack2::default();
        stack.set_value(ctx::LIMIT, LIMIT);
        let mut s = Self {
            stack,
            input: [[0.0; 2]; 2],
            output: [[0.0; 2]; 2],
            pos: 0,
        };
        s.set(bass, mid, treble, freq, true);
        s
    }

    /// Moves the targets; the routine's own smoother glides there unless `settle`.
    pub(super) fn set(&mut self, bass: f32, mid: f32, treble: f32, freq: f32, settle: bool) {
        for (off, v) in target_words(bass, mid, treble, freq) {
            self.stack.set_value(off, v);
            // smoothed copies: A -0x1E, B -0x1D, E -0x1A, F -0x19, C -0x15, D -0x16
            let copy = match off {
                -0x20 | -0x1F | -0x1C | -0x1B => Some(off + 2),
                -0x17 | -0x18 => Some(off + 2),
                _ => None,
            };
            if let (true, Some(c)) = (settle, copy) {
                self.stack.set_value(c, v);
            }
        }
    }

    /// Pushes one frame and returns the frame of the previous block at the same position.
    #[inline]
    pub(super) fn process_frame(&mut self, frame: [f32; 2]) -> [f32; 2] {
        let out = self.output[self.pos];
        self.input[self.pos] = frame;
        self.pos += 1;
        if self.pos == 2 {
            self.pos = 0;
            let mut block = [
                self.input[0][0],
                self.input[0][1],
                self.input[1][0],
                self.input[1][1],
            ];
            self.stack.process(&mut block);
            self.output = [[block[0], block[1]], [block[2], block[3]]];
        }
        out
    }
}

/// Counts `-60..=60` of a gain in dB (the engine's `0.25 dB` per count), rounded.
pub(super) fn counts_of_db(db: f32) -> f32 {
    (db / super::EQ_GAIN_DB_PER_COUNT)
        .round()
        .clamp(-60.0, 60.0)
}

/// Mid frequency index (`0..=127`) of a frequency in Hz: the nearest display step.
pub(super) fn freq_index_of_hz(hz: f32) -> f32 {
    let target = hz.max(1.0).ln();
    let mut best = (f32::MAX, 0usize);
    for (i, f) in super::EQ_FREQ_HZ.iter().enumerate() {
        let d = (f.ln() - target).abs();
        if d < best.0 {
            best = (d, i);
        }
    }
    best.1 as f32
}

const _: () = assert!(CTX_WORDS == 32);

#[cfg(test)]
mod tests {
    use super::*;

    /// Steady-state gain of the stage for a sine of `cycles` per sample, in dB.
    fn gain_db(stage: &mut FwEqStage, cycles: f32) -> f32 {
        let n = 20_000;
        let (mut i2, mut o2) = (0.0f64, 0.0f64);
        for k in 0..n {
            let x = 0.1 * (2.0 * std::f32::consts::PI * cycles * k as f32).sin();
            let y = stage.process_frame([x, x])[0];
            if k >= n / 2 {
                i2 += f64::from(x * x);
                o2 += f64::from(y * y);
            }
        }
        (10.0 * (o2 / i2).log10()) as f32
    }

    #[test]
    fn neutral_setting_is_a_two_frame_delay() {
        let mut s = FwEqStage::new(0.0, 0.0, 0.0, 64.0);
        let x: Vec<f32> = (0..16)
            .map(|k| ((k * 7 % 11) as f32 - 5.0) * 0.05)
            .collect();
        let y: Vec<f32> = x.iter().map(|&v| s.process_frame([v, -v])[0]).collect();
        assert_eq!(&y[..2], &[0.0, 0.0], "two silent frames");
        for k in 2..16 {
            assert!(
                (y[k] - x[k - 2]).abs() < 1e-6,
                "frame {k}: {} {}",
                y[k],
                x[k - 2]
            );
        }
    }

    #[test]
    fn treble_gain_at_nyquist_is_one_plus_f() {
        for counts in [-60.0f32, -30.0, 30.0, 60.0] {
            let mut s = FwEqStage::new(0.0, 0.0, counts, 64.0);
            let want = 20.0 * (1.0 + tone_shelf_gain(counts)).log10();
            let got = gain_db(&mut s, 0.5 - 1e-4);
            assert!((got - want).abs() < 0.05, "{counts}: {got} against {want}");
        }
        // the boost side stops 0.23 dB short of the engine's 15 dB, the cut side is 15.02 dB
        assert!(20.0 * (1.0 + tone_shelf_gain(60.0)).log10() < 14.8);
        assert!(20.0 * (1.0 + tone_shelf_gain(-60.0)).log10() < -14.9);
    }

    #[test]
    fn bass_gain_at_dc_is_one_plus_a() {
        for counts in [-60.0f32, -30.0, 30.0, 60.0] {
            let mut s = FwEqStage::new(counts, 0.0, 0.0, 64.0);
            let want = 20.0 * (1.0 + tone_shelf_gain(counts)).log10();
            let got = gain_db(&mut s, 0.0001);
            assert!((got - want).abs() < 0.1, "{counts}: {got} against {want}");
        }
    }

    #[test]
    fn mid_band_boosts_and_cuts_around_a_centre_that_follows_the_index() {
        let peak = |counts: f32, idx: f32| -> (f32, f32) {
            let mut best = (f32::MIN, 0.0);
            for k in 1..120 {
                let cycles = 0.0008 * 1.07f32.powi(k);
                if cycles >= 0.5 {
                    break;
                }
                let mut s = FwEqStage::new(0.0, counts, 0.0, idx);
                let g = gain_db(&mut s, cycles);
                let g = if counts < 0.0 { -g } else { g };
                if g > best.0 {
                    best = (g, cycles);
                }
            }
            best
        };
        let (boost, at_boost) = peak(60.0, 64.0);
        let (cut, at_cut) = peak(-60.0, 64.0);
        assert!(boost > 8.0, "boost {boost} dB");
        assert!(cut > 8.0, "cut {cut} dB");
        assert!((at_boost / at_cut - 1.0).abs() < 0.2, "{at_boost} {at_cut}");
        let (_, low) = peak(60.0, 20.0);
        let (_, high) = peak(60.0, 110.0);
        assert!(low < at_boost && at_boost < high, "{low} {at_boost} {high}");
    }

    #[test]
    fn stays_finite_on_full_scale_noise_at_the_extremes() {
        let mut s = FwEqStage::new(60.0, 60.0, 60.0, 127.0);
        let mut seed = 12345u32;
        for _ in 0..50_000 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let x = (seed >> 8) as f32 / 8_388_608.0 - 1.0;
            let y = s.process_frame([x, -x]);
            assert!(y[0].is_finite() && y[1].is_finite() && y[0].abs() < 100.0);
        }
    }

    #[test]
    fn engine_units_map_to_counts_and_indices() {
        assert_eq!(counts_of_db(3.75), 15.0);
        assert_eq!(counts_of_db(-40.0), -60.0);
        assert_eq!(freq_index_of_hz(200.0), 0.0);
        assert_eq!(freq_index_of_hz(8000.0), 127.0);
        let i = freq_index_of_hz(1000.0);
        assert!((i - 64.0).abs() <= 3.0, "{i}");
    }

    #[test]
    fn retargeting_glides_unless_settled() {
        let mut glide = FwEqStage::new(0.0, 0.0, 0.0, 64.0);
        glide.set(0.0, 0.0, 60.0, 64.0, false);
        let mut settled = FwEqStage::new(0.0, 0.0, 0.0, 64.0);
        settled.set(0.0, 0.0, 60.0, 64.0, true);
        let first = |s: &mut FwEqStage| {
            let mut out = 0.0f32;
            for k in 0..4 {
                let x = if k % 2 == 0 { 0.1 } else { -0.1 };
                out = s.process_frame([x, x])[0];
            }
            out.abs()
        };
        assert!(
            first(&mut settled) > first(&mut glide),
            "the glide starts below the target"
        );
    }
}
