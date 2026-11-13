//! Gate, Pattern, KB Hold, KB Sync and Master Clock sync, exercised through `render_frame`.

use super::test_util::*;
use super::*;

const FS: f32 = 48_000.0;

fn layer() -> SynthLayer {
    let mut s = SynthLayer::new(FS);
    s.attack_sec = 0.001;
    s.sustain_level = 1.0;
    s.release_sec = 0.05;
    s
}

fn window_energy(frames: &[[f32; 2]], from: usize, to: usize) -> f32 {
    energy(&frames[from..to])
}

#[test]
fn gate_alternates_sound_and_silence_at_the_step_period() {
    let mut s = layer();
    s.arp_mode = ArpMode::Gate;
    s.arp_rate_bpm = 240.0; // step = 60 / (240 * 4) = 62.5 ms = 3000 samples
    s.perf.gate_env = 1.0;
    s.note_on(57, 110);
    let frames = render_n(&mut s, 3000 * 8);
    let step = 3000;
    // Skip the first step (attack); every later step is open in its first half, shut in its second.
    for k in 1..7 {
        let base = k * step;
        let open = window_energy(&frames, base + 300, base + 1300);
        let shut = window_energy(&frames, base + 1800, base + 2900);
        assert!(open > 0.05, "step {k}: open half is silent ({open})");
        assert!(
            shut < open * 1e-3,
            "step {k}: shut half sounds ({shut} vs {open})"
        );
    }
}

#[test]
fn gate_envelope_hardness_controls_the_edge() {
    let min_level = |env: f32| {
        let mut s = layer();
        s.arp_mode = ArpMode::Gate;
        s.arp_rate_bpm = 240.0;
        s.perf.gate_env = env;
        s.note_on(57, 110);
        let mut lo = 1.0f32;
        for i in 0..3000 * 4 {
            s.render_frame();
            if i > 3000 + 2400 {
                lo = lo.min(s.perf.gate_level);
            }
        }
        lo
    };
    let hard = min_level(1.0);
    let soft = min_level(0.0);
    assert!(hard < 0.01, "hard gate must close fully, got {hard}");
    assert!(soft > 0.05, "soft gate must stay smooth, got {soft}");
}

#[test]
fn gate_mode_plays_notes_normally_and_resets_level_when_left() {
    let mut s = layer();
    s.arp_mode = ArpMode::Gate;
    s.note_on(60, 100);
    assert!(s.voices.iter().any(|v| v.active && v.gate && v.note == 60));
    for _ in 0..4000 {
        s.render_frame();
    }
    s.arp_mode = ArpMode::Off;
    s.render_frame();
    assert_eq!(s.perf.gate_level, 1.0);
}

#[test]
fn gate_pattern_rests_follow_the_preset() {
    let mut s = layer();
    s.arp_mode = ArpMode::Gate;
    s.arp_rate_bpm = 240.0;
    s.perf.gate_env = 1.0;
    s.perf.pattern_on = true;
    s.perf.pattern_preset = 1; // [rest, 4, rest, 4]
    s.note_on(57, 110);
    let frames = render_n(&mut s, 3000 * 9);
    // Step 0 uses the initial length; steps 1.. follow the pattern from its first entry (a rest).
    for k in [1usize, 3, 5, 7] {
        let first_half = window_energy(&frames, k * 3000 + 300, k * 3000 + 1300);
        assert!(first_half < 1e-3, "step {k} must be a rest ({first_half})");
    }
    for k in [2usize, 4, 6, 8] {
        let first_half = window_energy(&frames, k * 3000 + 300, k * 3000 + 1300);
        assert!(first_half > 0.05, "step {k} must sound ({first_half})");
    }
}

#[test]
fn arp_pattern_skips_rest_steps() {
    let mut s = layer();
    s.arp_mode = ArpMode::Up;
    s.arp_rate_bpm = 240.0;
    s.perf.pattern_on = true;
    s.perf.pattern_preset = 1; // [rest, 4, rest, 4]
    s.note_on(60, 100);
    s.note_on(64, 100);
    let mut edges = 0;
    let mut was = false;
    for _ in 0..3000 * 8 + 10 {
        s.render_frame();
        let now = s.arp_active_notes[0].is_some();
        if now && !was {
            edges += 1;
        }
        was = now;
    }
    assert_eq!(edges, 4, "8 steps of [rest, note] play 4 notes");
}

#[test]
fn kb_hold_keeps_sounding_after_note_off() {
    let tail = |hold: bool| {
        let mut s = layer();
        s.perf.kb_hold = hold;
        s.note_on(60, 100);
        render_n(&mut s, 2000);
        s.note_off(60);
        let frames = render_n(&mut s, 24_000);
        energy(&frames[20_000..])
    };
    assert!(tail(false) < 1e-6, "without hold the note must have died");
    assert!(tail(true) > 0.1, "with hold the note must keep sustaining");
}

#[test]
fn kb_hold_exclude_leaves_the_layer_unheld() {
    let mut s = layer();
    s.perf.kb_hold = true;
    s.perf.kb_hold_exclude = true;
    s.note_on(60, 100);
    s.note_off(60);
    let frames = render_n(&mut s, 24_000);
    assert!(energy(&frames[20_000..]) < 1e-6);
}

