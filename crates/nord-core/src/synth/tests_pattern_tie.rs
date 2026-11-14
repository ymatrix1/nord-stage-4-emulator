//! `Q` gate ties and preset dial value 0 of the Arpeggiator pattern, exercised through
//! `SynthLayer::render_frame`.
//!
//! The rules come from the firmware pattern compiler (`docs/research/synth-arp-patterns.md`
//! section 7): a `Q` step is held into the next step, which does not trigger and ends with its own
//! gate; `E` lasts half a step, `q` a full step.

use super::*;

const FS: f32 = 48_000.0;
/// One step at 240 BPM: 60 / (240 * 4) s = 3000 samples at 48 kHz.
const STEP: usize = 3000;

fn layer(gates: &[u8], ties: &[bool]) -> SynthLayer {
    let mut s = SynthLayer::new(FS);
    s.attack_sec = 0.001;
    s.sustain_level = 1.0;
    s.release_sec = 0.02;
    s.arp_rate_bpm = 240.0;
    s.arp_mode = ArpMode::Up;
    s.perf.pattern_on = true;
    s.perf.pattern_custom = true;
    s.set_pattern_length(gates.len() as u8);
    for (i, &g) in gates.iter().enumerate() {
        s.set_step_gate(i, g);
        s.set_step_tie(i, ties.get(i).copied().unwrap_or(false));
    }
    s
}

/// Lengths (in samples) of the runs during which an arp note is active, over `steps` steps.
fn note_runs(s: &mut SynthLayer, steps: usize) -> Vec<usize> {
    let mut runs = Vec::new();
    let mut run = 0usize;
    for _ in 0..STEP * steps {
        s.render_frame();
        if s.arp_active_notes[0].is_some() {
            run += 1;
        } else if run > 0 {
            runs.push(run);
            run = 0;
        }
    }
    if run > 0 {
        runs.push(run);
    }
    runs
}

fn near(got: usize, want: usize) -> bool {
    got.abs_diff(want) <= 40
}

#[test]
fn q_ties_into_the_next_step_and_ends_with_its_gate() {
    // Pattern [Q, E, ., .] = gates 8 (tie), 4, 0, 0: one note of 8 + 4 eighths = 1.5 steps.
    let mut s = layer(&[8, 4, 0, 0], &[true]);
    s.note_on(60, 100);
    let runs = note_runs(&mut s, 8);
    assert_eq!(runs.len(), 2, "one note per period: {runs:?}");
    assert!(runs.iter().all(|&r| near(r, STEP * 3 / 2)), "{runs:?}");
}

/// Note chosen at every arpeggiator step change (a trigger) over `steps` steps.
fn triggers(s: &mut SynthLayer, steps: usize) -> Vec<Option<u8>> {
    let mut out = Vec::new();
    let mut last = s.arp_step;
    for _ in 0..STEP * steps {
        s.render_frame();
        if s.arp_step != last {
            last = s.arp_step;
            out.push(s.arp_active_notes[0]);
        }
    }
    out
}

fn two_notes(gates: &[u8], ties: &[bool]) -> SynthLayer {
    let mut s = layer(gates, ties);
    s.note_on(60, 100);
    s.note_on(64, 100);
    s
}

#[test]
fn tied_step_does_not_retrigger_or_advance_the_arpeggio() {
    // Two held notes, Up. Gates [8, 4, 4, 4]: without the tie every step triggers; with the first
    // step tied (Q E E E) the second step continues the note and the arpeggio does not advance.
    let plain = triggers(&mut two_notes(&[8, 4, 4, 4], &[]), 8);
    let tied = triggers(&mut two_notes(&[8, 4, 4, 4], &[true]), 8);
    assert_eq!(plain.len(), 8 - 1, "{plain:?}");
    assert_eq!(tied.len(), 6 - 1, "{tied:?}");
    // The notes alternate 60, 64 on every trigger, with no extra advance at the continuation.
    assert!(
        tied.windows(2).all(|w| w[0] != w[1]),
        "arpeggio skipped a note: {tied:?}"
    );
}

#[test]
fn q_before_a_rest_lasts_one_full_step() {
    let mut s = layer(&[8, 0], &[true]);
    s.note_on(60, 100);
    let runs = note_runs(&mut s, 6);
    assert!(runs.iter().all(|&r| near(r, STEP)), "{runs:?}");
}

#[test]
fn q_chains_through_following_steps() {
    // Q Q E = 8 + 8 + 4 eighths in one note, then a rest to fill the period.
    let mut s = layer(&[8, 8, 4, 0], &[true, true]);
    s.note_on(60, 100);
    let runs = note_runs(&mut s, 8);
    assert_eq!(runs.len(), 2, "{runs:?}");
    assert!(runs.iter().all(|&r| near(r, STEP * 5 / 2)), "{runs:?}");
}

#[test]
fn a_q_on_the_last_step_does_not_tie_across_the_pattern_end() {
    // [E, Q]: the tie of the last step is ignored (the compiler starts every cycle with a fresh
    // note), so every step triggers exactly as in the same pattern without the tie.
    let tied = triggers(&mut two_notes(&[4, 8], &[false, true]), 8);
    let plain = triggers(&mut two_notes(&[4, 8], &[]), 8);
    assert_eq!(tied, plain);
}

#[test]
fn dial_zero_plays_the_custom_pattern_and_a_preset_replaces_it() {
    // Preset dial 0 (default) plays `custom`: gates [8, 0] = one full-step note per two steps.
    let mut s = layer(&[8, 0], &[]);
    s.perf.pattern_custom = false;
    s.select_pattern_preset(0);
    s.note_on(60, 100);
    let runs = note_runs(&mut s, 6);
    assert_eq!(runs.len(), 3, "{runs:?}");

    // Dial on preset 3 (EEQEEQ..., no rests) plays the table, not the custom pattern.
    let mut s = layer(&[8, 0], &[]);
    s.select_pattern_preset(3);
    s.note_on(60, 100);
    let runs = note_runs(&mut s, 6);
    assert!(runs.len() >= 4, "{runs:?}");
}
