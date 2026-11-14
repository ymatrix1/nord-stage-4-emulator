//! Full 6-Bay Top Control Surface and 88-Key Interactive Keybed Layout.
//!
//! Spec: `docs/specs/visual-style-and-panel.md`. One file per bay (Phase 6b B4).

mod aux_flag;
mod bay_effects;
mod bay_master;
mod bay_organ;
mod bay_piano;
mod bay_piano_select;
pub(crate) mod bay_program;
mod bay_synth;
mod ctl;
mod kb_zone;
mod keybed;
pub mod layout_px;
mod layout_tweaks;
#[cfg(test)]
mod legibility_rules;
#[cfg(test)]
mod legibility_tests;
mod organ_drawbars;
mod organ_logic;
#[cfg(test)]
mod organ_master_tests;
mod organ_preset;
#[cfg(test)]
mod organ_rewire_tests;
mod organ_strip;
mod organ_widgets;
pub mod panorama;
#[cfg(test)]
mod panorama_tests;
pub mod param_hint;
#[cfg(test)]
mod piano_click_tests;
mod piano_logic;
pub mod piano_models;
mod piano_overlay;
mod piano_widgets;
mod rotor_angle;
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
pub mod view;

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
pub use bay_program::fw_messages::FwMessage;
pub use state::Stage4PanelState;

use bay_effects::render_bay_effects;
use bay_master::render_bay_master;
use bay_organ::render_bay_organ;
use bay_piano::render_bay_piano;
use bay_program::render_bay_program;
use bay_synth::render_bay_synth;
use keybed::render_keybed;
use prelude::*;

/// Draw the whole front panel (or the focused section) at the current view.
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
    let ctx = ui.ctx().clone();
    let cursor = ui.cursor().min;
    state.view.handle_keys(&ctx, state.view.origin(cursor));
    view::apply_zoom(&ctx, &state.view);
    let region = state.view.region();
    let (visible, _) = ui.allocate_exact_size(view::region_size(region), Sense::hover());
    let panel_rect =
        Rect::from_min_size(view::region_origin(region, visible.min), view::PANEL_SIZE);
    let outer_clip = ui.clip_rect();
    if region.is_some() {
        ui.set_clip_rect(visible.intersect(outer_clip));
    }
    let painter = ui.painter().clone();
    render_panel_at(ui, &painter, panel_rect, state, audio_handle);
    if state.view.panorama.is_some() {
        ui.set_clip_rect(outer_clip);
        ui.allocate_space(vec2(0.0, panorama::strip_extra_h()));
        panorama::show_strip(ui, panorama::strip_rect(visible), &mut state.view);
    }
}

/// Draw every bay and the keybed into `panel_rect` (logical size [`view::PANEL_SIZE`]).
fn render_panel_at(
    ui: &mut Ui,
    painter: &Painter,
    panel_rect: Rect,
    state: &mut Stage4PanelState,
    audio_handle: &AudioHandle,
) {
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
        painter,
        pos2(panel_rect.left() + 20.0, panel_rect.top() + 6.0),
        "nord stage 4",
        13.0,
        Color32::WHITE,
        Align2::LEFT_TOP,
    );
    label(
        painter,
        pos2(panel_rect.right() - 20.0, panel_rect.top() + 6.0),
        "OPEN-SOURCE EMULATOR",
        10.0,
        Color32::from_gray(210),
        Align2::RIGHT_TOP,
    );

    // Section headers toggle the focus view; registered first so controls on top win.
    for s in view::FOCUSABLE {
        let hdr = view::header_px(s, panel_rect.min);
        let resp = ui
            .interact(hdr, ui.id().with(("section_header", s)), Sense::click())
            .on_hover_text("Click or press Z: zoom this section (Esc: back, 1-5: jump)");
        if resp.clicked() {
            state.view.toggle(s);
        }
    }

    render_bay_master(ui, painter, panel_rect, state, audio_handle);
    render_bay_organ(ui, painter, panel_rect, state, audio_handle);
    render_bay_piano(ui, painter, panel_rect, state, audio_handle);
    render_bay_program(ui, painter, panel_rect, state, audio_handle);
    render_bay_synth(ui, painter, panel_rect, state, audio_handle);
    render_bay_effects(ui, painter, panel_rect, state, audio_handle);
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
