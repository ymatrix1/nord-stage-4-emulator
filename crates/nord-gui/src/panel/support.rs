//! Headless egui helpers shared by the panel tests.

use super::prelude::*;
use super::render_stage4_panel;

/// Run `body` inside one headless frame and return how many shapes it painted.
pub(super) fn shapes_painted(
    body: impl FnOnce(&mut Ui, &Painter, Rect, &mut Stage4PanelState, &AudioHandle),
) -> usize {
    let ctx = egui::Context::default();
    let (audio_handle, _rx) = AudioHandle::new();
    let mut state = Stage4PanelState::default();
    let mut body = Some(body);
    let out = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(1560.0, 420.0), Sense::hover());
            let painter = ui.painter().clone();
            if let Some(f) = body.take() {
                f(ui, &painter, rect, &mut state, &audio_handle);
            }
        });
    });
    out.shapes.len()
}

/// Shapes painted by the whole panel in one headless frame.
pub(super) fn shapes_for_full_panel() -> usize {
    let ctx = egui::Context::default();
    let (audio_handle, _rx) = AudioHandle::new();
    let mut state = Stage4PanelState::default();
    let out = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            render_stage4_panel(ui, &mut state, &audio_handle);
        });
    });
    out.shapes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_counts_shapes_of_a_trivial_body() {
        let n = shapes_painted(|_ui, painter, rect, _s, _a| {
            painter.rect_filled(rect, 0.0, Color32::RED);
        });
        assert!(n >= 1);
    }

    #[test]
    fn full_panel_helper_is_not_empty() {
        assert!(shapes_for_full_panel() > 0);
    }
}