#[test]
fn kb_hold_is_replaced_by_a_new_chord_and_ends_when_switched_off() {
    let mut s = layer();
    s.perf.kb_hold = true;
    s.note_on(60, 100);
    s.note_off(60);
    render_n(&mut s, 1000);
    assert!(s.voices.iter().any(|v| v.active && v.gate && v.note == 60));
    s.note_on(67, 100);
    assert!(
        !s.voices.iter().any(|v| v.active && v.gate && v.note == 60),
        "the new chord replaces the held one"
    );
    assert!(s.voices.iter().any(|v| v.active && v.gate && v.note == 67));
    s.note_off(67);
    s.perf.kb_hold = false;
    s.render_frame();
    assert!(!s.voices.iter().any(|v| v.active && v.gate));
}

#[test]
fn kb_hold_keeps_the_arpeggio_running() {
    let mut s = layer();
    s.perf.kb_hold = true;
    s.arp_mode = ArpMode::Up;
    s.arp_rate_bpm = 240.0;
    s.note_on(60, 100);
    s.note_on(64, 100);
    s.note_off(60);
    s.note_off(64);
    let before = s.arp_step;
    render_n(&mut s, 3000 * 3 + 10);
    assert_ne!(s.arp_step, before, "arp must keep stepping on held notes");
    assert!(s.arp_active_notes[0].is_some());
}

#[test]
fn synced_arp_step_equals_the_clock_subdivision() {
    for (division, bpm, expect) in [
        (ClockDivision::Quarter, 90.0f32, 32_000usize),
        (ClockDivision::Eighth, 120.0, 12_000),
        (ClockDivision::SixteenthT, 150.0, 3_200),
    ] {
        let mut s = layer();
        s.arp_mode = ArpMode::Up;
        s.perf.arp_clock_sync = true;
        s.perf.arp_division = division;
        s.perf.master_bpm = bpm;
        s.note_on(60, 100);
        s.note_on(64, 100);
        let mut at = Vec::new();
        let mut last = s.arp_step;
        for i in 0..expect * 4 {
            s.render_frame();
            if s.arp_step != last {
                at.push(i);
                last = s.arp_step;
            }
        }
        assert!(at.len() >= 3, "{division:?}: too few steps");
        for w in at.windows(2) {
            let d = w[1] - w[0];
            // The f32 step counter drifts by a few samples per long step.
            assert!(
                d.abs_diff(expect) <= expect / 500 + 1,
                "{division:?}: step {d} != {expect}"
            );
        }
    }
}

#[test]
fn synced_lfo_period_is_the_subdivision() {
    let mut s = layer();
    s.lfo_rate_hz = 7.0;
    s.perf.lfo_clock_sync = true;
    s.perf.lfo_division = ClockDivision::Quarter;
    s.perf.master_bpm = 120.0;
    assert!((s.lfo_hz() - 2.0).abs() < 1e-6);
    render_n(&mut s, 12_000); // 0.25 s at 2 Hz is half a cycle
    assert!((s.lfo_phase - PI).abs() < 0.01, "phase {}", s.lfo_phase);
    s.perf.lfo_clock_sync = false;
    assert_eq!(s.lfo_hz(), 7.0);
}

#[test]
fn kb_sync_restarts_the_arp_on_the_first_new_note_only() {
    let first_step_delay = |sync: bool| {
        let mut s = layer();
        s.arp_mode = ArpMode::Up;
        s.arp_rate_bpm = 240.0;
        s.perf.kb_sync = sync;
        s.note_on(60, 100);
        render_n(&mut s, 1500); // mid-step
        s.note_off(60);
        render_n(&mut s, 100);
        s.note_on(64, 100);
        for i in 0..4000 {
            s.render_frame();
            if s.arp_active_notes[0] == Some(64) {
                return i;
            }
        }
        usize::MAX
    };
    assert!(
        first_step_delay(true) <= 2,
        "synced arp fires on the new note"
    );
    assert!(first_step_delay(false) > 500, "free arp stays on its grid");
}

#[test]
fn kb_sync_does_not_reset_while_a_key_is_still_down() {
    let mut s = layer();
    s.arp_mode = ArpMode::Up;
    s.arp_rate_bpm = 240.0;
    s.perf.kb_sync = true;
    s.note_on(60, 100);
    render_n(&mut s, 1500);
    let counter = s.arp_sample_counter;
    s.note_on(64, 100);
    assert_eq!(s.arp_sample_counter, counter);
}

#[test]
fn engine_feeds_the_master_tempo_and_applies_kb_sync_clock_reset() {
    let mut e = crate::engine::Stage4Engine::new(FS);
    e.master_clock.bpm = 90.0;
    e.synth_a.perf.kb_sync = true;
    e.synth_a.perf.arp_clock_sync = true;
    e.synth_a.arp_mode = ArpMode::Up;
    e.render_frame();
    assert_eq!(e.synth_a.perf.master_bpm, 90.0);
    assert_eq!(e.synth_c.perf.master_bpm, 90.0);
    e.master_clock.beat_phase = 0.5;
    e.synth_a.note_on(60, 100);
    e.render_frame();
    assert!(e.master_clock.beat_phase < 0.01);
}

#[test]
fn defaults_render_like_a_layer_without_the_perf_features() {
    let mut a = layer();
    let mut b = layer();
    b.perf = PerfState::default();
    a.note_on(60, 100);
    b.note_on(60, 100);
    a.note_off(60);
    b.note_off(60);
    assert_eq!(render_n(&mut a, 6000), render_n(&mut b, 6000));
}
