//! Line-by-line check of the product Pipe2 layer against the PNO core 1 oracle (bank `0x122620`
//! plus mix stage `0x12268F`) on three chords.
//!
//! The oracle amplitudes are the `oracle` column of `python tools/organ_oracle_audio.py compare`
//! (`sharc-organ` render of each chord, 16,384 frames, first 2,048 dropped, exact table, level 1)
//! as committed in `docs/research/pipe-video-gap.md`. Each amplitude is the weight times the
//! quadrature sum with the -19 dB second harmonic and weaker ones of lower wheels, so the check
//! also covers the x table. The Rust side is measured with a Hann-windowed Goertzel line at the
//! wheel frequency.

use super::{wheel_hz, OrganLayer, OrganModel, WeightMode, FIRMWARE_FRAME_RATE_HZ};
use std::f64::consts::TAU;

const SETTLE: usize = 4096;
const FRAMES: usize = 16_384;
/// Allowed deviation of every line (after one common gain) from the oracle, in dB.
const TOL_DB: f64 = 0.05;

struct Chord {
    notes: [u8; 3],
    drawbars: [u8; 9],
    /// `(wheel, oracle amplitude)` of every line the oracle report lists.
    lines: &'static [(usize, f64)],
}

/// C4 E4 G4 with 16', 8' and 5 1/3' (`oracle-audio` section 4).
const MID: Chord = Chord {
    notes: [60, 64, 67],
    drawbars: [8, 8, 8, 0, 0, 0, 0, 0, 0],
    lines: &[
        (24, 1.0000),
        (28, 1.0001),
        (31, 1.0002),
        (36, 1.0060),
        (40, 1.0062),
        (43, 2.0125),
        (47, 1.0106),
        (50, 1.0120),
    ],
};

/// C2 G2 C3 with all nine stops: lowest wheels, up to three stops folded on one wheel.
const LOW: Chord = Chord {
    notes: [36, 43, 48],
    drawbars: [8; 9],
    lines: &[
        (0, 0.9978),
        (7, 0.9941),
        (12, 2.0009),
        (19, 2.0060),
        (24, 2.0113),
        (26, 1.0004),
        (31, 3.0208),
        (36, 2.0133),
        (38, 1.0227),
        (40, 1.0017),
        (43, 3.0404),
        (47, 1.0020),
        (48, 2.0141),
        (50, 1.0461),
        (52, 1.0059),
        (55, 2.0598),
        (60, 1.0275),
    ],
};

/// C6 E6 G6 with all nine stops: octave foldback at the top of the 0..90 wheel range.
const HIGH: Chord = Chord {
    notes: [84, 88, 91],
    drawbars: [8; 9],
    lines: &[
        (48, 1.0000),
        (52, 1.0000),
        (55, 1.0000),
        (60, 1.0062),
        (64, 1.0064),
        (67, 2.0113),
        (71, 1.0016),
        (72, 1.0069),
        (74, 0.9962),
        (76, 1.0069),
        (79, 4.9992),
        (80, 1.0000),
        (83, 3.0013),
        (84, 2.0032),
        (86, 2.0045),
        (88, 3.0022),
    ],
};

/// Hann-windowed amplitude of the line at `hz` over the last `FRAMES` samples of `x`.
fn line_amplitude(x: &[f32], hz: f64, fs: f64) -> f64 {
    let n = x.len();
    let (mut re, mut im, mut wsum) = (0.0f64, 0.0f64, 0.0f64);
    for (i, &v) in x.iter().enumerate() {
        let w = 0.5 - 0.5 * (TAU * i as f64 / n as f64).cos();
        let ph = TAU * hz * i as f64 / fs;
        re += w * f64::from(v) * ph.cos();
        im -= w * f64::from(v) * ph.sin();
        wsum += w;
    }
    2.0 * (re * re + im * im).sqrt() / wsum
}

/// Mono output of a Pipe2 layer with the rotary, key click and percussion off.
fn render(chord: &Chord, mode: WeightMode) -> Vec<f32> {
    let mut o = OrganLayer::new(FIRMWARE_FRAME_RATE_HZ as f32);
    o.model = OrganModel::Pipe2;
    o.weight_mode = mode;
    o.output_level = super::OutputLevel::Legacy; // the test compares the two weight modes at their own scales
    o.drawbars = chord.drawbars;
    o.percussion_enabled = false;
    o.rotary_enabled = false;
    o.key_click_enabled = false;
    for n in chord.notes {
        o.note_on(n);
    }
    let all: Vec<f32> = (0..SETTLE + FRAMES)
        .map(|_| {
            let [l, r] = o.render_frame();
            assert_eq!(l, r, "exact table: dual mono");
            l
        })
        .collect();
    all[SETTLE..].to_vec()
}

/// Spread (max - min, dB) of `20 log10(rust / oracle)` over the lines, and the common gain (dB).
fn line_errors(chord: &Chord, mode: WeightMode) -> (f64, f64) {
    let x = render(chord, mode);
    let db: Vec<f64> = chord
        .lines
        .iter()
        .map(|&(w, amp)| {
            let a = line_amplitude(&x, wheel_hz(w, 0.0), FIRMWARE_FRAME_RATE_HZ);
            20.0 * (a / amp).log10()
        })
        .collect();
    let (lo, hi) = db
        .iter()
        .fold((f64::MAX, f64::MIN), |(l, h), &v| (l.min(v), h.max(v)));
    (hi - lo, 0.5 * (lo + hi))
}

#[test]
fn pipe_lines_match_the_oracle_in_both_weight_modes() {
    for chord in [&MID, &LOW, &HIGH] {
        for mode in [WeightMode::Legacy, WeightMode::Firmware] {
            let (spread, _) = line_errors(chord, mode);
            assert!(
                spread < 2.0 * TOL_DB,
                "{:?} {mode:?}: line spread {spread} dB",
                chord.notes
            );
        }
    }
}

#[test]
fn legacy_bus_scale_is_the_assumed_gap_to_the_firmware_scale() {
    // Legacy: 0.08 x 0.92 per full stop; Firmware: the ARM weight 127 * 1008 * 2^-23 x 0.92.
    // The ratio is the ASSUMED legacy scale against the VERIFIED weight unit (level L open).
    let (_, legacy) = line_errors(&MID, WeightMode::Legacy);
    let (_, firmware) = line_errors(&MID, WeightMode::Firmware);
    let ratio_db = legacy - firmware;
    let want = 20.0 * (0.08f64 / f64::from(super::UNIT_WEIGHT)).log10();
    assert!((ratio_db - want).abs() < 0.05, "{ratio_db} vs {want}");
    assert!((want - 14.4).abs() < 0.1, "{want}");
}
