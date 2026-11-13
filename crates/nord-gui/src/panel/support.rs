//! Shared panel-test fixture: one headless egui frame with a default state and audio queue.

use super::Stage4PanelState;
use crate::audio::AudioHandle;
use egui::{pos2, vec2, Painter, Rect, Ui};

/// Runs `f` inside a headless `CentralPanel` with a 1560 x 420 panel rect at the origin, a default
/// `Stage4PanelState` and a fresh `AudioHandle`; returns the number of shapes painted.
pub fn run_panel_frame(
    mut f: impl FnMut(&mut Ui, &Painter, Rect, &mut Stage4PanelState, &AudioHandle),
) -> usize {
    let ctx = egui::Context::default();
    let (audio, _rx) = AudioHandle::new();
    let mut state = Stage4PanelState::default();
    let out = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1560.0, 420.0));
            let painter = ui.painter().clone();
            f(ui, &painter, rect, &mut state, &audio);
        });
    });
    out.shapes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_panel_frame_invokes_closure_once_per_frame() {
        let mut calls = 0;
        run_panel_frame(|_, _, rect, _, _| {
            calls += 1;
            assert_eq!(rect.size(), vec2(1560.0, 420.0));
        });
        assert_eq!(calls, 1);
    }
}
