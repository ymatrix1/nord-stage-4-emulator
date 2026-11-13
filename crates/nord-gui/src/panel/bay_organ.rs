//! Bay 2: Organ of the front panel.

use super::prelude::*;

/// Draw this bay and apply its input to `state` / the audio thread.
pub(super) fn render_bay_organ(
    ui: &mut Ui,
    painter: &Painter,
    panel_rect: Rect,
    state: &mut Stage4PanelState,
    audio_handle: &AudioHandle,
) {
    // =========================================================================
    // BAY 2: ORGAN SECTION (Slate: x: 122..488, y: 24..300)
    // =========================================================================
    let bay2 = Rect::from_min_max(
        pos2(panel_rect.left() + 122.0, panel_rect.top() + 24.0),
        pos2(panel_rect.left() + 488.0, panel_rect.top() + 300.0),
    );
    section_frame(painter, bay2, "ORGAN SECTION");

    // Organ Layer A & B Strips
    let fader_a = Rect::from_min_size(
        pos2(bay2.left() + 12.0, bay2.top() + 32.0),
        vec2(32.0, 160.0),
    );
    if layer_strip(
        ui,
        fader_a,
        "org_a",
        &mut state.organ_a_level,
        &mut state.organ_a_active,
    ) {
        audio_handle.send_command(AudioCommand::SetOrganLevel(if state.organ_a_active {
            state.organ_a_level
        } else {
            0.0
        }));
        audio_handle.send_command(AudioCommand::SetLayerActive(0, state.organ_a_active));
    }
    label(
        painter,
        pos2(fader_a.center().x, fader_a.bottom() + 10.0),
        "A",
        11.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    let fader_b = Rect::from_min_size(
        pos2(bay2.left() + 48.0, bay2.top() + 32.0),
        vec2(32.0, 160.0),
    );
    if layer_strip(
        ui,
        fader_b,
        "org_b",
        &mut state.organ_b_level,
        &mut state.organ_b_active,
    ) {
        audio_handle.send_command(AudioCommand::SetOrganBLevel(if state.organ_b_active {
            state.organ_b_level
        } else {
            0.0
        }));
        audio_handle.send_command(AudioCommand::SetLayerActive(1, state.organ_b_active));
    }
    label(
        painter,
        pos2(fader_b.center().x, fader_b.bottom() + 10.0),
        "B",
        11.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    // Organ Model Selector (B3, Vox, Farf, Pipe1, Pipe2)
    let model_rect = Rect::from_min_size(
        pos2(bay2.left() + 90.0, bay2.top() + 38.0),
        vec2(84.0, 80.0),
    );
    let org_models = [
        OrganModel::B3,
        OrganModel::Vox,
        OrganModel::Farfisa,
        OrganModel::Pipe1,
        OrganModel::Pipe2,
    ];
    let cur_m_idx = org_models
        .iter()
        .position(|m| *m == state.organ_model)
        .unwrap_or(0);
    if let Some(new_idx) = triangle_matrix_selector(
        ui,
        model_rect,
        "sel_org_model",
        &["B3", "VOX", "FARF"],
        &["PIPE1", "PIPE2"],
        cur_m_idx,
    ) {
        state.organ_model = org_models[new_idx % org_models.len()];
        audio_handle.send_command(AudioCommand::SetOrganModel(state.organ_model));
    }
    label(
        painter,
        pos2(model_rect.center().x, bay2.top() + 29.0),
        "ORGAN MODEL",
        9.0,
        Color32::from_gray(230),
        Align2::CENTER_CENTER,
    );

    // Vib / Chorus Box
    let vib_rect = Rect::from_min_size(
        pos2(bay2.left() + 182.0, bay2.top() + 38.0),
        vec2(84.0, 80.0),
    );
    let sc_modes = [
        ScannerMode::V1,
        ScannerMode::V2,
        ScannerMode::V3,
        ScannerMode::C1,
        ScannerMode::C2,
        ScannerMode::C3,
    ];
    let cur_sc_idx = sc_modes
        .iter()
        .position(|m| *m == state.scanner_mode)
        .unwrap_or(5);
    if let Some(new_sc) = triangle_matrix_selector(
        ui,
        vib_rect,
        "sel_vib",
        &["V1", "V2", "V3"],
        &["C1", "C2", "C3"],
        cur_sc_idx,
    ) {
        state.scanner_mode = sc_modes[new_sc % sc_modes.len()];
        audio_handle.send_command(AudioCommand::SetScannerMode(state.scanner_mode));
    }
    label(
        painter,
        pos2(vib_rect.center().x, bay2.top() + 29.0),
        "VIB/CHORUS",
        9.0,
        Color32::from_gray(230),
        Align2::CENTER_CENTER,
    );

    // B3 Percussion Box
    let perc_rect = Rect::from_min_size(
        pos2(bay2.left() + 274.0, bay2.top() + 38.0),
        vec2(84.0, 80.0),
    );
    painter.rect_filled(perc_rect, 3.0, Color32::from_rgb(20, 24, 28));
    painter.rect_stroke(
        perc_rect,
        3.0,
        Stroke::new(1.0_f32, Color32::from_gray(60)),
        StrokeKind::Inside,
    );
    label(
        painter,
        pos2(perc_rect.center().x, bay2.top() + 29.0),
        "B3 PERCUSSION",
        9.0,
        Color32::from_gray(230),
        Align2::CENTER_CENTER,
    );

    let p_on_rect = Rect::from_min_size(
        pos2(perc_rect.left() + 6.0, perc_rect.top() + 14.0),
        vec2(32.0, 16.0),
    );
    if nord_button(ui, p_on_rect, "btn_perc_on", state.perc_on) {
        state.perc_on = !state.perc_on;
        audio_handle.send_command(AudioCommand::SetPercussion {
            enabled: state.perc_on,
            soft: state.perc_soft,
            fast: state.perc_fast,
            third: state.perc_third,
        });
    }
    label(
        painter,
        pos2(p_on_rect.center().x, p_on_rect.center().y),
        "ON",
        8.5,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    let p_soft_rect = Rect::from_min_size(
        pos2(perc_rect.right() - 38.0, perc_rect.top() + 14.0),
        vec2(32.0, 16.0),
    );
    if nord_button(ui, p_soft_rect, "btn_perc_soft", state.perc_soft) {
        state.perc_soft = !state.perc_soft;
        audio_handle.send_command(AudioCommand::SetPercussion {
            enabled: state.perc_on,
            soft: state.perc_soft,
            fast: state.perc_fast,
            third: state.perc_third,
        });
    }
    label(
        painter,
        pos2(p_soft_rect.center().x, p_soft_rect.center().y),
        "SOFT",
        8.5,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    let p_fast_rect = Rect::from_min_size(
        pos2(perc_rect.left() + 6.0, perc_rect.top() + 38.0),
        vec2(32.0, 16.0),
    );
    if nord_button(ui, p_fast_rect, "btn_perc_fast", state.perc_fast) {
        state.perc_fast = !state.perc_fast;
        audio_handle.send_command(AudioCommand::SetPercussion {
            enabled: state.perc_on,
            soft: state.perc_soft,
            fast: state.perc_fast,
            third: state.perc_third,
        });
    }
    label(
        painter,
        pos2(p_fast_rect.center().x, p_fast_rect.center().y),
        "FAST",
        8.5,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    let p_3rd_rect = Rect::from_min_size(
        pos2(perc_rect.right() - 38.0, perc_rect.top() + 38.0),
        vec2(32.0, 16.0),
    );
    if nord_button(ui, p_3rd_rect, "btn_perc_3rd", state.perc_third) {
        state.perc_third = !state.perc_third;
        audio_handle.send_command(AudioCommand::SetPercussion {
            enabled: state.perc_on,
            soft: state.perc_soft,
            fast: state.perc_fast,
            third: state.perc_third,
        });
    }
    label(
        painter,
        pos2(p_3rd_rect.center().x, p_3rd_rect.center().y),
        "3RD",
        8.5,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    // 9 Physical Drawbars (16', 5 1/3', 8', 4', 2 2/3', 2', 1 3/5', 1 1/3', 1')
    let db_footages = [
        "16'", "5 1/3'", "8'", "4'", "2 2/3'", "2'", "1 3/5'", "1 1/3'", "1'",
    ];
    let mut db_changed = false;
    for (i, &footage) in db_footages.iter().enumerate() {
        let is_white = matches!(i, 2 | 3 | 5 | 8);
        let x = bay2.left() + 94.0 + (i as f32) * 29.5;
        let db_rect = Rect::from_min_size(pos2(x, bay2.top() + 128.0), vec2(27.0, 128.0));
        if drawbar(
            ui,
            db_rect,
            &format!("db_{i}"),
            &mut state.organ_drawbars[i],
            is_white,
        ) {
            db_changed = true;
        }
        label(
            painter,
            pos2(db_rect.center().x, db_rect.bottom() + 8.0),
            footage,
            8.5,
            Color32::WHITE,
            Align2::CENTER_CENTER,
        );
    }
    if db_changed {
        audio_handle.send_command(AudioCommand::SetDrawbars(state.organ_drawbars));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;

    #[test]
    fn render_bay_organ_paints_shapes() {
        let n = shapes_painted(render_bay_organ);
        assert!(n > 50, "render_bay_organ painted only {n} shapes");
    }
}
