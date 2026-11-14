//! Headless end-to-end tests of the Synth bay: every inventory id is painted, dimmed controls are
//! the ones without engine support, and clicks reach the DSP thread as `AudioCommand::Synth`.

use super::render_bay_synth;
use crate::audio::{
    apply_audio_command, AudioCommand, AudioHandle, SynthEdit, SynthParam, SynthTarget,
};
use crate::panel::layout_px::rect_or_empty;
use crate::panel::prelude::*;
use crate::panel::ui_state::unsupported_reason;
use nord_panel::{all_controls, Section};

fn run_frame(
    ctx: &egui::Context,
    state: &mut Stage4PanelState,
    audio: &AudioHandle,
    input: egui::RawInput,
) {
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(1560.0, 420.0), Sense::hover());
            let painter = ui.painter().clone();
            render_bay_synth(ui, &painter, rect, state, audio);
        });
    });
}

fn plain(events: Vec<egui::Event>, modifiers: egui::Modifiers) -> egui::RawInput {
    egui::RawInput {
        events,
        modifiers,
        ..egui::RawInput::default()
    }
}

/// Click the centre of control `id` over three frames (move, press, release).
fn click(
    ctx: &egui::Context,
    state: &mut Stage4PanelState,
    audio: &AudioHandle,
    id: &str,
    modifiers: egui::Modifiers,
) {
    let c = rect_or_empty(id, pos2(0.0, 0.0)).center();
    run_frame(
        ctx,
        state,
        audio,
        plain(vec![egui::Event::PointerMoved(c)], modifiers),
    );
    for pressed in [true, false] {
        let ev = egui::Event::PointerButton {
            pos: c,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        };
        run_frame(ctx, state, audio, plain(vec![ev], modifiers));
    }
}

fn synth_edits(rx: &crossbeam_channel::Receiver<AudioCommand>) -> Vec<SynthEdit> {
    rx.try_iter()
        .filter_map(|c| match c {
            AudioCommand::Synth(e) => Some(e),
            _ => None,
        })
        .collect()
}

fn fresh() -> (
    egui::Context,
    Stage4PanelState,
    AudioHandle,
    crossbeam_channel::Receiver<AudioCommand>,
) {
    let ctx = egui::Context::default();
    let (audio, rx) = AudioHandle::new();
    let mut state = Stage4PanelState::default();
    run_frame(&ctx, &mut state, &audio, egui::RawInput::default());
    while rx.try_recv().is_ok() {}
    (ctx, state, audio, rx)
}

fn synth_ids() -> Vec<String> {
    all_controls()
        .into_iter()
        .filter(|c| c.section == Section::Synth)
        .map(|c| c.id)
        .collect()
}

#[test]
fn every_synth_inventory_id_has_a_painted_widget() {
    let (_ctx, state, _audio, _rx) = fresh();
    let drawn = state.synth_ui.drawn();
    let ids = synth_ids();
    assert_eq!(ids.len(), 73, "inventory lists 73 synth controls");
    let missing: Vec<_> = ids
        .iter()
        .filter(|id| !drawn.contains(&id.as_str()))
        .collect();
    assert!(missing.is_empty(), "no widget painted for {missing:?}");
    for d in drawn {
        assert!(ids.iter().any(|id| id == d), "{d} is not an inventory id");
    }
}

#[test]
fn dimmed_controls_are_exactly_the_ones_without_engine_support() {
    let mut dim: Vec<_> = synth_ids()
        .into_iter()
        .filter(|id| unsupported_reason(id).is_some())
        .collect();
    dim.sort();
    let mut expected = [
        "synth.aux_led.a",
        "synth.aux_led.b",
        "synth.aux_led.c",
        "synth.extern",
        "synth.kb_zone_down",
        "synth.kb_zone_up",
        "synth.zone_leds",
    ];
    expected.sort();
    assert_eq!(dim, expected);
}

