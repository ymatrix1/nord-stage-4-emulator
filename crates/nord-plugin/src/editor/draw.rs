//! One editor frame: read-back, the shared front panel, dimmed-control overlay, sink pump.
//!
//! The panel is the same `render_stage4_panel` the standalone app draws (`nord-gui`), at its
//! native 1560 x 420 logical px. The panel lives in its own egui layer whose transform scales
//! and centres it so the whole panel always fits (aspect preserved, letterboxed in the chassis
//! colour). The host's DPI scale multiplies on top through `nih_plug_egui`.
//!
//! egui's zoom factor is NOT used: `egui_baseview` lays out and tessellates with the native
//! pixels-per-point, so a zoom factor below 1 only changed font rounding (the panel drew at
//! 100 %, clipped, and the ppp mismatch is the suspected cause of the flicker in FL Studio).

use super::coverage::{Coverage, DIM_TOOLTIP};
use super::index::ParamIndex;
use super::notes::NoteSender;
use super::readback::sync_panel_from_params;
use super::sink::{pump, ParamSink};
use super::write::ParamWriter;
use crossbeam_channel::Receiver;
use nih_plug_egui::egui::emath::TSTransform;
use nih_plug_egui::egui::{
    self, Area, Color32, Context, Id, LayerId, Order, Pos2, Rect, Sense, Stroke, StrokeKind, Ui,
    UiBuilder, Vec2,
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
/// Default editor window size: the native panel size (100 %, sharp text).
pub const DEFAULT_SIZE: (u32, u32) = (1560, 420);
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

/// Scale that makes the whole panel fit an area of `size` logical points (aspect preserved).
pub fn fit_scale(size: Vec2) -> f32 {
    (size.x / PANEL_W).min(size.y / PANEL_H).max(MIN_SCALE)
}

/// Layer that carries the scaled panel.
fn panel_layer() -> LayerId {
    LayerId::new(Order::Background, Id::new("nord-stage-4-panel"))
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

    // The shared panel would otherwise drive egui's zoom itself (fighting this one every frame
    // and flickering); the editor always shows the whole panel.
    st.panel.view.host_scaled = true;
    st.panel.view.focus = None;
    st.panel.view.panorama = None;

    // egui_baseview ignores egui's zoom factor (see the module doc): keep it at 1.
    if (ctx.zoom_factor() - 1.0).abs() > 1e-3 {
        ctx.set_zoom_factor(1.0);
    }

    // The central panel (it carries the resize corner) is drawn above the scaled panel layer, and
    // its default frame fills the whole window with `panel_fill` (CHASSIS_RED in the app style):
    // opaque, it hid the panel and left only red. Keep it transparent.
    if ctx.style().visuals.panel_fill != Color32::TRANSPARENT {
        ctx.style_mut(|s| s.visuals.panel_fill = Color32::TRANSPARENT);
    }

    ResizableWindow::new("nord-stage-4-editor")
        .min_size(Vec2::splat(8.0_f32))
        .show(ctx, egui_state, |ui| {
            let avail = ui.max_rect();
            // Layers (all `Order::Background`): the scaled panel, then this central panel carrying
            // `ResizableWindow`'s corner handle, moved to the top each frame. The panel covers the
            // whole window, so without that the corner never got the pointer and the window could
            // not be resized. The letterbox fill is drawn inside the panel layer, behind the
            // panel: a separate fill layer ended up above it and hid the whole panel.
            let scale = fit_scale(avail.size());
            let panel_size = Vec2::new(PANEL_W, PANEL_H);
            let offset = ((avail.size() - panel_size * scale) * 0.5).max(Vec2::ZERO);
            // The panel is laid out in its own coordinates (origin 0,0, native size); the layer
            // transform maps that onto the window.
            let to_window = TSTransform::new(avail.min.to_vec2() + offset, scale);
            let window_in_panel_coords = to_window.inverse() * avail;
            let panel_rect = Rect::from_min_size(Pos2::ZERO, panel_size);
            Area::new(Id::new("nord-stage-4-panel"))
                .order(Order::Background)
                .fixed_pos(Pos2::ZERO)
                .constrain(false)
                .show(ui.ctx(), |ui| {
                    ui.painter()
                        .with_clip_rect(window_in_panel_coords)
                        .rect_filled(window_in_panel_coords, 0.0_f32, CHASSIS_DARK);
                    ui.set_clip_rect(panel_rect);
                    ui.scope_builder(UiBuilder::new().max_rect(panel_rect), |ui| {
                        render_stage4_panel(ui, &mut st.panel, &st.handle);
                    });
                    st.last_overlays = draw_dim_overlay(ui, panel_rect.min, &st.coverage);
                });
            ui.ctx().set_transform_layer(panel_layer(), to_window);
            ui.ctx().move_to_top(LayerId::background());
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
    fn the_panel_is_scaled_by_its_layer_and_egui_zoom_stays_at_one() {
        let (_params, mut st, _rx) = state();
        let egui_state = EguiState::from_size(1170, 315);
        let ctx = Context::default();
        let size = Vec2::new(1170.0, 315.0);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            ..Default::default()
        };
        // A few frames: the zoom must never leave 1 and the layer transform must settle.
        for _ in 0..3 {
            let _ = ctx.run(input.clone(), |ctx| {
                draw_editor(ctx, &egui_state, &mut st, Recorder::default());
            });
        }
        assert!((ctx.zoom_factor() - 1.0).abs() < 1e-6);
        let t = ctx
            .layer_transform_to_global(panel_layer())
            .expect("panel layer transform");
        let want = 1170.0 / PANEL_W;
        assert!((t.scaling - want).abs() < 1e-4, "{} vs {want}", t.scaling);
        assert!(t.translation.x.abs() < 1.0 && t.translation.y.abs() < 1.0);
    }

    /// Regression: the shared panel used to set its own zoom every frame (flip-flop with the
    /// editor's), so the drawn size alternated frame to frame (flicker in FL Studio).
    #[test]
    fn the_drawn_size_is_stable_across_frames() {
        let (_params, mut st, _rx) = state();
        let egui_state = EguiState::from_size(DEFAULT_SIZE.0, DEFAULT_SIZE.1);
        let ctx = Context::default();
        let size = Vec2::new(DEFAULT_SIZE.0 as f32, DEFAULT_SIZE.1 as f32);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            ..Default::default()
        };
        let mut bounds = vec![];
        for _ in 0..6 {
            let out = ctx.run(input.clone(), |ctx| {
                draw_editor(ctx, &egui_state, &mut st, Recorder::default());
            });
            let r = out.shapes.iter().fold(Rect::NOTHING, |r, s| {
                r.union(s.shape.visual_bounding_rect().intersect(s.clip_rect))
            });
            bounds.push(r);
            assert!((ctx.zoom_factor() - 1.0).abs() < 1e-6);
        }
        // Frame 0 is egui's Area sizing pass (nothing of the panel is shown); it must then settle.
        assert!(
            bounds[1].width() >= size.x - 1.0,
            "panel drawn: {:?}",
            bounds[1]
        );
        for r in &bounds[1..] {
            assert!(r.max.x <= size.x + 1.0 && r.max.y <= size.y + 1.0, "{r:?}");
            assert_eq!(*r, bounds[1]);
        }
    }

    /// Regression: the scaled panel layer covered the window, so the resize corner (a widget of
    /// the central panel) never got the pointer and the editor could not be resized.
    #[test]
    fn dragging_the_bottom_right_corner_requests_a_resize() {
        let (_params, mut st, _rx) = state();
        let egui_state = EguiState::from_size(DEFAULT_SIZE.0, DEFAULT_SIZE.1);
        let ctx = Context::default();
        let size = Vec2::new(DEFAULT_SIZE.0 as f32, DEFAULT_SIZE.1 as f32);
        let frame = |events: Vec<egui::Event>, st: &mut EditorState| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                events,
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                draw_editor(ctx, &egui_state, st, Recorder::default());
            });
        };
        let corner = Pos2::new(size.x - 4.0, size.y - 4.0);
        frame(vec![], &mut st);
        frame(vec![egui::Event::PointerMoved(corner)], &mut st);
        frame(
            vec![egui::Event::PointerButton {
                pos: corner,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }],
            &mut st,
        );
        let smaller = corner - Vec2::new(300.0, 80.0);
        frame(vec![egui::Event::PointerMoved(smaller)], &mut st);
        frame(vec![egui::Event::PointerMoved(smaller)], &mut st);
        let dbg = format!("{egui_state:?}");
        assert!(
            dbg.contains("requested_size: AtomicCell { value: Some("),
            "{dbg}"
        );
    }

    /// Regression: the central panel is drawn above the panel layer; an opaque `panel_fill`
    /// (CHASSIS_RED in the app style) hid the whole panel, leaving only red.
    #[test]
    fn the_top_central_panel_fill_is_transparent_even_with_the_app_style() {
        let (_params, mut st, _rx) = state();
        let egui_state = EguiState::from_size(DEFAULT_SIZE.0, DEFAULT_SIZE.1);
        let ctx = Context::default();
        nord_gui::style::setup_nord_visuals(&ctx);
        let size = Vec2::new(DEFAULT_SIZE.0 as f32, DEFAULT_SIZE.1 as f32);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            ..Default::default()
        };
        let mut out = None;
        for _ in 0..3 {
            out = Some(ctx.run(input.clone(), |ctx| {
                draw_editor(ctx, &egui_state, &mut st, Recorder::default());
            }));
        }
        assert_eq!(ctx.style().visuals.panel_fill, Color32::TRANSPARENT);
        // Nothing opaque and window-sized may be drawn after the panel content (the last 100
        // shapes are panel content or overlays): neither the central-panel fill nor a fill layer.
        for s in out.unwrap().shapes.iter().rev().take(100) {
            if let egui::Shape::Rect(r) = &s.shape {
                let covers = r.rect.width() >= size.x - 1.0 && r.rect.height() >= size.y - 1.0;
                assert!(!(covers && r.fill.a() > 0), "opaque cover {r:?}");
            }
        }
    }

    #[test]
    fn fit_scale_matches_scale_for_and_letterboxes() {
        assert!((fit_scale(Vec2::new(780.0, 210.0)) - 0.5).abs() < 1e-6);
        assert!((fit_scale(Vec2::new(3120.0, 420.0)) - 1.0).abs() < 1e-6);
        assert_eq!(fit_scale(Vec2::new(1.0, 1.0)), MIN_SCALE);
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
