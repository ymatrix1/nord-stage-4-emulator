//! Front-panel bay: program/morph controls, OLED and program select buttons.

use crate::audio::AudioHandle;
use crate::panel::Stage4PanelState;
use crate::style::*;
use crate::widgets::*;
use egui::{pos2, vec2, Align2, Color32, Painter, Rect, Stroke, StrokeKind, Ui};
use nord_core::synth::SynthOscMode;

/// Draws and drives the bay: program/morph controls, OLED and program select buttons.
pub(super) fn render_bay_program(
    ui: &mut Ui,
    painter: &Painter,
    panel_rect: Rect,
    state: &mut Stage4PanelState,
    _audio_handle: &AudioHandle,
) {
    // =========================================================================
    // BAY 4: PROGRAM / MORPH (Crimson Chassis: x: 794..1014, y: 24..300)
    // =========================================================================
    let bay4 = Rect::from_min_max(
        pos2(panel_rect.left() + 794.0, panel_rect.top() + 24.0),
        pos2(panel_rect.left() + 1014.0, panel_rect.top() + 300.0),
    );
    painter.rect_filled(bay4, 5.0, Color32::from_rgb(115, 18, 22));
    painter.rect_stroke(
        bay4,
        5.0,
        Stroke::new(1.0_f32, GROUP_BORDER),
        StrokeKind::Inside,
    );

    // Morph Assign buttons & LEDs
    let morph_box =
        Rect::from_min_size(pos2(bay4.left() + 8.0, bay4.top() + 8.0), vec2(204.0, 36.0));
    painter.rect_filled(morph_box, 3.0, Color32::from_rgb(80, 12, 16));
    label(
        painter,
        pos2(morph_box.center().x, morph_box.top() + 6.0),
        "MORPH ASSIGN",
        8.5,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    let btn_w = 58.0;
    let b_wheel = Rect::from_min_size(
        pos2(morph_box.left() + 6.0, morph_box.top() + 14.0),
        vec2(btn_w, 16.0),
    );
    if nord_button(ui, b_wheel, "btn_m_wheel", state.morph_wheel_active) {
        state.morph_wheel_active = !state.morph_wheel_active;
    }
    label(
        painter,
        pos2(b_wheel.center().x, b_wheel.center().y),
        "WHEEL",
        8.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    let b_at = Rect::from_min_size(
        pos2(morph_box.left() + 72.0, morph_box.top() + 14.0),
        vec2(btn_w, 16.0),
    );
    if nord_button(ui, b_at, "btn_m_at", state.morph_at_active) {
        state.morph_at_active = !state.morph_at_active;
    }
    label(
        painter,
        pos2(b_at.center().x, b_at.center().y),
        "A.T.",
        8.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    let b_ped = Rect::from_min_size(
        pos2(morph_box.left() + 138.0, morph_box.top() + 14.0),
        vec2(btn_w, 16.0),
    );
    if nord_button(ui, b_ped, "btn_m_ped", state.morph_ctrl_active) {
        state.morph_ctrl_active = !state.morph_ctrl_active;
    }
    label(
        painter,
        pos2(b_ped.center().x, b_ped.center().y),
        "CTRLPED",
        8.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    // OLED Display (Central Program info)
    let oled_rect = Rect::from_min_size(
        pos2(bay4.left() + 12.0, bay4.top() + 50.0),
        vec2(196.0, 96.0),
    );
    let org_txt = format!("{:?}", state.organ_model);
    let pno_txt = state
        .loaded_piano_name
        .clone()
        .unwrap_or_else(|| format!("{:?} XL", state.piano_cat));
    let syn_txt = if state.synth_osc_mode == SynthOscMode::Sample {
        state
            .loaded_sample_name
            .clone()
            .unwrap_or_else(|| "Sample".to_string())
    } else {
        format!("{:?}", state.synth_waveform)
    };
    oled(
        painter,
        oled_rect,
        &state.prog_id,
        &state.prog_title,
        &[
            format!("Org: {org_txt} 888000000"),
            format!("Pno: {pno_txt}"),
            format!("Syn: {syn_txt}"),
        ],
    );

    // Preset Library / Program Select buttons 1..8
    let prog_box = Rect::from_min_size(
        pos2(bay4.left() + 8.0, bay4.top() + 154.0),
        vec2(204.0, 110.0),
    );
    group_frame(painter, prog_box, "PROGRAM");

    for row in 0..2 {
        for col in 0..4 {
            let idx = (row * 4 + col + 1) as u8;
            let is_sel = state.active_program_idx == idx;
            let bx = prog_box.left() + 10.0 + (col as f32) * 48.0;
            let by = prog_box.top() + 18.0 + (row as f32) * 44.0;
            let b_rect = Rect::from_min_size(pos2(bx, by), vec2(40.0, 24.0));
            if nord_button(ui, b_rect, &format!("prog_btn_{idx}"), is_sel) {
                state.active_program_idx = idx;
                state.prog_id = format!("A:{idx:02}");
            }
            label(
                painter,
                pos2(b_rect.center().x, b_rect.center().y),
                &format!("{idx}"),
                9.5,
                Color32::WHITE,
                Align2::CENTER_CENTER,
            );
            led(
                painter,
                pos2(b_rect.center().x, b_rect.top() - 7.0),
                is_sel,
                LED_RED,
                LED_RED_DIM,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::run_panel_frame;

    #[test]
    fn test_render_bay_program_draws_shapes_headless() {
        let shapes = run_panel_frame(|ui, painter, rect, state, audio| {
            render_bay_program(ui, painter, rect, state, audio);
        });
        assert!(shapes > 0);
    }
}
