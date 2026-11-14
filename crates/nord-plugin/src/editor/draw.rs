//! One editor frame: read-back, the shared front panel, dimmed-control overlay, sink pump.
//!
//! The panel is the same `render_stage4_panel` the standalone app draws (`nord-gui`), at its
//! native 1560 x 420 logical px. The window scales it with egui's zoom factor so the whole
//! panel always fits (aspect preserved, centred, letterboxed in the chassis colour), which
//! also covers HiDPI: the host's DPI scale multiplies on top through `nih_plug_egui`.

use super::coverage::{Coverage, DIM_TOOLTIP};
use super::index::ParamIndex;
use super::notes::NoteSender;
use super::readback::sync_panel_from_params;
use super::sink::{pump, ParamSink};
use super::write::ParamWriter;
use crossbeam_channel::Receiver;
use nih_plug_egui::egui::{
    self, Color32, Context, Id, Rect, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2,
};
use nih_plug_egui::resizable_window::ResizableWindow;
use nih_plug_egui::EguiState;
use nord_gui::audio::{AudioCommand, AudioHandle};
use nord_gui::panel::{layout_px, render_stage4_panel, Stage4PanelState};
use nord_gui::style::CHASSIS_DARK;
use std::sync::Arc;

/// Logical panel size the shared drawing code is laid out for.
pub const PANEL_W: f32 = 1560.0;
pub const PANEL_H: f32 = 420.0;
/// Default editor window size (75 % of native; fits a laptop screen).
pub const DEFAULT_SIZE: (u32, u32) = (1170, 315);
/// Smallest scale the editor shrinks to.
pub const MIN_SCALE: f32 = 0.25;

/// Everything the editor keeps between frames (GUI thread only).
pub struct EditorState {
    pub panel: Stage4PanelState,
    pub handle: AudioHandle,
    pub rx: Receiver<AudioCommand>,
    pub index: Arc<ParamIndex>,
    pub coverage: Coverage,
    pub notes: NoteSender,
    /// Dim overlays painted in the last frame (for tests and diagnostics).
    pub last_overlays: usize,
    /// Panel commands dropped for lack of a host parameter.
    pub unmapped: u64,
}

impl EditorState {
    pub fn new(index: Arc<ParamIndex>, notes: NoteSender) -> Self {
        let (handle, rx) = AudioHandle::new();
        let coverage = super::coverage::classify(&index);
        Self {
            panel: Stage4PanelState::default(),
            handle,
            rx,
            index,
            coverage,
            notes,
            last_overlays: 0,
            unmapped: 0,
        }
    }
}

/// Zoom that makes the whole panel fit a window of `size` logical px.
pub fn scale_for(size: (u32, u32)) -> f32 {
    let (w, h) = (size.0 as f32, size.1 as f32);
    (w / PANEL_W).min(h / PANEL_H).max(MIN_SCALE)
}

/// Translucent overlay with a tooltip over every dimmed control; returns how many were drawn.
///
/// The overlay also takes the pointer so a dimmed control cannot be clicked or dragged.
pub fn draw_dim_overlay(ui: &mut Ui, origin: egui::Pos2, coverage: &Coverage) -> usize {
    let mut n = 0;
    for id in coverage.dimmed_ids() {
        let Some(rect) = layout_px::rect_of(id, origin) else {
            continue;
        };
        ui.painter()
            .rect_filled(rect, 3.0_f32, Color32::from_black_alpha(150));
        ui.painter().rect_stroke(
            rect,
            3.0_f32,
            Stroke::new(1.0_f32, Color32::from_black_alpha(90)),
            StrokeKind::Inside,
        );
        ui.interact(rect, Id::new(("dimmed", id)), Sense::click_and_drag())
            .on_hover_text(DIM_TOOLTIP);
        n += 1;
    }
    n
}

