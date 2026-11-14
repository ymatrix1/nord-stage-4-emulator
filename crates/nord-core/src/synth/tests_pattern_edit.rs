//! Custom pattern (gate / accent / pan), ZIG ZAG, fractional RANGE, exercised through
//! `SynthLayer::render_frame`.

use super::test_util::*;
use super::*;

const FS: f32 = 48_000.0;
/// One step at 240 BPM: 60 / (240 * 4) s = 3000 samples at 48 kHz.
const STEP: usize = 3000;

fn layer() -> SynthLayer {
    let mut s = SynthLayer::new(FS);
    s.attack_sec = 0.001;
    s.sustain_level = 1.0;
    s.release_sec = 0.02;
    s.arp_rate_bpm = 240.0;
    s
}

/// Active arp note at every arpeggiator step change.
fn step_notes(s: &mut SynthLayer, steps: usize) -> Vec<Option<u8>> {
    let mut out = Vec::new();
    let mut last = s.arp_step;
    let mut changes = 0;
    while changes < steps {
        s.render_frame();
        if s.arp_step != last {
            last = s.arp_step;
            out.push(s.arp_active_notes[0]);
            changes += 1;
        }
    }
    out
}

fn side_energy(frames: &[[f32; 2]]) -> (f32, f32) {
    (
        frames.iter().map(|f| f[0] * f[0]).sum(),
        frames.iter().map(|f| f[1] * f[1]).sum(),
    )
}

fn custom_arp(len: u8) -> SynthLayer {
    let mut s = layer();
    s.arp_mode = ArpMode::Up;
    s.perf.pattern_on = true;
    s.perf.pattern_custom = true;
    s.set_pattern_length(len);
    s
}

#[test]
fn custom_pattern_repeats_with_its_length_and_rests() {
    let mut s = custom_arp(3);
    for (i, g) in [4u8, 0, 4].into_iter().enumerate() {
        s.set_step_gate(i, g);
    }
    s.note_on(60, 100);
    let mut edges = 0;
    let mut was = false;
    for _ in 0..STEP * 12 + 10 {
        s.render_frame();
        let now = s.arp_active_notes[0].is_some();
        if now && !was {
            edges += 1;
        }
        was = now;
    }
    // 12 steps = 4 periods of [note, rest, note].
    assert_eq!(edges, 8);
}

#[test]
fn custom_gate_length_sets_the_note_duration() {
    let sounding = |eighths: u8| {
        let mut s = custom_arp(2);
        s.set_step_gate(0, eighths);
        s.set_step_gate(1, eighths);
        s.note_on(60, 100);
        let frames = render_n(&mut s, STEP * 3);
        // Window in the second half of the second step.
        energy(&frames[STEP * 2 + 2000..STEP * 3 - 100])
    };
    assert!(
        sounding(8) > 0.05,
        "full-length gate sounds to the step end"
    );
    let (full, short) = (sounding(8), sounding(2));
    assert!(
        short < full * 0.1,
        "quarter-length gate is shut by then ({short} vs {full})"
    );
}

#[test]
fn accent_boosts_velocity_when_amp_velocity_is_active() {
    let mut s = custom_arp(2);
    s.amp_velocity = 1.0;
    s.set_step_gate(0, 8);
    s.set_step_gate(1, 8);
    s.set_step_accent(0, true);
    s.note_on(60, 60);
    let frames = render_n(&mut s, STEP * 3 + 100);
    let accented = energy(&frames[STEP + 500..STEP + 2500]);
    let plain = energy(&frames[STEP * 2 + 500..STEP * 2 + 2500]);
    assert!(accented > plain * 1.5, "accent {accented} vs plain {plain}");

    // Without a velocity-sensitive parameter the accent is inaudible (manual p.37 note).
    let mut s = custom_arp(2);
    s.amp_velocity = 0.0;
    s.set_step_gate(0, 8);
    s.set_step_gate(1, 8);
    s.set_step_accent(0, true);
    s.note_on(60, 60);
    let frames = render_n(&mut s, STEP * 3 + 100);
    let a = energy(&frames[STEP + 500..STEP + 2500]);
    let b = energy(&frames[STEP * 2 + 500..STEP * 2 + 2500]);
    assert!((a / b - 1.0).abs() < 0.2, "accent leaked: {a} vs {b}");
}

