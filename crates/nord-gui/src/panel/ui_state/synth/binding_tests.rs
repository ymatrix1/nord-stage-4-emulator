//! Proof for the `ui:synth.*` bindings of the control inventory (`ns4-lu50`): each Synth control
//! is pressed through `SynthUi::press` / `knob_input` (what the painted widget calls), the queued
//! edits are applied to a real engine and the engine state is asserted. A final test requires the
//! proven ids to be exactly the Synth controls bound to a `ui:` path.

use super::{Btn, Knob, SynthPage, SynthUi};
use nord_core::engine::Stage4Engine;
use nord_core::synth::SynthOscMode;
use nord_panel::behaviour::{PotTracker, Trigger};
use nord_panel::inventory::synth_dials::{DialTarget, DIAL_CELLS};
use nord_panel::{all_controls, Binding, Section};

/// Apply every queued edit of the panel to the engine.
fn apply(s: &mut SynthUi, e: &mut Stage4Engine) {
    for edit in s.take_edits() {
        edit.apply(e);
    }
}

/// The eight page buttons open their display page, and every dial that the inventory's dial table
/// maps to an edit on that page emits an engine edit.
fn page_buttons() -> Vec<&'static str> {
    let pages = [
        (Btn::Waveform, SynthPage::Waveform, "synth.waveform"),
        (Btn::PitchSmp, SynthPage::PitchSmp, "synth.osc.pitch_smp"),
        (Btn::OscEnvelope, SynthPage::EnvOsc, "synth.osc.envelope"),
        (Btn::FilterType, SynthPage::FilterType, "synth.filter.type"),
        (
            Btn::FilterEnvelope,
            SynthPage::EnvFilter,
            "synth.filter.envelope",
        ),
        (Btn::AmpEnvelope, SynthPage::EnvAmp, "synth.amp.envelope"),
        (Btn::VibratoMenu, SynthPage::Vibrato, "synth.vibrato.menu"),
        (Btn::ArpMenu, SynthPage::Arp(1), "synth.arp.menu"),
    ];
    let mut ids = Vec::new();
    for (btn, page, id) in pages {
        let mut s = SynthUi::new();
        s.layer_mut().arp_run = true;
        s.page = SynthPage::LfoWave; // any other page, so the press has to open this one
        s.press(btn, Trigger::Primary);
        assert_eq!(s.page, page, "{id} opens its page");
        assert_eq!(btn.id(), id);
        let key = page.dial_key(SynthOscMode::Analog);
        let edit_dials: Vec<usize> = DIAL_CELLS
            .iter()
            .filter(|c| c.page == key && matches!(c.target, DialTarget::Edit { .. }))
            .map(|c| usize::from(c.dial))
            .collect();
        assert!(!edit_dials.is_empty(), "{key} has no editing dial");
        for d in edit_dials {
            s.take_edits();
            s.turn_dial(d, 1, false);
            let mut got = s.take_edits();
            if got.is_empty() {
                s.turn_dial(d, -1, false); // a value already at its upper end
                got = s.take_edits();
            }
            assert!(!got.is_empty(), "{key}: dial {d} emits an edit");
        }
        ids.push(id);
    }
    ids
}

/// The three VELOCITY shifts reach the engine layer.
fn velocity_shifts() -> Vec<&'static str> {
    let mut e = Stage4Engine::new(48_000.0);
    let mut s = SynthUi::new();
    assert!(!e.synth_a.osc_env_velocity);
    s.press(Btn::OscEnvelope, Trigger::Shifted);
    apply(&mut s, &mut e);
    assert!(e.synth_a.osc_env_velocity, "OSC VELOCITY");
    let before = e.synth_a.filter_env_vel_amount;
    s.press(Btn::FilterEnvelope, Trigger::Shifted);
    apply(&mut s, &mut e);
    assert_eq!(e.synth_a.filter_env_vel_amount, 1.0, "was {before}");
    // AMP VELOCITY steps through four sensitivities: 0, 1/3, 2/3, 1.
    let mut seen = Vec::new();
    for _ in 0..4 {
        s.press(Btn::AmpEnvelope, Trigger::Shifted);
        apply(&mut s, &mut e);
        assert!(
            (e.synth_a.amp_velocity - f32::from(s.layer().amp_vel) / 3.0).abs() < 1e-6,
            "AMP VELOCITY follows the panel step"
        );
        seen.push(s.layer().amp_vel);
    }
    seen.sort_unstable();
    assert_eq!(seen, [0, 1, 2, 3]);
    vec![
        "synth.osc.velocity",
        "synth.filter.velocity",
        "synth.amp.velocity",
    ]
}

