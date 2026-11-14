//! Vox 36-sine bank: the second half of the PNO Core1 post stage `0x12272D` (section 3.4 of
//! `docs/research/organ-post-stage.md`), the source of the 12 bucket words (rows 84..95) of the
//! 96-word block that the output stage weights.
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1.7; golden vectors `vox_golden.rs` from the
//! bit-exact model `tools/organ_post_ref.py` (`sines`), data `vox_image.rs`. Per firmware frame:
//!
//! ```text
//! base   = B (1 + mod) = 2 B m                     B = DM(0xE2BE0) / 2^31, m = the note pitch factor
//! ph_j  += base L_j                                L_j = DM(0xE2C8E + j), words of the 32-bit phase
//! u      = 2 |s_j| - 1                             s_j = signed phase -1..1 (one cycle per wrap)
//! y_j    = u g_j (((c7 u^2 + c5) u^2 + c3) u^2 + c1)     (about g_j sin(pi u / 2))
//! bucket[b(j)] += y_j      b(j) = j for j < 12, j - 12, j - 24 (j < 32), j - 32 for the groups
//! ```
//!
//! At the image base word the sines run at 0.096 (j = 0) to 0.45 cycles per frame, 4.2 to
//! 19.5 kHz at the firmware frame rate: not pitch carriers; they are the top rows of the Vox key
//! map (keys above C6 with the 2' footage fold onto them). Everything is allocated in `new()`.

use super::lane_mix::FIRMWARE_FRAME_RATE_HZ;
use super::vox_image as img;

/// Number of sine oscillators.
pub const SINES: usize = 36;
/// Number of bucket words (rows 84..95 of the block).
pub const BUCKETS: usize = 12;

const COUNTS: f64 = 4_294_967_296.0;
const WORD: f64 = 2_147_483_648.0;

/// The 36 sine oscillators of the post stage (state preallocated).
#[derive(Debug, Clone)]
pub struct VoxSines {
    /// 32.32 fixed point phases (the high word is the DM word).
    phase: [u64; SINES],
    ladder: [f64; SINES],
    gain: [f32; SINES],
    poly: [f32; 4],
    scale: f64,
}

fn group_gain(j: usize) -> f32 {
    let g = match j {
        0..=11 => 0,
        12..=23 => 1,
        24..=31 => 2,
        _ => 3,
    };
    f32::from_bits(img::SINE_GAIN_WORDS[g])
}

fn group_phase(j: usize) -> u64 {
    let g = match j {
        0..=23 => 0,
        24..=31 => 1,
        _ => 2,
    };
    u64::from(img::SINE_START[g]) << 32
}

impl VoxSines {
    /// Bank at the image start phases, engine rate `fs`.
    pub fn new(fs: f32) -> Self {
        let mut s = Self {
            phase: std::array::from_fn(group_phase),
            ladder: std::array::from_fn(|j| f64::from(img::SINE_LADDER[j])),
            gain: std::array::from_fn(group_gain),
            poly: std::array::from_fn(|i| f32::from_bits(img::SINE_POLY_WORDS[i])),
            scale: 1.0,
        };
        s.set_sample_rate(fs);
        s
    }

    /// Re-derives the rate conversion (phases kept).
    pub fn set_sample_rate(&mut self, fs: f32) {
        self.scale = FIRMWARE_FRAME_RATE_HZ / f64::from(fs.max(8000.0));
    }

    /// One frame: `two_m = 1 + mod` (twice the note pitch factor) advances the 36 phases and the
    /// 12 bucket sums are written to `out`.
    pub fn step(&mut self, two_m: f32, out: &mut [f32; BUCKETS]) {
        let per_word = f64::from(img::SINE_BASE) / WORD * f64::from(two_m) * self.scale * COUNTS;
        out.fill(0.0);
        for (j, ((ph, &l), &g)) in self
            .phase
            .iter_mut()
            .zip(&self.ladder)
            .zip(&self.gain)
            .enumerate()
        {
            *ph = ph.wrapping_add((per_word * l) as u64);
            let s = ((*ph >> 32) as u32 as i32) as f32 / WORD as f32;
            let u = 2.0 * s.abs() - 1.0;
            let u2 = u * u;
            let [c7, c5, c3, c1] = self.poly;
            let p = ((c7 * u2 + c5) * u2 + c3) * u2 + c1;
            out[usize::from(img::SINE_BUCKET[j])] += u * g * p;
        }
    }
}