#[test]
fn clicking_unison_sends_a_synth_edit_the_engine_applies() {
    let (ctx, mut state, audio, rx) = fresh();
    click(
        &ctx,
        &mut state,
        &audio,
        "synth.unison",
        egui::Modifiers::NONE,
    );
    let edits = synth_edits(&rx);
    assert_eq!(
        edits,
        vec![SynthEdit::Param(
            SynthTarget::Layer(0),
            SynthParam::Unison(1)
        )]
    );
    let mut engine = Stage4Engine::new(48_000.0);
    let mut vol = 0.8;
    for e in edits {
        apply_audio_command(AudioCommand::Synth(e), &mut engine, &mut vol);
    }
    assert_eq!(engine.synth_a.unison, 1);
    assert_eq!(engine.synth_b.unison, 0);
}

#[test]
fn shift_click_on_filter_type_turns_the_filter_group_on() {
    let (ctx, mut state, audio, rx) = fresh();
    let shift = egui::Modifiers {
        shift: true,
        ..egui::Modifiers::NONE
    };
    click(&ctx, &mut state, &audio, "synth.filter.type", shift);
    assert!(state.synth_ui.group_filter.is_active());
    assert!(synth_edits(&rx)
        .iter()
        .any(|e| matches!(e, SynthEdit::Group { src: 0, .. })));
}

#[test]
fn clicking_layer_b_focuses_and_activates_it() {
    let (ctx, mut state, audio, rx) = fresh();
    click(
        &ctx,
        &mut state,
        &audio,
        "synth.layer.b",
        egui::Modifiers::NONE,
    );
    assert_eq!(state.synth_ui.focus, 1);
    assert!(state.synth_b_active);
    assert!(rx
        .try_iter()
        .any(|c| matches!(c, AudioCommand::SetLayerActive(5, true))));
}

#[test]
fn clicking_a_page_button_switches_the_display_page() {
    let (ctx, mut state, audio, _rx) = fresh();
    click(
        &ctx,
        &mut state,
        &audio,
        "synth.filter.envelope",
        egui::Modifiers::NONE,
    );
    assert_eq!(
        state.synth_ui.page,
        crate::panel::ui_state::SynthPage::EnvFilter
    );
}

#[test]
fn program_load_values_in_the_old_fields_reach_layer_a() {
    let (ctx, mut state, audio, _rx) = fresh();
    state.synth_cutoff = 3200.0;
    run_frame(&ctx, &mut state, &audio, egui::RawInput::default());
    assert_eq!(state.synth_ui.layers[0].cutoff_hz, 3200.0);
    assert_eq!(state.synth_cutoff, 3200.0);
}

fn shift() -> egui::Modifiers {
    egui::Modifiers {
        shift: true,
        ..egui::Modifiers::NONE
    }
}

#[test]
fn shift_click_on_waveform_runs_sound_init_on_the_engine_without_undo() {
    let (ctx, mut state, audio, rx) = fresh();
    let mut engine = Stage4Engine::new(48_000.0);
    engine.synth_a.cutoff_hz = 321.0;
    engine.synth_a.perf.zigzag = true;
    click(&ctx, &mut state, &audio, "synth.sound_init", shift());
    let edits = synth_edits(&rx);
    assert_eq!(edits.first(), Some(&SynthEdit::SoundInit(0)));
    let mut vol = 0.8;
    for e in edits {
        apply_audio_command(AudioCommand::Synth(e), &mut engine, &mut vol);
    }
    assert_ne!(engine.synth_a.cutoff_hz, 321.0);
    assert!(!engine.synth_a.perf.zigzag);
    assert_eq!(engine.undo_available(), None);
}

#[test]
fn shift_clicking_layer_a_and_b_toggles_sustped_and_pstick_in_the_engine() {
    let (ctx, mut state, audio, rx) = fresh();
    click(&ctx, &mut state, &audio, "synth.layer.a", shift());
    click(&ctx, &mut state, &audio, "synth.layer.b", shift());
    let edits = synth_edits(&rx);
    assert!(edits.contains(&SynthEdit::Sustped(true)));
    assert!(edits.contains(&SynthEdit::Pstick(true)));
    let mut engine = Stage4Engine::new(48_000.0);
    let mut vol = 0.8;
    for e in edits {
        apply_audio_command(AudioCommand::Synth(e), &mut engine, &mut vol);
    }
    assert_eq!(engine.pedals.sustped[4..7], [true; 3]);
    assert_eq!(engine.pedals.pstick[4..7], [true; 3]);
    assert!(state.synth_ui.sustped && state.synth_ui.pstick);
}
