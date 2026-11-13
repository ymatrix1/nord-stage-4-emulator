//! The closed forms of `fw_map` against the captured series of `tests_fw_series.rs` (float32
//! words, error below `1e-5` of the series range, as `tools/tests/test_fx_forms.py`), plus the
//! 13-point tone tables at their nodes.

use super::fw_map::*;
use super::tests_fw_series as s;

/// Largest `|form(v) - word|` over a series, relative to the series' range (at least 1e-9).
fn worst(series: &[(i32, u32)], form: impl Fn(i32) -> f32) -> f32 {
    let words: Vec<f32> = series.iter().map(|&(_, w)| f32::from_bits(w)).collect();
    let span = words.iter().cloned().fold(f32::MIN, f32::max)
        - words.iter().cloned().fold(f32::MAX, f32::min);
    series
        .iter()
        .zip(&words)
        .map(|(&(v, _), &w)| (form(v) - w).abs() / span.max(1e-9))
        .fold(0.0, f32::max)
}

const TOL: f32 = 1.0e-5;

#[test]
fn delay_laws_match_the_series() {
    assert!(worst(&s::DLY_SEND, |v| delay_send(v as f32)) < TOL);
    assert!(worst(&s::DLY_DRY, |v| delay_dry(v as f32)) < TOL);
    assert!(worst(&s::DLY_FBK, |v| delay_feedback(v as f32)) < TOL);
}

#[test]
fn compressor_laws_match_the_series() {
    assert!(worst(&s::CMP_MAKEUP, |v| cmp_makeup(v as f32)) < TOL);
    assert!(worst(&s::CMP_THRESHOLD, |v| cmp_threshold(v as f32)) < TOL);
}

#[test]
fn drive_and_allpass_laws_match_the_series() {
    assert!(worst(&s::AMP_DRIVE, |v| amp_drive_target(v as f32)) < TOL);
    for series in [&s::AP_RATE_T2[..], &s::AP_RATE_T3[..]] {
        for &(v, word) in series {
            assert_eq!(word, allpass_rate(v as u32), "rate at {v}");
        }
    }
    for wet in [&s::AP_WET_T2[..], &s::AP_WET_T3[..]] {
        assert!(worst(wet, |v| allpass_wet(v as f32)) < TOL);
    }
    for dry in [&s::AP_DRY_T2[..], &s::AP_DRY_T3[..]] {
        assert!(worst(dry, |v| allpass_dry(v as f32)) < TOL);
    }
}

#[test]
fn tone_tables_reproduce_their_captured_nodes_exactly() {
    assert_eq!(worst(&s::TONE_F, |v| tone_shelf_gain(v as f32)), 0.0);
    assert_eq!(worst(&s::TONE_A, |v| tone_shelf_gain(v as f32)), 0.0);
    assert_eq!(worst(&s::TONE_E, |v| tone_shelf_pole(v as f32)), 0.0);
    assert_eq!(
        worst(&s::TONE_BASS_A, |v| tone_bass_smoother(v as f32)),
        0.0
    );
}

#[test]
fn tone_interpolation_stays_between_neighbouring_nodes() {
    for i in -600..=600 {
        let c = i as f32 / 10.0;
        let g = tone_shelf_gain(c);
        let (lo, hi) = (tone_shelf_gain(c.floor()), tone_shelf_gain(c.ceil()));
        assert!(
            g >= lo.min(hi) - 1e-6 && g <= lo.max(hi) + 1e-6,
            "count {c}"
        );
    }
}
