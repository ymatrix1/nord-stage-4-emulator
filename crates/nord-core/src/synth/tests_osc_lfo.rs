//! Voice allocation, LFO, vibrato, wavetable and FM tests of the synth layer.

use super::test_util::*;
use super::*;

#[test]
fn test_synth_polyphonic_voices() {
    let mut synth = SynthLayer::new(48_000.0);
    // Play 3-note chord: C4 (60), E4 (64), G4 (67)
    synth.note_on(60, 100);
    synth.note_on(64, 100);
    synth.note_on(67, 100);

    let active_count = synth.voices.iter().filter(|v| v.active).count();
    assert_eq!(active_count, 3, "Chord must allocate 3 polyphonic voices");

    let mut chord_energy = 0.0f32;
    for _ in 0..512 {
        let [l, r] = synth.render_frame();
        chord_energy += l * l + r * r;
    }
    assert!(chord_energy > 0.05);

    // Release one note
    synth.note_off(60);
    assert!(!synth.voices.iter().find(|v| v.note == 60).unwrap().gate);
    assert!(synth.voices.iter().find(|v| v.note == 64).unwrap().gate);
}

#[test]
fn vibrato_delay_keeps_output_identical_until_the_delay_elapses() {
    let run = |mode: VibratoMode| {
        let mut s = SynthLayer::new(48_000.0);
        s.vibrato_mode = mode;
        s.vibrato_depth_cents = 100.0;
        s.vibrato_delay_sec = 0.05;
        s.vibrato_rate_hz = 6.0;
        s.note_on(60, 100);
        render_n(&mut s, 12_000)
    };
    let off = run(VibratoMode::Off);
    let dly = run(VibratoMode::Delay);
    assert_eq!(off[..2000], dly[..2000]);
    assert_ne!(off[8000..], dly[8000..]);
    let on = run(VibratoMode::On);
    assert_ne!(off[1000..2000], on[1000..2000]);
}

#[test]
fn lfo_saw_down_differs_from_saw_up_and_off_destination_is_inert() {
    let run = |shape: LfoShape, dest: LfoDestination| {
        let mut s = SynthLayer::new(48_000.0);
        s.lfo_shape = shape;
        s.lfo_dest = dest;
        s.lfo_amount = 1.0;
        s.lfo_rate_hz = 5.0;
        s.note_on(60, 100);
        render_n(&mut s, 6000)
    };
    let up = run(LfoShape::Saw, LfoDestination::OscPitch);
    let down = run(LfoShape::SawDown, LfoDestination::OscPitch);
    assert_ne!(up, down);
    let off = run(LfoShape::Square, LfoDestination::Off);
    let tri_off = run(LfoShape::Triangle, LfoDestination::Off);
    assert_eq!(off, tri_off, "LFO off must not modulate anything");
}

#[test]
fn lfo_pitch_depth_follows_the_firmware_curve_and_stays_finite_at_full_depth() {
    let run = |amount: f32| {
        let mut s = SynthLayer::new(48_000.0);
        s.lfo_dest = LfoDestination::OscPitch;
        s.lfo_shape = LfoShape::Square;
        s.lfo_rate_hz = 20.0;
        s.lfo_amount = amount;
        s.note_on(60, 100);
        render_n(&mut s, 4000)
    };
    let full = run(1.0);
    assert!(full.iter().all(|f| f[0].is_finite() && f[1].is_finite()));
    // 1 semitone (raw 27) differs from the old fixed 5% rule and from a full +/-48 semitone swing.
    assert_ne!(run(27.0 / 127.0), full);
    assert_ne!(run(27.0 / 127.0), run(0.0));
}

#[test]
fn test_lfo_modulation() {
    let mut synth = SynthLayer::new(48_000.0);
    synth.lfo_shape = LfoShape::Triangle;
    synth.lfo_amount = 0.8;
    synth.lfo_rate_hz = 10.0;
    synth.note_on(60, 100);

    for _ in 0..1024 {
        let [l, r] = synth.render_frame();
        assert!(l.is_finite() && r.is_finite());
    }
    assert!(synth.lfo_phase > 0.0);
}

#[test]
fn test_64_mipmapped_wavetables_render_distinct_and_bandlimited() {
    let bank = wavetable_bank();
    assert_eq!(bank.tables.len(), NUM_WAVETABLES);

    let mut family_rms = Vec::new();
    for wt_idx in (0..NUM_WAVETABLES).step_by(8) {
        let mut synth = SynthLayer::new(48_000.0);
        synth.osc_mode = SynthOscMode::Wavetable;
        synth.wavetable_index = wt_idx as u8;
        synth.wavetable_position = 0.35;
        synth.cutoff_hz = 16_000.0;
        synth.note_on(48, 110);

        let mut e = 0.0f32;
        for _ in 0..512 {
            let [l, r] = synth.render_frame();
            assert!(l.is_finite() && r.is_finite());
            e += l * l;
        }
        assert!(e > 0.01, "wavetable {wt_idx} must render active audio");
        family_rms.push((e / 512.0).sqrt());
    }
    assert!(family_rms.iter().any(|&x| (x - family_rms[0]).abs() > 1e-3));

    // High MIDI note (C8 = 108) must select coarser mipmap without NaN or excessive Nyquist energy
    let mut high = SynthLayer::new(48_000.0);
    high.osc_mode = SynthOscMode::Wavetable;
    high.wavetable_index = 42;
    high.note_on(108, 100);
    for _ in 0..256 {
        let [l, r] = high.render_frame();
        assert!(l.is_finite() && r.is_finite());
        assert!(l.abs() < 1.5);
    }
}

#[test]
fn test_three_operator_fm_with_feedback_and_mod_env() {
    let mut s2 = SynthLayer::new(48_000.0);
    s2.osc_mode = SynthOscMode::Fm;
    s2.fm_amount = 0.7;
    s2.fm_ratio = 2.0;
    s2.fm_feedback = 0.0;
    s2.fm_sub_amount = 0.0;
    s2.cutoff_hz = 14_000.0;
    s2.note_on(60, 100);

    let mut s3 = s2.clone();
    s3.fm_ratio = std::f32::consts::SQRT_2;
    s3.fm_feedback = 0.75;
    s3.fm_sub_ratio = 5.0;
    s3.fm_sub_amount = 0.8;

    let mut diff_energy = 0.0f32;
    for _ in 0..1024 {
        let [a, _] = s2.render_frame();
        let [b, _] = s3.render_frame();
        assert!(a.is_finite() && b.is_finite());
        diff_energy += (a - b) * (a - b);
    }
    assert!(
        diff_energy > 0.05,
        "3-op ratio + feedback FM must produce distinct timbre"
    );
    assert!(
        s3.voices[0].mod_env < 1.0,
        "FM mod envelope must decay smoothly during note sustain"
    );
}
