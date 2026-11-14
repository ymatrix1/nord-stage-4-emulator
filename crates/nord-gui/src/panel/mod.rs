//! Full 6-Bay Top Control Surface and 88-Key Interactive Keybed Layout.
//!
//! Spec: `docs/specs/visual-style-and-panel.md`. One file per bay (Phase 6b B4).

mod aux_flag;
mod bay_effects;
mod bay_master;
mod bay_organ;
mod bay_piano;
mod bay_piano_select;
mod bay_program;
mod bay_synth;
mod ctl;
mod kb_zone;
mod keybed;
pub mod layout_px;
mod organ_drawbars;
mod organ_logic;
#[cfg(test)]
mod organ_master_tests;
mod organ_preset;
#[cfg(test)]
mod organ_rewire_tests;
mod organ_strip;
mod organ_widgets;
#[cfg(test)]
mod piano_click_tests;
mod piano_logic;
mod piano_overlay;
mod piano_widgets;
#[cfg(test)]
mod sim;
mod state;
#[cfg(test)]
mod support;
#[cfg(test)]
mod telemetry_wiring_tests;
#[cfg(test)]
mod tests;
pub mod ui_state;

/// Names every bay file needs; imported with `use super::prelude::*;`.
mod prelude {
    pub use super::state::Stage4PanelState;
    pub use crate::audio::{apply_audio_command, AudioCommand, AudioHandle};
    pub use crate::style::*;
    pub use crate::widgets::*;
    pub use egui::Painter;
    pub use egui::{pos2, vec2, Align2, Color32, Rect, Sense, Stroke, StrokeKind, Ui};
    pub use nord_core::engine::Stage4Engine;
    pub use nord_core::fx::{Mod1Type, Mod2Type, ReverbModel};
    pub use nord_core::midi::MidiEvent;
    pub use nord_core::organ::{OrganModel, RotarySpeed, ScannerMode};
    pub use nord_core::piano::{PianoCategory, PianoTimbre, VelocityTouch};
    pub use nord_core::synth::{FilterModel, SynthOscMode, VaWaveform};
}

pub use bay_program::catalog::LoadRequest;
pub use state::Stage4PanelState;

use bay_effects::render_bay_effects;
use bay_master::render_bay_master;
use bay_organ::render_bay_organ;
use bay_piano::render_bay_piano;
use bay_program::render_bay_program;
use bay_synth::render_bay_synth;
use keybed::render_keybed;
use prelude::*;

pub fn render_stage4_panel(ui: &mut Ui, state: &mut Stage4PanelState, audio_handle: &AudioHandle) {
    // One lock-free read of the engine per frame; the bays take what they show from it.
    let now_ms = (ui.input(|i| i.time) * 1000.0) as u32;
    state.telemetry = audio_handle.telemetry().latest();
    state.leds.update(&state.telemetry, now_ms);
    if state.telemetry.get().is_some() {
        // LED ladders and activity LEDs follow the engine, not only mouse input.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
    }
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
        "OPEN-SOURCE EMULATOR",
        10.0,
        Color32::from_gray(210),
        Align2::RIGHT_TOP,
    );

    render_bay_master(ui, &painter, panel_rect, state, audio_handle);
    render_bay_organ(ui, &painter, panel_rect, state, audio_handle);
    render_bay_piano(ui, &painter, panel_rect, state, audio_handle);
    render_bay_program(ui, &painter, panel_rect, state, audio_handle);
    render_bay_synth(ui, &painter, panel_rect, state, audio_handle);
    render_bay_effects(ui, &painter, panel_rect, state, audio_handle);
    render_keybed(ui, panel_rect, state, audio_handle);
}

#[cfg(test)]
mod frame_tests {
    #[test]
    fn full_panel_paints_all_bays_and_keybed() {
        let n = super::support::shapes_for_full_panel();
        assert!(n > 1000, "full panel painted only {n} shapes");
    }
}
