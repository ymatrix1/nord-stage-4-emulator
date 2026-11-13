//! Bay 3: Piano of the front panel.

use super::prelude::*;

/// Draw this bay and apply its input to `state` / the audio thread.
pub(super) fn render_bay_piano(
    ui: &mut Ui,
    painter: &Painter,
    panel_rect: Rect,
    state: &mut Stage4PanelState,
    audio_handle: &AudioHandle,
) {
    // =========================================================================
    // BAY 3: PIANO SECTION (Slate: x: 494..788, y: 24..300)
    // =========================================================================
    let bay3 = Rect::from_min_max(
        pos2(panel_rect.left() + 494.0, panel_rect.top() + 24.0),
        pos2(panel_rect.left() + 788.0, panel_rect.top() + 300.0),
    );
    section_frame(painter, bay3, "PIANO SECTION");

    // Piano Layer A & B Strips
    let pfader_a = Rect::from_min_size(
        pos2(bay3.left() + 12.0, bay3.top() + 32.0),
        vec2(32.0, 160.0),
    );
    if layer_strip(
        ui,
        pfader_a,
        "pno_a",
        &mut state.piano_a_level,
        &mut state.piano_a_active,
    ) {
        audio_handle.send_command(AudioCommand::SetPianoLevel(if state.piano_a_active {
            state.piano_a_level
        } else {
            0.0
        }));
        audio_handle.send_command(AudioCommand::SetLayerActive(2, state.piano_a_active));
    }
    label(
        painter,
        pos2(pfader_a.center().x, pfader_a.bottom() + 10.0),
        "A",
        11.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    let pfader_b = Rect::from_min_size(
        pos2(bay3.left() + 48.0, bay3.top() + 32.0),
        vec2(32.0, 160.0),
    );
    if layer_strip(
        ui,
        pfader_b,
        "pno_b",
        &mut state.piano_b_level,
        &mut state.piano_b_active,
    ) {
        audio_handle.send_command(AudioCommand::SetPianoBLevel(if state.piano_b_active {
            state.piano_b_level
        } else {
            0.0
        }));
        audio_handle.send_command(AudioCommand::SetLayerActive(3, state.piano_b_active));
    }
    label(
        painter,
        pos2(pfader_b.center().x, pfader_b.bottom() + 10.0),
        "B",
        11.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    // Acoustics: Soft Release, String Res, Pedal Noise
    let ac_rect = Rect::from_min_size(
        pos2(bay3.left() + 90.0, bay3.top() + 32.0),
        vec2(86.0, 96.0),
    );
    group_frame(painter, ac_rect, "ACOUSTICS");

    led(
        painter,
        pos2(ac_rect.left() + 12.0, ac_rect.top() + 24.0),
        state.string_res,
        LED_RED,
        LED_RED_DIM,
    );
    label(
        painter,
        pos2(ac_rect.left() + 22.0, ac_rect.top() + 24.0),
        "STRING RES",
        8.5,
        Color32::WHITE,
        Align2::LEFT_CENTER,
    );

    led(
        painter,
        pos2(ac_rect.left() + 12.0, ac_rect.top() + 44.0),
        state.pedal_noise,
        LED_RED,
        LED_RED_DIM,
    );
    label(
        painter,
        pos2(ac_rect.left() + 22.0, ac_rect.top() + 44.0),
        "PED NOISE",
        8.5,
        Color32::WHITE,
        Align2::LEFT_CENTER,
    );

    let ac_btn = Rect::from_min_size(
        pos2(ac_rect.left() + 8.0, ac_rect.bottom() - 24.0),
        vec2(70.0, 16.0),
    );
    if nord_button(ui, ac_btn, "btn_acoustics", false) {
        state.string_res = !state.string_res;
        audio_handle.send_command(AudioCommand::SetPianoAcoustics {
            string_res: state.string_res,
            pedal_noise: state.pedal_noise,
        });
    }

    // Timbre Group (Bright / Mid / Soft / Dyno)
    let timb_rect = Rect::from_min_size(
        pos2(bay3.left() + 90.0, bay3.top() + 140.0),
        vec2(86.0, 110.0),
    );
    group_frame(painter, timb_rect, "TIMBRE");
    let timbres = [
        PianoTimbre::Off,
        PianoTimbre::Soft,
        PianoTimbre::Mid,
        PianoTimbre::Bright,
        PianoTimbre::Dyno1,
        PianoTimbre::Dyno2,
    ];
    let cur_t_idx = timbres
        .iter()
        .position(|t| *t == state.piano_timbre)
        .unwrap_or(0);
    if let Some(new_t) = triangle_matrix_selector(
        ui,
        timb_rect.shrink(6.0),
        "sel_timbre",
        &["SOFT", "MID", "BRIGHT"],
        &["OFF", "DYNO1", "DYNO2"],
        cur_t_idx,
    ) {
        state.piano_timbre = timbres[new_t % timbres.len()];
        audio_handle.send_command(AudioCommand::SetPianoTimbre(state.piano_timbre));
    }

    // Piano Select Matrix (Grand, Upright, Electric, Clav, Digital, Misc)
    let psel_rect = Rect::from_min_size(
        pos2(bay3.left() + 180.0, bay3.top() + 32.0),
        vec2(104.0, 140.0),
    );
    group_frame(painter, psel_rect, "PIANO SELECT");
    let p_cats = [
        PianoCategory::Grand,
        PianoCategory::Upright,
        PianoCategory::Electric,
        PianoCategory::Clav,
        PianoCategory::Digital,
        PianoCategory::Misc,
    ];
    let cur_cat_idx = p_cats
        .iter()
        .position(|c| *c == state.piano_cat)
        .unwrap_or(0);
    if let Some(new_cat) = triangle_matrix_selector(
        ui,
        psel_rect.shrink(4.0),
        "sel_piano_cat",
        &["GRAND", "UPRIGHT", "ELEC"],
        &["CLAV", "DIGITAL", "MISC"],
        cur_cat_idx,
    ) {
        state.piano_cat = p_cats[new_cat % p_cats.len()];
        audio_handle.send_command(AudioCommand::SetPianoCategory(state.piano_cat));
    }

    // Model knob
    let mut model_dummy = 0.5;
    knob(
        ui,
        pos2(bay3.left() + 232.0, bay3.top() + 218.0),
        15.0,
        "pno_model",
        &mut model_dummy,
        0.0,
        1.0,
        "MODEL LIST",
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;

    #[test]
    fn render_bay_piano_paints_shapes() {
        let n = shapes_painted(render_bay_piano);
        assert!(n > 50, "render_bay_piano painted only {n} shapes");
    }
}
