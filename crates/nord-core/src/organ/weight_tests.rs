//! Tests of `WeightMode::Firmware` on the layer: the key to wheel map of the booted ARM code, the
//! oracle partial levels (derived vectors only), key velocity, release tail and mode switching.
//!
//! Evidence: `docs/research/organ-weights.md` sections 3 and 5, `docs/research/organ-oracle-audio.md`
//! section 4 (oracle partials of the chord C4 E4 G4, drawbars 888), `python
//! tools/organ_weight_compare.py` (level 1.0000x, attack and release envelopes within 0.1 %).

use super::weights::{wheel_of, UNIT_WEIGHT};
use super::{vibrato_bank::wheel_hz, OrganLayer, WeightMode};

/// Rate at which wheel 0 has the cycles per frame of the firmware increment (`organ_ref` default).
const FRAME_RATE: f32 = 43_156.4;

/// Layer with everything but the tonewheel drawbar path off.
fn layer(mode: WeightMode) -> OrganLayer {
    let mut o = OrganLayer::new(FRAME_RATE);
    o.weight_mode = mode;
    o.percussion_enabled = false;
    o.crosstalk_level = 0.0;
    o.rotary_enabled = false;
    o.key_click_enabled = false;
    o
}

/// Amplitude of the sinusoid of `hz` in `x` (Hann window, `fs` = [`FRAME_RATE`]).
fn amplitude(x: &[f32], hz: f64) -> f64 {
    let n = x.len();
    let (mut re, mut im, mut wsum) = (0.0f64, 0.0f64, 0.0f64);
    for (i, &v) in x.iter().enumerate() {
        let w = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos();
        let ph = std::f64::consts::TAU * hz * i as f64 / f64::from(FRAME_RATE);
        re += w * f64::from(v) * ph.cos();
        im += w * f64::from(v) * ph.sin();
        wsum += w;
    }
    2.0 * (re * re + im * im).sqrt() / wsum
}

fn render_left(o: &mut OrganLayer, frames: usize) -> Vec<f32> {
    (0..frames).map(|_| o.render_frame()[0]).collect()
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

/// `docs/research/data/organ-key-wheel-map.csv`: wheel and taper per drawbar of all 128 notes.
#[test]
fn key_wheel_map_matches_all_128_notes_and_9_drawbars() {
    let csv = include_str!("../../../../docs/research/data/organ-key-wheel-map.csv");
    let mut rows = 0;
    for line in csv.lines().skip(1).filter(|l| !l.is_empty()) {
        let f: Vec<usize> = line.split(',').map(|v| v.parse().unwrap()).collect();
        assert_eq!(f.len(), 19, "{line}");
        for j in 0..9 {
            assert_eq!(
                wheel_of(f[0] as i32, j),
                f[1 + j],
                "note {} drawbar {j}",
                f[0]
            );
            assert_eq!(f[10 + j], 127, "no tapering: note {} drawbar {j}", f[0]);
        }
        rows += 1;
    }
    assert_eq!(rows, 128);
}

/// Oracle partials (`organ-oracle-audio.md` section 4, unit weight): 1.0000 (wheel 24), 1.0002
/// (31), 2.0125 (43); the firmware weight of one key is `UNIT_WEIGHT`.
#[test]
fn chord_partials_are_the_firmware_weights_times_the_oracle_partials() {
    let mut o = layer(WeightMode::Firmware);
    for n in [60, 64, 67] {
        o.note_on(n);
    }
    let _ = render_left(&mut o, 16_384); // attack settles (a = 0.0096: 100 frames)
    let x = render_left(&mut o, 8192);
    let unit = f64::from(UNIT_WEIGHT);
    for (wheel, want) in [(24, 1.0000), (31, 1.0002), (43, 2.0125)] {
        let got = amplitude(&x, wheel_hz(wheel, 0.0)) / unit;
        assert!(
            (got - want).abs() < 0.02 * want,
            "wheel {wheel}: {got} vs {want}"
        );
    }
}

#[test]
fn velocity_scales_the_level_by_the_integer_shift_formula() {
    let level = |vel: u8| {
        let mut o = layer(WeightMode::Firmware);
        o.drawbars = [0, 0, 8, 0, 0, 0, 0, 0, 0];
        o.note_on_with_gain(60, f32::from(vel) / 127.0);
        let _ = render_left(&mut o, 16_384);
        rms(&render_left(&mut o, 8192))
    };
    // (64 * 127) >> 4 = 508 against (127 * 127) >> 4 = 1008
    let ratio = f64::from(level(64)) / f64::from(level(127));
    assert!((ratio - 508.0 / 1008.0).abs() < 2e-3, "{ratio}");
}

#[test]
fn release_decays_with_the_update_coefficient_and_ends_in_silence() {
    let mut o = layer(WeightMode::Firmware);
    o.drawbars = [0, 0, 8, 0, 0, 0, 0, 0, 0];
    o.note_on(60);
    let _ = render_left(&mut o, 16_384);
    let held = rms(&render_left(&mut o, 4096));
    o.note_off(60);
    // 1/a = 234 frames: after 1170 frames (5 time constants) the weight is 0.7 %
    let _ = render_left(&mut o, 1000);
    let tail = rms(&render_left(&mut o, 1000));
    assert!(tail > 0.0 && tail < 0.05 * held, "{tail} vs {held}");
    let _ = render_left(&mut o, 20_000);
    assert_eq!(o.render_frame(), [0.0, 0.0], "release tail ends exactly");
}

#[test]
fn attack_is_a_ramp_and_legacy_is_instant() {
    let early = |mode: WeightMode| {
        let mut o = layer(mode);
        o.drawbars = [0, 0, 8, 0, 0, 0, 0, 0, 0];
        o.note_on(60);
        let first = render_left(&mut o, 165); // about one period of wheel 36
        let later = render_left(&mut o, 4096 * 4);
        rms(&first) / rms(&later[later.len() - 4096..])
    };
    let (fw, legacy) = (early(WeightMode::Firmware), early(WeightMode::Legacy));
    assert!(legacy > 0.9, "instant gains: {legacy}");
    assert!(
        fw < 0.8 * legacy,
        "ramp over about 100 frames: {fw} vs {legacy}"
    );
}

#[test]
fn switching_modes_with_a_held_key_stays_finite_and_audible() {
    let mut o = layer(WeightMode::Legacy);
    o.note_on(48);
    let _ = render_left(&mut o, 2000);
    o.weight_mode = WeightMode::Firmware;
    let _ = render_left(&mut o, 40_000);
    let x = render_left(&mut o, 4096);
    assert!(x.iter().all(|v| v.is_finite()) && rms(&x) > 1e-3);
}

#[test]
fn percussion_and_default_mode_are_unchanged_by_default() {
    assert_eq!(OrganLayer::new(48_000.0).weight_mode, WeightMode::Legacy);
    let mut o = layer(WeightMode::Firmware);
    o.percussion_enabled = true;
    o.note_on(60);
    let x = render_left(&mut o, 4096);
    assert!(x.iter().all(|v| v.is_finite()) && rms(&x) > 0.0);
}
