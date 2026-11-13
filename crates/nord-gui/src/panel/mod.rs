//! Full 6-Bay Top Control Surface and 88-Key Interactive Keybed Layout.
//!
//! Spec: `docs/specs/visual-style-and-panel.md`
//!
//! - `state`: `Stage4PanelState`; `program`: applying a decoded program
//! - `bay_*`: one renderer per front-panel bay; `keybed`: 88-key keyboard

mod bay_fx;
mod bay_master;
mod bay_organ;
mod bay_piano;
mod bay_program;
mod bay_synth;
mod keybed;
mod program;
mod state;
#[cfg(test)]
mod support;

pub use state::Stage4PanelState;

use crate::audio::{apply_audio_command, AudioHandle};
use crate::style::*;
use crate::widgets::*;
use egui::{pos2, vec2, Align2, Color32, Sense, Stroke, StrokeKind, Ui};
use nord_core::engine::Stage4Engine;

/// Renders the entire hardware front panel and synchronizes edits with the DSP audio handle.
pub fn render_stage4_panel(ui: &mut Ui, state: &mut Stage4PanelState, audio_handle: &AudioHandle) {
    let panel_size = vec2(1560.0, 420.0);
    let (panel_rect, _) = ui.allocate_exact_size(panel_size, Sense::hover());
    let painter = ui.painter().clone();

    // 1. Outer Chassis Background
    painter.rect_filled(panel_rect, 6.0, CHASSIS_RED);
    painter.rect_stroke(
        panel_rect,
        6.0,
        Stroke::new(2.0_f32, CHASSIS_DARK),
        StrokeKind::Inside,
    );

    // Chassis branding (top left and top right)
    label(
        &painter,
        pos2(panel_rect.left() + 20.0, panel_rect.top() + 6.0),
        "nord stage 4",
        13.0,
        Color32::WHITE,
        Align2::LEFT_TOP,
    );
    label(
        &painter,
        pos2(panel_rect.right() - 20.0, panel_rect.top() + 6.0),
        "CLAVIA DMI AB • SWEDEN",
        10.0,
        Color32::from_gray(210),
        Align2::RIGHT_TOP,
    );

    bay_master::render_bay_master(ui, &painter, panel_rect, state, audio_handle);
    bay_organ::render_bay_organ(ui, &painter, panel_rect, state, audio_handle);
    bay_piano::render_bay_piano(ui, &painter, panel_rect, state, audio_handle);
    bay_program::render_bay_program(ui, &painter, panel_rect, state, audio_handle);
    bay_synth::render_bay_synth(ui, &painter, panel_rect, state, audio_handle);
    bay_fx::render_bay_fx(ui, &painter, panel_rect, state, audio_handle);
    keybed::render_keybed(ui, panel_rect, state, audio_handle);
}

impl Stage4PanelState {
    /// Renders the front panel directly targeting a synchronous engine (e.g. for testing/offline rendering).
    pub fn ui(&mut self, ui: &mut Ui, engine: &mut Stage4Engine) {
        let (audio_handle, rx) = AudioHandle::new();
        render_stage4_panel(ui, self, &audio_handle);

        while let Ok(cmd) = rx.try_recv() {
            apply_audio_command(cmd, engine, &mut self.master_volume);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioCommand;

    #[test]
    fn test_panel_state_audio_sync() {
        let mut state = Stage4PanelState::default();
        let (audio_handle, rx) = AudioHandle::new();
        state.organ_drawbars[0] = 5;
        audio_handle.send_command(AudioCommand::SetDrawbars(state.organ_drawbars));

        match rx.try_recv() {
            Ok(AudioCommand::SetDrawbars(d)) => {
                assert_eq!(d[0], 5);
            }
            _ => panic!("Expected SetDrawbars"),
        }
    }

    #[test]
    fn test_render_full_panel_headless_paints_shapes() {
        let shapes = support::run_panel_frame(|ui, _painter, _rect, state, audio| {
            render_stage4_panel(ui, state, audio);
        });
        assert!(shapes > 1000);
    }
}