#[test]
fn pan_places_steps_left_and_right() {
    let mut s = custom_arp(2);
    s.set_step_gate(0, 8);
    s.set_step_gate(1, 8);
    s.set_step_pan(0, PatternPan::Left);
    s.set_step_pan(1, PatternPan::Right);
    s.note_on(60, 100);
    let frames = render_n(&mut s, STEP * 3 + 100);
    let (l0, r0) = side_energy(&frames[STEP + 500..STEP + 2500]);
    let (l1, r1) = side_energy(&frames[STEP * 2 + 500..STEP * 2 + 2500]);
    assert!(l0 > 0.05 && r0 < l0 * 1e-3, "step 0 left: {l0} {r0}");
    assert!(r1 > 0.05 && l1 < r1 * 1e-3, "step 1 right: {l1} {r1}");
}

#[test]
fn centre_pan_matches_an_unpanned_render_exactly() {
    let run = |custom: bool| {
        let mut s = layer();
        s.arp_mode = ArpMode::Up;
        s.perf.pattern_on = true;
        s.perf.pattern_preset = 3; // all steps centred
        if custom {
            s.customize_preset(3);
        }
        s.note_on(60, 100);
        render_n(&mut s, STEP * 6)
    };
    assert_eq!(run(false), run(true));
}

#[test]
fn zigzag_jumps_two_forward_and_one_back() {
    let mut s = layer();
    s.arp_mode = ArpMode::Up;
    s.perf.zigzag = true;
    for n in [60, 62, 64, 65, 67] {
        s.note_on(n, 100);
    }
    let notes = step_notes(&mut s, 8);
    // Active note after each tick is the one triggered by it; the counter has already advanced.
    let expect: Vec<Option<u8>> = [60, 64, 62, 65, 64, 67, 65, 60]
        .into_iter()
        .map(Some)
        .collect();
    assert_eq!(notes, expect);
}

#[test]
fn plain_up_is_unchanged_without_zigzag() {
    let mut s = layer();
    s.arp_mode = ArpMode::Up;
    for n in [60, 64, 67] {
        s.note_on(n, 100);
    }
    let notes = step_notes(&mut s, 4);
    let expect: Vec<Option<u8>> = [60, 64, 67, 60].into_iter().map(Some).collect();
    assert_eq!(notes, expect);
}

#[test]
fn fractional_range_adds_the_lowest_notes_of_one_more_octave() {
    let mut s = layer();
    s.arp_mode = ArpMode::Up;
    s.arp_octave_range = 1;
    s.perf.range_extra_thirds = 2;
    for n in [60, 64, 67] {
        s.note_on(n, 100);
    }
    // Chord span to 67 = 7 semitones <= 8: all three repeat an octave up.
    let notes = step_notes(&mut s, 7);
    let expect: Vec<Option<u8>> = [60, 64, 67, 72, 76, 79, 60].into_iter().map(Some).collect();
    assert_eq!(notes, expect);

    let mut s = layer();
    s.arp_mode = ArpMode::Up;
    s.arp_octave_range = 2;
    s.perf.range_extra_thirds = 1;
    for n in [60, 64, 67] {
        s.note_on(n, 100);
    }
    // 2 octaves plus the notes within 4 semitones of the root (60, 64) one octave higher.
    let notes = step_notes(&mut s, 9);
    let expect: Vec<Option<u8>> = [60, 64, 67, 72, 76, 79, 84, 88, 60]
        .into_iter()
        .map(Some)
        .collect();
    assert_eq!(notes, expect);
}

#[test]
fn fractional_range_down_reverses_the_pool() {
    let mut s = layer();
    s.arp_mode = ArpMode::Down;
    s.arp_octave_range = 1;
    s.perf.range_extra_thirds = 1;
    for n in [60, 64, 67] {
        s.note_on(n, 100);
    }
    let notes = step_notes(&mut s, 5);
    let expect: Vec<Option<u8>> = [76, 72, 67, 64, 60].into_iter().map(Some).collect();
    assert_eq!(notes, expect);
}

#[test]
fn defaults_leave_pattern_edit_features_off() {
    let s = SynthLayer::new(FS);
    assert!(!s.perf.zigzag && !s.perf.pattern_custom);
    assert_eq!(s.perf.range_extra_thirds, 0);
}
