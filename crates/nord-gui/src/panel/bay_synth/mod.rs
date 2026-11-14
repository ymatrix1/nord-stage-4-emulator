//! Bay 5: Synth of the front panel (Phase 6b B8e).
//!
//! Every `synth.*` control of the inventory is placed with `layout_px::rect_or_empty`. The bay is
//! split by area: `strip` (layers, faders, octave), `oled` + `oled_view` + `graphics` (display and
//! its page contents), `controls` (buttons and pots), `widgets` (shared widget code). The state
//! machines live in `panel/ui_state/synth`; engine writes go out as `AudioCommand::Synth`.

mod controls;
#[cfg(test)]
mod coverage;
mod graphics;
#[cfg(test)]
mod hint_tests;
mod oled;
mod oled_arp;
mod oled_types;
mod oled_view;
mod strip;
mod widgets;

use super::layout_px::to_px;
use super::prelude::*;
use super::ui_state::{Legacy, DIAL_COUNT};

/// Mirror of the older `synth_*` state fields (layer A) for `SynthUi::sync_legacy`.
fn legacy_of(s: &Stage4PanelState) -> Legacy {
    Legacy {
        osc_mode: s.synth_osc_mode,
        waveform: s.synth_waveform,
        filter_model: s.synth_filter_model,
        cutoff: s.synth_cutoff,
        resonance: s.synth_resonance,
        drive: s.synth_drive,
        attack: s.synth_attack,
        decay: s.synth_decay,
        release: s.synth_release,
        octave: s.synth_octave,
    }
}

fn store_legacy(s: &mut Stage4PanelState, l: Legacy) {
    s.synth_osc_mode = l.osc_mode;
    s.synth_waveform = l.waveform;
    s.synth_filter_model = l.filter_model;
    s.synth_cutoff = l.cutoff;
    s.synth_resonance = l.resonance;
    s.synth_drive = l.drive;
    s.synth_attack = l.attack;
    s.synth_decay = l.decay;
    s.synth_release = l.release;
    s.synth_octave = l.octave;
}

/// Draw this bay and apply its input to `state` / the audio thread.
pub(super) fn render_bay_synth(
    ui: &mut Ui,
    painter: &Painter,
    panel_rect: Rect,
    state: &mut Stage4PanelState,
    audio_handle: &AudioHandle,
) {
    let o = panel_rect.min;
    // A program load (or the library) may have written the older fields: adopt them first.
    let out = state.synth_ui.sync_legacy(legacy_of(state));
    store_legacy(state, out);
    state.synth_ui.begin_frame();
    // SECTION EDIT (manual p.43): every edit of this bay reaches all three layers.
    state.synth_ui.section_edit = state.program_ui.section_edit_active();
    // Sample Library: the dials browse the backup's samples; picks go out as load requests.
    state
        .synth_ui
        .samples
        .set_catalog(&state.program_ui.catalog.samples);
    state
        .synth_ui
        .samples
        .observe_loaded(state.loaded_sample_name.as_deref());

    let frame = to_px(
        nord_panel::layout::section_rect(nord_panel::Section::Synth),
        o,
    );
    section_frame(painter, frame, "SYNTH");
    crate::panel::frames::paint_frames(painter, o, nord_panel::Section::Synth);

    let mut levels = [
        state.synth_a_level,
        state.synth_b_level,
        state.synth_c_level,
    ];
    let mut active = [
        state.synth_a_active,
        state.synth_b_active,
        state.synth_c_active,
    ];
    strip::paint_strip(
        ui,
        painter,
        o,
        &mut state.synth_ui,
        &mut levels,
        &mut active,
        state.fx_focus,
        audio_handle,
        &state.telemetry,
    );
    [
        state.synth_a_level,
        state.synth_b_level,
        state.synth_c_level,
    ] = levels;
    [
        state.synth_a_active,
        state.synth_b_active,
        state.synth_c_active,
    ] = active;

    oled::paint_oled(
        painter,
        o,
        &mut state.synth_ui,
        state.loaded_sample_name.as_deref(),
        crate::panel::param_hint::active(ui.ctx()).as_deref(),
    );
    for i in 0..DIAL_COUNT {
        widgets::dial(ui, painter, o, &mut state.synth_ui, i);
    }
    controls::paint_controls(ui, painter, o, &mut state.synth_ui);

    for e in state.synth_ui.take_edits() {
        audio_handle.send_command(AudioCommand::Synth(e));
    }
    for (layer, entry) in state.synth_ui.samples.take_requests() {
        state
            .program_ui
            .requests
            .push(crate::panel::LoadRequest::Sample { entry, layer });
    }
    // Panel edits go back to the older fields so the rest of the app keeps seeing layer A.
    let out = state.synth_ui.sync_legacy(legacy_of(state));
    store_legacy(state, out);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;

    #[test]
    fn render_bay_synth_paints_shapes() {
        let n = shapes_painted(render_bay_synth);
        assert!(n > 300, "render_bay_synth painted only {n} shapes");
    }

    #[test]
    fn legacy_round_trips_through_the_state() {
        let mut s = Stage4PanelState::default();
        let l = legacy_of(&s);
        s.synth_cutoff = 1.0;
        store_legacy(&mut s, l);
        assert_eq!(s.synth_cutoff, l.cutoff);
    }
}