#[cfg(test)]
/// Bucket of sine `j` (closed form of the image table): `DM(0x95E22 + 2 (j >> 1)) + (j & 1)` (VERIFIED, `vox_image` check).
fn bucket(j: usize) -> u8 {
    match j {
        0..=11 => j as u8,
        12..=23 => (j - 12) as u8,
        24..=31 => (j - 24) as u8,
        _ => (j - 32) as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::super::vox_golden as g;
    use super::*;

    #[test]
    fn ladder_is_the_closed_form_of_the_image_words() {
        for j in 0..SINES {
            let fold = match j {
                0..=23 => 1.0,
                24..=31 => 0.75,
                _ => 0.63,
            };
            let want =
                (f64::from(0x1AE8_9F99u32) * (j as f64 / 12.0).exp2() * fold).min(WORD - 1.0);
            let got = f64::from(img::SINE_LADDER[j]);
            assert!((got / want - 1.0).abs() < 1e-3, "{j}: {got} {want}"); // 0.75 and 0.63 are rounded
        }
    }

    #[test]
    fn buckets_cover_twelve_words_in_octave_groups() {
        let mut count = [0usize; BUCKETS];
        for j in 0..SINES {
            count[usize::from(bucket(j))] += 1;
            assert_eq!(bucket(j), img::SINE_BUCKET[j], "{j}");
        }
        assert_eq!(count, [4, 4, 4, 4, 3, 3, 3, 3, 2, 2, 2, 2]);
    }

    #[test]
    fn golden_buckets_match_the_python_model() {
        let mut bank = VoxSines::new(FIRMWARE_FRAME_RATE_HZ as f32);
        let mut out = [0.0f32; BUCKETS];
        let mut worst = 0.0f32;
        for f in 0..g::FRAMES {
            bank.step(1.0, &mut out);
            if f % g::STRIDE == 0 {
                for (b, &y) in out.iter().enumerate() {
                    worst = worst.max((y - g::B_PLAIN[f / g::STRIDE][b]).abs());
                }
            }
        }
        assert!(worst < 1e-3, "max {worst}");
    }

    #[test]
    fn one_sine_runs_at_the_image_rate_and_reaches_its_gain() {
        let mut bank = VoxSines::new(FIRMWARE_FRAME_RATE_HZ as f32);
        let mut out = [0.0f32; BUCKETS];
        let before = bank.phase[0];
        bank.step(1.0, &mut out);
        let cycles = bank.phase[0].wrapping_sub(before) as f64 / (COUNTS * COUNTS);
        let want = f64::from(img::SINE_BASE) / WORD * f64::from(img::SINE_LADDER[0]) / COUNTS;
        assert!((cycles / want - 1.0).abs() < 1e-6, "{cycles} {want}");
        // 0.096 cycles per frame = 4.2 kHz at the firmware frame rate
        assert!((cycles * FIRMWARE_FRAME_RATE_HZ - 4_180.0).abs() < 30.0);
        let mut peak = 0.0f32;
        for _ in 0..200 {
            bank.step(1.0, &mut out);
            peak = peak.max(out[0].abs());
        }
        assert!(peak > 0.9 && peak < 1.5, "{peak}"); // sines 0, 12, 24, 32 share bucket 0
    }

    #[test]
    fn frequency_follows_the_pitch_factor_and_engine_rate() {
        let rate = |fs: f32, two_m: f32| {
            let mut b = VoxSines::new(fs);
            let mut out = [0.0f32; BUCKETS];
            let before = b.phase[5];
            b.step(two_m, &mut out);
            b.phase[5].wrapping_sub(before) as f64 / (COUNTS * COUNTS) * f64::from(fs)
        };
        let a = rate(FIRMWARE_FRAME_RATE_HZ as f32, 1.0);
        assert!((rate(48_000.0, 1.0) / a - 1.0).abs() < 1e-6);
        assert!((rate(48_000.0, 1.01) / a - 1.01).abs() < 1e-6);
    }

    #[test]
    fn output_is_finite_and_bounded() {
        let mut bank = VoxSines::new(48_000.0);
        let mut out = [0.0f32; BUCKETS];
        for _ in 0..20_000 {
            bank.step(1.035, &mut out);
            assert!(out.iter().all(|v| v.is_finite() && v.abs() < 4.0));
        }
    }
}
