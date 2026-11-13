//! Front-panel bay: layer effects, delay/reverb and master compressor.

use crate::audio::{AudioCommand, AudioHandle};
use crate::panel::Stage4PanelState;
use crate::widgets::*;
use egui::{pos2, vec2, Align2, Color32, Painter, Rect, Ui};

/// Draws and drives the bay: layer effects, delay/reverb and master compressor.
pub(super) fn render_bay_fx(
    ui: &mut Ui,
    painter: &Painter,
    panel_rect: Rect,
    state: &mut Stage4PanelState,
    audio_handle: &AudioHandle,
) {
    // =========================================================================
    // BAY 6: LAYER EFFECTS (Slate: x: 1354..1552, y: 24..300)
    // =========================================================================
    let bay6 = Rect::from_min_max(
        pos2(panel_rect.left() + 1354.0, panel_rect.top() + 24.0),
        pos2(panel_rect.left() + 1552.0, panel_rect.top() + 300.0),
    );
    section_frame(painter, bay6, "LAYER EFFECTS");

    // FX Focus buttons (Organ, Piano, Synth A/B/C)
    let focus_rect = Rect::from_min_size(
        pos2(bay6.left() + 6.0, bay6.top() + 28.0),
        vec2(186.0, 24.0),
    );
    let focus_labels = ["ORG", "PNO", "SYN A", "SYN B", "SYN C"];
    for (i, &lbl) in focus_labels.iter().enumerate() {
        let is_sel = state.fx_focus == i;
        let bx = focus_rect.left() + (i as f32) * 37.0;
        let b_rect = Rect::from_min_size(pos2(bx, focus_rect.top()), vec2(34.0, 18.0));
        if nord_button(ui, b_rect, &format!("fx_foc_{i}"), is_sel) {
            state.fx_focus = i;
        }
        label(
            painter,
            pos2(b_rect.center().x, b_rect.center().y),
            lbl,
            7.5,
            Color32::WHITE,
            Align2::CENTER_CENTER,
        );
    }

    // Mod 1 & Mod 2 Knobs
    let mod1_rect =
        Rect::from_min_size(pos2(bay6.left() + 6.0, bay6.top() + 56.0), vec2(90.0, 70.0));
    group_frame(painter, mod1_rect, "MOD 1");
    if knob(
        ui,
        pos2(mod1_rect.left() + 24.0, mod1_rect.top() + 36.0),
        13.0,
        "m1_rate",
        &mut state.mod1_rate,
        0.5,
        20.0,
        "RATE",
    ) {
        audio_handle.send_command(AudioCommand::SetFxMod1 {
            kind: state.mod1_type,
            rate: state.mod1_rate,
            amount: state.mod1_amount,
        });
    }
    if knob(
        ui,
        pos2(mod1_rect.left() + 66.0, mod1_rect.top() + 36.0),
        13.0,
        "m1_amt",
        &mut state.mod1_amount,
        0.0,
        1.0,
        "AMT",
    ) {
        audio_handle.send_command(AudioCommand::SetFxMod1 {
            kind: state.mod1_type,
            rate: state.mod1_rate,
            amount: state.mod1_amount,
        });
    }

    let mod2_rect = Rect::from_min_size(
        pos2(bay6.left() + 100.0, bay6.top() + 56.0),
        vec2(90.0, 70.0),
    );
    group_frame(painter, mod2_rect, "MOD 2");
    if knob(
        ui,
        pos2(mod2_rect.left() + 24.0, mod2_rect.top() + 36.0),
        13.0,
        "m2_rate",
        &mut state.mod2_rate,
        0.2,
        10.0,
        "RATE",
    ) {
        audio_handle.send_command(AudioCommand::SetFxMod2 {
            kind: state.mod2_type,
            rate: state.mod2_rate,
            amount: state.mod2_amount,
        });
    }
    if knob(
        ui,
        pos2(mod2_rect.left() + 66.0, mod2_rect.top() + 36.0),
        13.0,
        "m2_amt",
        &mut state.mod2_amount,
        0.0,
        1.0,
        "AMT",
    ) {
        audio_handle.send_command(AudioCommand::SetFxMod2 {
            kind: state.mod2_type,
            rate: state.mod2_rate,
            amount: state.mod2_amount,
        });
    }

    // Delay & Reverb Knobs
    let delay_rect = Rect::from_min_size(
        pos2(bay6.left() + 6.0, bay6.top() + 132.0),
        vec2(90.0, 70.0),
    );
    group_frame(painter, delay_rect, "DELAY");
    if knob(
        ui,
        pos2(delay_rect.left() + 24.0, delay_rect.top() + 36.0),
        13.0,
        "dly_fb",
        &mut state.delay_feedback,
        0.0,
        0.9,
        "FEEDBK",
    ) {
        audio_handle.send_command(AudioCommand::SetFxDelay {
            enabled: state.delay_on,
            feedback: state.delay_feedback,
            mix: state.delay_mix,
        });
    }
    if knob(
        ui,
        pos2(delay_rect.left() + 66.0, delay_rect.top() + 36.0),
        13.0,
        "dly_mix",
        &mut state.delay_mix,
        0.0,
        1.0,
        "MIX",
    ) {
        audio_handle.send_command(AudioCommand::SetFxDelay {
            enabled: state.delay_on,
            feedback: state.delay_feedback,
            mix: state.delay_mix,
        });
    }

    let rev_rect = Rect::from_min_size(
        pos2(bay6.left() + 100.0, bay6.top() + 132.0),
        vec2(90.0, 70.0),
    );
    group_frame(painter, rev_rect, "REVERB");
    if knob(
        ui,
        pos2(rev_rect.center().x, rev_rect.top() + 36.0),
        15.0,
        "rev_mix",
        &mut state.reverb_mix,
        0.0,
        1.0,
        "REVERB",
    ) {
        audio_handle.send_command(AudioCommand::SetFxReverb {
            model: state.reverb_model,
            mix: state.reverb_mix,
        });
    }

    // Amp / Master Compressor
    let comp_rect = Rect::from_min_size(
        pos2(bay6.left() + 6.0, bay6.top() + 208.0),
        vec2(184.0, 62.0),
    );
    group_frame(painter, comp_rect, "COMPRESSOR & DRIVE");
    if knob(
        ui,
        pos2(comp_rect.left() + 48.0, comp_rect.top() + 32.0),
        14.0,
        "eq_drv",
        &mut state.eq_drive,
        0.0,
        1.0,
        "DRIVE",
    ) {
        audio_handle.send_command(AudioCommand::SetFxEq {
            drive: state.eq_drive,
            bass: state.eq_bass,
            mid: state.eq_mid,
            mid_freq: state.eq_mid_freq,
            treble: state.eq_treble,
        });
    }
    if knob(
        ui,
        pos2(comp_rect.left() + 136.0, comp_rect.top() + 32.0),
        14.0,
        "mst_comp",
        &mut state.master_comp,
        0.0,
        1.0,
        "COMPRESSOR",
    ) {
        audio_handle.send_command(AudioCommand::SetMasterComp(state.master_comp));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::run_panel_frame;

    #[test]
    fn test_render_bay_fx_draws_shapes_headless() {
        let shapes = run_panel_frame(|ui, painter, rect, state, audio| {
            render_bay_fx(ui, painter, rect, state, audio);
        });
        assert!(shapes > 0);
    }
}
