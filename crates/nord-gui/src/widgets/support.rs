//! Shared widget-test fixtures: run one headless egui frame and count painted shapes.

use egui::{pos2, vec2, Rect, Ui};

/// Rect from `(x, y, w, h)`.
pub fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(pos2(x, y), vec2(w, h))
}

/// Runs `f` once inside a headless `CentralPanel` frame; returns the number of shapes painted.
pub fn run_in_ui(mut f: impl FnMut(&mut Ui)) -> usize {
    let ctx = egui::Context::default();
    let out = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| f(ui));
    });
    out.shapes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_in_ui_reports_painted_shapes() {
        let n = run_in_ui(|ui| {
            ui.painter()
                .rect_filled(rect(0.0, 0.0, 10.0, 10.0), 0.0, egui::Color32::RED);
        });
        assert!(n >= 1);
        assert_eq!(rect(1.0, 2.0, 3.0, 4.0).size(), vec2(3.0, 4.0));
    }
}
