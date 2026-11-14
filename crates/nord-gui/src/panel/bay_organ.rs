//! Bay 2: Organ of the front panel (manual p.18-22).
//!
//! Controls sit at their `nord_panel::layout` positions. The shared strip lives in
//! `organ_strip.rs`, the drawbars in `organ_drawbars.rs`; this file has the section frame, the
//! model selector, PRESET/SYNC, vibrato/chorus and the percussion buttons.

use super::ctl::{gesture_button, led_small, shift_legend, tag, tag_fit, Face};
use super::layout_px::{rect_or_empty, to_px};
use super::organ_drawbars::render_drawbars;
use super::organ_logic::{model_index, scanner_for, vib_index, MODELS, VIB_TYPES};
use super::organ_preset::render_preset;
use super::organ_strip::render_strip;
use super::prelude::*;
use crate::audio::OrganEdit;
use nord_panel::behaviour::{cycle, Trigger};

/// Draw this bay and apply its input to `state` / the audio thread.
pub(super) fn render_bay_organ(
    ui: &mut Ui,
    painter: &Painter,
    panel_rect: Rect,
    state: &mut Stage4PanelState,
    audio_handle: &AudioHandle,
) {
    let origin = panel_rect.min;
    let frame = to_px(
        nord_panel::layout::section_rect(nord_panel::Section::Organ),
        origin,
    );
    section_frame(painter, frame, "ORGAN");
    super::frames::paint_frames(painter, origin, nord_panel::Section::Organ);
    render_strip(ui, origin, state, audio_handle);
    render_model(ui, painter, origin, state, audio_handle);
    render_preset(ui, painter, origin, state, audio_handle);
    render_vibrato(ui, painter, origin, state, audio_handle);
    render_percussion(ui, painter, origin, state, audio_handle);
    render_drawbars(ui, painter, origin, state, audio_handle);
}

fn render_model(
    ui: &mut Ui,
    painter: &Painter,
    origin: egui::Pos2,
    state: &mut Stage4PanelState,
    audio: &AudioHandle,
) {
    let btn = rect_or_empty("organ.model", origin);
    let idx = model_index(state.organ_model, state.organ_ui.b3_bass);
    let face = Face {
        tip: "ORGAN MODEL: B3, B3 Bass, Vox, Farf, Pipe 1, Pipe 2",
        ..Face::black()
    };
    if let Some(Trigger::Primary) = gesture_button(
        ui,
        &mut state.organ_ui.trackers,
        btn,
        "organ.model",
        false,
        face,
    ) {
        let (model, b3_bass) = MODELS[cycle(idx, MODELS.len(), true)];
        state.organ_model = model;
        state.organ_ui.b3_bass = b3_bass;
        audio.send_command(AudioCommand::Organ(OrganEdit::Model { model, b3_bass }));
    }
    // Above the model LEDs and their names, not over them.
    let leds = rect_or_empty("organ.model_leds", origin);
    // Six LEDs, two columns of three, with the model names beside them.
    let names = [("B3", "BASS"), ("VOX", "FARF"), ("PIPE1", "PIPE2")];
    for i in 0..MODELS.len() {
        let c = pos2(
            leds.left() + leds.width() * if i % 2 == 0 { 0.2 } else { 0.8 },
            leds.top() + leds.height() * ((i / 2) as f32 + 0.5) / 3.0,
        );
        led_small(painter, c, i == idx, LED_RED, LED_RED_DIM, 1.5);
    }
    for (row, (l, r)) in names.iter().enumerate() {
        let y = leds.top() + leds.height() * (row as f32 + 0.5) / 3.0;
        label(
            painter,
            pos2(leds.left() - 3.0, y),
            l,
            5.0,
            Color32::WHITE,
            Align2::RIGHT_CENTER,
        );
        label(
            painter,
            pos2(leds.right() + 3.0, y),
            r,
            5.0,
            Color32::WHITE,
            Align2::LEFT_CENTER,
        );
    }
    super::ctl::mark(ui, "organ.model_leds");
}

