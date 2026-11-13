//! Tests of the vibrato-level selection, the smoothed lane gains and the stereo blend of the
//! organ's `Firmware` link (`tonewheel.rs`, `lane_mix.rs`, `mod.rs::render_frame`).

use super::{OrganLayer, ScannerMode, VibratoLink};

const FS: f32 = 48_000.0;
const SETTLE: usize = 4000;

fn settled(mode: ScannerMode) -> OrganLayer {
    let mut o = OrganLayer::new(FS);
    o.scanner_mode = mode;
    o.crosstalk_level = 0.0;
    for _ in 0..SETTLE {
        o.render_tonewheel_lanes(1.0 / FS);
    }
    o
}

#[test]
fn scanner_mode_selects_table_and_lane_count() {
    let mut o = OrganLayer::new(FS);
    assert_eq!(o.render_tonewheel_lanes(1.0 / FS).1, 1);
    assert!(!o.tone_bank.copies());
    o.scanner_mode = ScannerMode::C2;
    assert_eq!(o.render_tonewheel_lanes(1.0 / FS).1, 2);
    assert!(o.tone_bank.copies());
    let mut manual = OrganLayer::new(FS);
    manual.vibrato_link = VibratoLink::Manual;
    manual.scanner_mode = ScannerMode::C2;
    assert_eq!(manual.render_tonewheel_lanes(1.0 / FS).1, 1);
    assert!(!manual.tone_bank.copies());
    manual.detuned_copies = true;
    assert_eq!(manual.render_tonewheel_lanes(1.0 / FS).1, 1);
    assert!(manual.tone_bank.copies());
    let mut forced = OrganLayer::new(FS);
    forced.detuned_copies = true;
    assert_eq!(forced.render_tonewheel_lanes(1.0 / FS).1, 2);
}

#[test]
fn settled_lanes_are_the_stereo_blend() {
    let mut o = settled(ScannerMode::V1);
    let mut bank = o.tone_bank.clone();
    let (mut a, mut b) = ([0.0; 91], [0.0; 91]);
    bank.advance_copies(&mut a, &mut b);
    let (buses, lanes) = o.render_tonewheel_lanes(1.0 / FS);
    assert_eq!(lanes, 2);
    for i in 0..91 {
        let k = o.pickup_calibration(i);
        let (x, y) = (buses[0][i], buses[1][i]);
        assert!((x - k * (0.75 * a[i] + 0.375 * b[i])).abs() < 1e-5, "x {i}");
        assert!((y - k * (0.375 * a[i] + 0.75 * b[i])).abs() < 1e-5, "y {i}");
    }
}

#[test]
fn exact_gains_put_copy_zero_on_both_lanes() {
    let mut o = settled(ScannerMode::V1);
    o.scanner_mode = ScannerMode::Off;
    // while the ramp runs both lanes are still produced; once settled one lane remains
    assert_eq!(o.render_tonewheel_lanes(1.0 / FS).1, 2);
    for _ in 0..SETTLE {
        o.render_tonewheel_lanes(1.0 / FS);
    }
    let (buses, lanes) = o.render_tonewheel_lanes(1.0 / FS);
    assert_eq!(lanes, 1);
    assert!(!o.tone_bank.copies());
    assert!(buses[0].iter().any(|v| v.abs() > 0.1));
}

#[test]
fn gains_ramp_instead_of_jumping() {
    let mut o = OrganLayer::new(FS);
    o.crosstalk_level = 0.0;
    o.scanner_mode = ScannerMode::C1;
    let diff = |o: &mut OrganLayer| {
        let (b, _) = o.render_tonewheel_lanes(1.0 / FS);
        b[0].iter()
            .zip(b[1].iter())
            .map(|(x, y)| (x - y).abs())
            .sum::<f32>()
    };
    let first = diff(&mut o);
    for _ in 0..SETTLE {
        diff(&mut o);
    }
    let last = diff(&mut o);
    // lane difference = 0.375 (a - b) at the end, about 0.25 * ramp progress after one frame
    assert!(first < 0.5 * last, "{first} vs {last}");
}

fn chord(o: &mut OrganLayer) {
    o.percussion_enabled = false;
    o.rotary_enabled = false;
    o.crosstalk_level = 0.0;
    o.note_on(60);
    o.note_on(67);
}

#[test]
fn mono_sum_equals_the_manual_mono_mean() {
    let mut fw = OrganLayer::new(FS);
    fw.detuned_copies = true;
    let mut manual = OrganLayer::new(FS);
    manual.detuned_copies = true;
    manual.vibrato_link = VibratoLink::Manual;
    for _ in 0..SETTLE {
        fw.render_tonewheel_lanes(1.0 / FS);
        manual.render_tonewheel_lanes(1.0 / FS);
    }
    chord(&mut fw);
    chord(&mut manual);
    let mut stereo_diff = 0.0f32;
    for n in 0..2000 {
        let [l, r] = fw.render_frame();
        let [m, m2] = manual.render_frame();
        assert_eq!(m, m2);
        assert!(((l + r) * 0.5 - m).abs() < 1e-5, "{n}: {l} {r} {m}");
        stereo_diff += (l - r).abs();
    }
    assert!(stereo_diff > 0.1, "lanes must differ: {stereo_diff}");
}

#[test]
fn exact_table_stays_dual_mono_without_rotary() {
    let mut o = OrganLayer::new(FS);
    chord(&mut o);
    for _ in 0..1000 {
        let [l, r] = o.render_frame();
        assert_eq!(l, r);
    }
}

#[test]
fn smoothing_follows_the_sample_rate() {
    let mut o = OrganLayer::new(96_000.0);
    o.scanner_mode = ScannerMode::C3;
    o.render_tonewheel_lanes(1.0 / 96_000.0);
    assert!((f64::from(o.lane_mixer.keep()) - 0.99f64.powf(43_156.4 / 96_000.0)).abs() < 1e-6);
}
