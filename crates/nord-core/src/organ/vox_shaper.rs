//! Waveshaper of the Vox note oscillators: a band-limited step regenerated in closed form.
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1.7. The PNO Core1 post stage `0x12272D` looks
//! the clipped fold of every note phase up in a 1,025-point table `T(i)`, `i = -512..512`. The
//! firmware table is raw image data and is not committed; it is the integral of a Blackman
//! windowed sinc (an anti-aliased clipper), regenerated here:
//!
//! ```text
//! k_j = sinc(t_j / a) * blackman(t_j / 512),   t_j = j - 512 + shift,   j = 0..1024
//! T_j = -1 + 2 (sum_(i<=j) k_i - k_j / 2) / sum_i k_i
//! ```

use std::f64::consts::PI;

/// Number of table points (`i = -512..=512`).
pub const TABLE_POINTS: usize = 1025;
/// Table half range: the clipped argument `x` in `-1..=1` addresses `fix(x * 512)`.
pub const TABLE_HALF: i32 = 512;
/// EMPIRICAL: sinc scale in table steps, fitted to the image table with
/// `tools/organ_shaper.py` (rms error 1.7e-4, max 3.7e-4; the clean `512 / 3` gives 2.7e-4).
const SINC_SCALE: f64 = 170.58;
/// EMPIRICAL: half-sample shift of the fit (clean value -0.5).
const SHIFT: f64 = -0.4638;
/// EMPIRICAL: Blackman `alpha` of the fit (textbook 0.16).
const ALPHA: f64 = 0.16;

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        1.0
    } else {
        (PI * x).sin() / (PI * x)
    }
}

fn blackman(u: f64) -> f64 {
    if u.abs() > 1.0 {
        0.0
    } else {
        (1.0 - ALPHA) / 2.0 + 0.5 * (PI * u).cos() + ALPHA / 2.0 * (2.0 * PI * u).cos()
    }
}

/// The band-limited step table, `-1` at `i = -512` to `+1` at `i = 512` with the sinc overshoot
/// (about 1.09 at `|i| = 171`). Built once in `new()`; the audio path only reads it.
pub fn blep_table() -> Box<[f32; TABLE_POINTS]> {
    let mut kernel = [0.0f64; TABLE_POINTS];
    for (j, k) in kernel.iter_mut().enumerate() {
        let t = j as f64 - f64::from(TABLE_HALF) + SHIFT;
        *k = sinc(t / SINC_SCALE) * blackman(t / f64::from(TABLE_HALF));
    }
    let total: f64 = kernel.iter().sum();
    let mut table = Box::new([0.0f32; TABLE_POINTS]);
    let mut acc = 0.0f64;
    for (v, k) in table.iter_mut().zip(kernel.iter()) {
        acc += k;
        *v = (-1.0 + 2.0 * (acc - 0.5 * k) / total) as f32;
    }
    table
}

/// `T[fix(x * 512)]` for `x` clamped to `-1..=1` (`fix` rounds to nearest even like the SHARC).
#[inline]
pub fn shape(table: &[f32; TABLE_POINTS], x: f32) -> f32 {
    let i = (x.clamp(-1.0, 1.0) * TABLE_HALF as f32).round_ties_even() as i32;
    table[(i + TABLE_HALF) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_runs_from_minus_one_to_one_and_is_nearly_odd() {
        let t = blep_table();
        assert!((t[0] + 1.0).abs() < 1e-3 && (t[1024] - 1.0).abs() < 1e-3);
        assert!(t[512].abs() < 0.01);
        let odd = t
            .iter()
            .zip(t.iter().rev())
            .map(|(a, b)| (a + b).abs())
            .fold(0.0f32, f32::max);
        assert!(odd < 0.02, "even part {odd}");
        assert!(t.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn overshoot_is_nine_percent_at_a_third_of_the_range() {
        let t = blep_table();
        let (peak, v) = t
            .iter()
            .enumerate()
            .fold((0, f32::MIN), |b, (i, &v)| if v > b.1 { (i, v) } else { b });
        assert!((peak as i32 - 512 - 171).abs() <= 2, "peak at {peak}");
        assert!(v > 1.07 && v < 1.11, "{v}");
        let at128 = t[512 + 128];
        assert!((1.0..1.06).contains(&at128), "{at128}");
    }

    #[test]
    fn matches_python_model_samples() {
        // tools/organ_shaper.py blep_table(*FITTED), printed with
        // `python tools/organ_vox_golden.py rust` (derived values, no firmware data).
        let t = blep_table();
        for (i, want) in super::super::vox_golden::TABLE_SAMPLES {
            assert!((t[i] - want).abs() < 2e-6, "{i}: {} vs {want}", t[i]);
        }
    }

    #[test]
    fn lookup_clamps_and_rounds_to_even() {
        let t = blep_table();
        assert_eq!(shape(&t, -3.0), t[0]);
        assert_eq!(shape(&t, 3.0), t[1024]);
        assert_eq!(shape(&t, 0.0), t[512]);
        // 0.5 / 512 rounds to 0 (even), 1.5 / 512 to 2
        assert_eq!(shape(&t, 0.5 / 512.0), t[512]);
        assert_eq!(shape(&t, 1.5 / 512.0), t[514]);
    }
}