fn render_vibrato(
    ui: &mut Ui,
    painter: &Painter,
    origin: egui::Pos2,
    state: &mut Stage4PanelState,
    audio: &AudioHandle,
) {
    let focus = state.organ_ui.focus_layer;
    let (ty, _) = state.organ_vib_of(focus);
    let sel = rect_or_empty("organ.vibrato_selector", origin);
    let face = Face {
        tip: "VIB/CHORUS type V1-V3, C1-C3 (layer in focus)",
        ..Face::black()
    };
    if let Some(Trigger::Primary) = gesture_button(
        ui,
        &mut state.organ_ui.trackers,
        sel,
        "organ.vibrato_selector",
        false,
        face,
    ) {
        let next = VIB_TYPES[cycle(vib_index(ty), VIB_TYPES.len(), true)];
        let on = state.organ_vib_of(focus).1;
        state.set_organ_vib(focus, next, on);
        send_scanner(state, audio, focus);
    }
    let cur = vib_index(ty);
    for i in 0..6 {
        let c = pos2(
            sel.left() + sel.width() * ((i % 3) as f32 + 0.5) / 3.0,
            sel.top() - 5.0 - 5.0 * (1 - i / 3) as f32,
        );
        led_small(painter, c, i == cur, LED_RED, LED_RED_DIM, 1.5);
    }
    for (l, id, text) in [
        (0, "organ.vibrato_on.a", "A"),
        (1, "organ.vibrato_on.b", "B"),
    ] {
        let rect = rect_or_empty(id, origin);
        let on = state.organ_vib_of(l).1;
        let face = Face {
            led: Some(on),
            tip: "VIB/CHORUS ON for this layer",
            ..Face::black()
        };
        if let Some(Trigger::Primary) =
            gesture_button(ui, &mut state.organ_ui.trackers, rect, id, false, face)
        {
            let ty = state.organ_vib_of(l).0;
            state.set_organ_vib(l, ty, !on);
            send_scanner(state, audio, l);
        }
        tag(painter, rect, text, 6.5);
    }
}

fn send_scanner(state: &Stage4PanelState, audio: &AudioHandle, layer: usize) {
    let (ty, on) = state.organ_vib_of(layer);
    audio.send_command(AudioCommand::Organ(OrganEdit::Scanner {
        layer: layer as u8,
        mode: scanner_for(ty, on),
    }));
}

fn send_percussion(state: &Stage4PanelState, audio: &AudioHandle) {
    audio.send_command(AudioCommand::SetPercussion {
        enabled: state.perc_on,
        soft: state.perc_soft,
        fast: state.perc_fast,
        third: state.perc_third,
    });
}

fn render_percussion(
    ui: &mut Ui,
    painter: &Painter,
    origin: egui::Pos2,
    state: &mut Stage4PanelState,
    audio: &AudioHandle,
) {
    for (id, text, shift_fn) in [
        ("organ.perc_on", "ON", false),
        ("organ.perc_volume", "VOLUME SOFT", true),
        ("organ.perc_decay", "DECAY FAST", false),
        ("organ.perc_third", "HARMONIC 3RD", false),
    ] {
        let rect = rect_or_empty(id, origin);
        let lit = match id {
            "organ.perc_on" => state.perc_on,
            "organ.perc_volume" => state.perc_soft,
            "organ.perc_decay" => state.perc_fast,
            _ => state.perc_third,
        };
        let face = Face {
            led: Some(lit),
            tip: text,
            ..Face::black()
        };
        match gesture_button(ui, &mut state.organ_ui.trackers, rect, id, shift_fn, face) {
            Some(Trigger::Primary) => {
                match id {
                    "organ.perc_on" => state.perc_on = !state.perc_on,
                    "organ.perc_volume" => state.perc_soft = !state.perc_soft,
                    "organ.perc_decay" => state.perc_fast = !state.perc_fast,
                    _ => state.perc_third = !state.perc_third,
                }
                send_percussion(state, audio);
            }
            Some(Trigger::Shifted) => {
                state.organ_ui.perc_poly = !state.organ_ui.perc_poly;
                audio.send_command(AudioCommand::Organ(OrganEdit::PercPoly(
                    state.organ_ui.perc_poly,
                )));
            }
            _ => {}
        }
        // The buttons are 47 px apart: the two-word names stack instead of touching.
        if id == "organ.perc_on" {
            // The button sits 17 px under HARMONIC 3RD: its name goes to the left instead.
            let at = pos2(rect.left() - 4.0, rect.center().y);
            label(painter, at, text, 5.5, Color32::WHITE, Align2::RIGHT_CENTER);
        } else {
            tag_fit(painter, rect, text, 8.0, 44.0);
        }
        if id == "organ.perc_volume" {
            shift_legend(
                ui,
                rect,
                "organ.perc_poly",
                "POLY",
                Some(state.organ_ui.perc_poly),
                None,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;

    #[test]
    fn render_bay_organ_paints_shapes() {
        let n = shapes_painted(render_bay_organ);
        assert!(n > 300, "render_bay_organ painted only {n} shapes");
    }

    #[test]
    fn scanner_and_percussion_commands_carry_state() {
        let (audio, rx) = AudioHandle::new();
        let mut s = Stage4PanelState::default();
        s.set_organ_vib(1, nord_core::organ::ScannerMode::V2, true);
        send_scanner(&s, &audio, 1);
        assert!(matches!(
            rx.try_recv(),
            Ok(AudioCommand::Organ(OrganEdit::Scanner { layer: 1, mode }))
                if mode == nord_core::organ::ScannerMode::V2
        ));
        s.perc_soft = true;
        send_percussion(&s, &audio);
        assert!(matches!(
            rx.try_recv(),
            Ok(AudioCommand::SetPercussion { soft: true, .. })
        ));
    }
}
