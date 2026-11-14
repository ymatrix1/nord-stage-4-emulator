//! SECTION EDIT (manual p.43) in the plugin editor. The editor draws the same panel as the
//! standalone app, so the panel itself fans an edit out to every layer of the section and the
//! sink then writes each layer's host parameter (the same fan-out as `SynthTarget::All`). These
//! tests latch Section Edit in the editor's own panel state, send the edits the panel produces
//! through `draw_editor` and check the host writes of all layers.

use super::draw::{draw_editor, EditorState, PANEL_H, PANEL_W};
use super::index::ParamIndex;
use super::notes::note_queue;
use super::sink::test_support::Recorder;
use super::write::{ParamWrite, ParamWriter};
use crate::params::Stage4Params;
use nih_plug_egui::{egui, EguiState};
use nord_gui::audio::{AudioCommand, OrganEdit, PianoEdit};
use nord_gui::panel::ui_state::{fx::Unit, FxUi, Knob, SynthUi};
use nord_panel::behaviour::Trigger;
use std::sync::Arc;

struct Shared<'a>(&'a mut Recorder);

impl ParamWriter for Shared<'_> {
    fn write(&mut self, w: &ParamWrite) -> bool {
        self.0.write(w)
    }
}

fn editor() -> EditorState {
    let params = Stage4Params::default();
    let index = Arc::new(ParamIndex::new(&params));
    let (tx, _rx) = note_queue(8);
    EditorState::new(index, tx)
}

/// One editor frame; returns the host writes it made.
fn frame(st: &mut EditorState) -> Vec<ParamWrite> {
    let egui_state = EguiState::from_size(PANEL_W as u32, PANEL_H as u32);
    let ctx = egui::Context::default();
    let mut rec = Recorder::default();
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        draw_editor(ctx, &egui_state, st, Shared(&mut rec));
    });
    rec.writes
}

fn layers_of(writes: &[ParamWrite], family: &str) -> Vec<u8> {
    let mut v: Vec<u8> = writes
        .iter()
        .filter(|w| w.family == family)
        .filter_map(|w| w.layer)
        .collect();
    v.sort_unstable();
    v
}

#[test]
fn the_editor_frame_hands_the_latch_to_every_bay() {
    let mut st = editor();
    frame(&mut st);
    assert!(!st.panel.synth_ui.section_edit && !st.panel.fx_ui.section_edit);
    st.panel
        .program_ui
        .activate("program.section_edit", Trigger::Latch);
    frame(&mut st);
    assert!(st.panel.synth_ui.section_edit);
    assert!(st.panel.fx_ui.section_edit);
}

#[test]
fn a_synth_edit_with_section_edit_writes_all_three_layers() {
    let mut st = editor();
    let mut ui = SynthUi::new();
    ui.section_edit = true;
    ui.knob_input(Knob::Glide, 0.5);
    for e in ui.take_edits() {
        st.handle.send_command(AudioCommand::Synth(e));
    }
    let writes = frame(&mut st);
    let layers = layers_of(&writes, "synthx_glide");
    assert_eq!(layers, [1, 2, 3], "{writes:?}");
}

#[test]
fn a_synth_edit_without_section_edit_writes_the_focused_layer() {
    let mut st = editor();
    let mut ui = SynthUi::new();
    ui.knob_input(Knob::Glide, 0.5);
    for e in ui.take_edits() {
        st.handle.send_command(AudioCommand::Synth(e));
    }
    assert_eq!(layers_of(&frame(&mut st), "synthx_glide"), [1]);
}

#[test]
fn an_fx_edit_with_section_edit_writes_every_chain_of_the_section() {
    let mut st = editor();
    let mut fx = FxUi::default();
    fx.section_edit = true;
    fx.chains[3].reverb_mix = 0.7;
    for e in fx.after_edit(3, Unit::Reverb) {
        st.handle.send_command(AudioCommand::Fx(e));
    }
    // Synth A/B/C are chains 4, 5, 6 (host layers 5, 6, 7).
    assert_eq!(layers_of(&frame(&mut st), "fx_rev_mix"), [5, 6, 7]);
}

#[test]
fn organ_and_piano_fan_out_edits_reach_each_layers_host_parameter() {
    let mut st = editor();
    for layer in 0..2u8 {
        st.handle
            .send_command(AudioCommand::Organ(OrganEdit::Octave { layer, shift: 1 }));
        st.handle
            .send_command(AudioCommand::Piano(PianoEdit::Unison { layer, amount: 2 }));
    }
    let writes = frame(&mut st);
    assert_eq!(layers_of(&writes, "organx_octave"), [1, 2]);
    assert_eq!(layers_of(&writes, "pianox_unison"), [1, 2]);
}