/// Draw one frame of the editor and forward the panel's edits to `writer`.
pub fn draw_editor<W: ParamWriter>(
    ctx: &Context,
    egui_state: &EguiState,
    st: &mut EditorState,
    writer: W,
) {
    sync_panel_from_params(&st.index, &mut st.panel);

    let scale = scale_for(egui_state.size());
    if (ctx.zoom_factor() - scale).abs() > 1e-3 {
        ctx.set_zoom_factor(scale);
    }

    ResizableWindow::new("nord-stage-4-editor")
        .min_size(Vec2::splat(8.0_f32))
        .show(ctx, egui_state, |ui| {
            let avail = ui.max_rect();
            ui.painter().rect_filled(avail, 0.0_f32, CHASSIS_DARK);
            let size = Vec2::new(PANEL_W, PANEL_H);
            let offset = ((avail.size() - size) * 0.5).max(Vec2::ZERO);
            let panel_rect = Rect::from_min_size(avail.min + offset, size);
            ui.scope_builder(UiBuilder::new().max_rect(panel_rect), |ui| {
                render_stage4_panel(ui, &mut st.panel, &st.handle);
            });
            st.last_overlays = draw_dim_overlay(ui, panel_rect.min, &st.coverage);
        });

    let mut sink = ParamSink::new(writer, &st.notes);
    pump(&st.rx, &mut sink);
    st.unmapped += sink.unmapped;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::notes::note_queue;
    use crate::editor::sink::test_support::Recorder;
    use crate::params::Stage4Params;

    fn state() -> (
        Stage4Params,
        EditorState,
        crate::editor::notes::NoteReceiver,
    ) {
        let params = Stage4Params::default();
        let index = Arc::new(ParamIndex::new(&params));
        let (tx, rx) = note_queue(64);
        (params, EditorState::new(index, tx), rx)
    }

    #[test]
    fn scale_fits_the_panel_and_keeps_the_aspect() {
        assert!((scale_for((1560, 420)) - 1.0).abs() < 1e-6);
        assert!((scale_for((780, 210)) - 0.5).abs() < 1e-6);
        // Wide window: height limits the scale.
        assert!((scale_for((3120, 420)) - 1.0).abs() < 1e-6);
        // Tall window: width limits the scale.
        assert!((scale_for((780, 2000)) - 0.5).abs() < 1e-6);
        assert_eq!(scale_for((1, 1)), MIN_SCALE);
        assert_eq!(
            scale_for(DEFAULT_SIZE),
            DEFAULT_SIZE.0 as f32 / PANEL_W,
            "default size has the panel aspect"
        );
    }

    #[test]
    fn default_size_has_the_panel_aspect_ratio() {
        let a = DEFAULT_SIZE.0 as f32 / DEFAULT_SIZE.1 as f32;
        assert!((a - PANEL_W / PANEL_H).abs() < 0.05, "{a}");
    }

    #[test]
    fn a_headless_frame_draws_the_panel_and_one_overlay_per_dimmed_control() {
        let (_params, mut st, _rx) = state();
        let egui_state = EguiState::from_size(PANEL_W as u32, PANEL_H as u32);
        let ctx = Context::default();
        let out = ctx.run(egui::RawInput::default(), |ctx| {
            draw_editor(ctx, &egui_state, &mut st, Recorder::default());
        });
        assert!(out.shapes.len() > 1000, "only {} shapes", out.shapes.len());
        assert_eq!(st.last_overlays, st.coverage.dimmed_ids().count());
        assert!(st.last_overlays > 0, "some controls must be dimmed today");
    }

    #[test]
    fn a_panel_edit_in_a_frame_reaches_the_writer_and_the_note_queue() {
        let (_params, mut st, notes) = state();
        st.handle.send_command(AudioCommand::SetMasterVolume(0.33));
        st.handle.send_midi(nord_core::midi::MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 99,
        });
        let egui_state = EguiState::from_size(PANEL_W as u32, PANEL_H as u32);
        let ctx = Context::default();
        let mut rec = Recorder::default();
        // Re-borrow the recorder through a wrapper so the test can inspect it afterwards.
        struct Shared<'a>(&'a mut Recorder);
        impl ParamWriter for Shared<'_> {
            fn write(&mut self, w: &crate::editor::write::ParamWrite) -> bool {
                self.0.write(w)
            }
        }
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            draw_editor(ctx, &egui_state, &mut st, Shared(&mut rec));
        });
        assert_eq!(rec.writes.len(), 1);
        assert_eq!(rec.writes[0].family, "master_level");
        assert!(notes.pop().is_some());
    }

    #[test]
    fn dim_overlay_blocks_nothing_when_nothing_is_dimmed() {
        let ctx = Context::default();
        let empty = Coverage { entries: vec![] };
        let mut n = usize::MAX;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                n = draw_dim_overlay(ui, egui::pos2(0.0_f32, 0.0_f32), &empty);
            });
        });
        assert_eq!(n, 0);
    }
}