/// GROUP (Shift+Type / Waveform / Menu) copies the focused layer's group to the other layers.
fn groups() -> Vec<&'static str> {
    let mut e = Stage4Engine::new(48_000.0);
    let mut s = SynthUi::new();
    e.synth_a.cutoff_hz = 812.0;
    e.synth_a.lfo_rate_hz = 3.3;
    e.synth_a.arp_rate_bpm = 77.0;
    s.press(Btn::FilterType, Trigger::Shifted);
    s.press(Btn::LfoWaveform, Trigger::Shifted);
    s.press(Btn::ArpMenu, Trigger::Shifted);
    apply(&mut s, &mut e);
    for l in [&e.synth_b, &e.synth_c] {
        assert_eq!(l.cutoff_hz, 812.0, "FILTER GROUP");
        assert_eq!(l.lfo_rate_hz, 3.3, "LFO GROUP");
        assert_eq!(l.arp_rate_bpm, 77.0, "ARP GROUP");
    }
    vec!["synth.filter.group", "synth.lfo.group", "synth.arp.group"]
}

/// ARP RUN switches the arpeggiator, OCTAVE SHIFT +/- moves the layer, OSC CTRL sets the
/// oscillator control of the selected waveform.
fn run_octave_and_osc_ctrl() -> Vec<&'static str> {
    let mut e = Stage4Engine::new(48_000.0);
    let mut s = SynthUi::new();
    let off = e.synth_a.arp_mode;
    s.press(Btn::ArpRun, Trigger::Primary);
    apply(&mut s, &mut e);
    assert_ne!(e.synth_a.arp_mode, off, "ARP RUN");
    s.press(Btn::ArpRun, Trigger::Primary);
    apply(&mut s, &mut e);
    assert_eq!(e.synth_a.arp_mode, off, "ARP RUN toggles back");

    s.press(Btn::OctaveUp, Trigger::Primary);
    apply(&mut s, &mut e);
    assert_eq!(e.synth_a.octave_offset, 1);
    s.press(Btn::OctaveDown, Trigger::Primary);
    s.press(Btn::OctaveDown, Trigger::Primary);
    apply(&mut s, &mut e);
    assert_eq!(e.synth_a.octave_offset, -1);

    // WAVETABLE type: OSC CTRL is the wavetable position (the SuperSaw / Pulse / FM maps are in
    // `SynthParam::apply`'s own tests).
    s.layer_mut().wave_type = 3;
    s.resend_osc();
    apply(&mut s, &mut e);
    assert_eq!(e.synth_a.osc_mode, SynthOscMode::Wavetable);
    s.pots.tr[Knob::OscCtrl.idx()] = PotTracker::new(0.8);
    s.knob_input(Knob::OscCtrl, 0.8);
    apply(&mut s, &mut e);
    assert_eq!(e.synth_a.wavetable_position, 0.8, "OSC CTRL");
    vec![
        "synth.arp.run",
        "synth.octave_up",
        "synth.octave_down",
        "synth.osc.ctrl",
    ]
}

#[test]
fn every_synth_ui_binding_is_performed_by_the_gui() {
    let mut proven: Vec<String> = [
        page_buttons(),
        velocity_shifts(),
        groups(),
        run_octave_and_osc_ctrl(),
    ]
    .concat()
    .into_iter()
    .map(String::from)
    .collect();
    proven.sort_unstable();
    let mut bound: Vec<String> = all_controls()
        .into_iter()
        .filter(|c| {
            c.section == Section::Synth
                && matches!(c.binding, Binding::Bound(b) if b.starts_with("ui:"))
        })
        .map(|c| c.id)
        .collect();
    bound.sort_unstable();
    assert_eq!(
        proven, bound,
        "the proven ids must be exactly the Synth controls bound to ui:synth.*"
    );
}
