//! Bay 1: Master level, wheels and the Rotary Speaker box (manual p.11, p.53).
//!
//! Controls sit at their `nord_panel::layout` positions. Shift functions (ANGLE on STOP MODE,
//! CLOSE MIC on ORGAN) share the base button and are printed as legends below it.

use super::ctl::{gesture_button, led_small, mark, missing, shift_legend, tag, Face};
use super::layout_px::rect_or_empty;
use super::prelude::*;
use crate::audio::OrganEdit;
use nord_panel::behaviour::Trigger;

/// Bean tracking the missing Rotary ORGAN routing and ANGLE behaviour.
const ROTARY_BEAN: &str = "ns4-e3zs";

fn send_rotary(state: &Stage4PanelState, audio: &AudioHandle) {
    audio.send_command(AudioCommand::Organ(OrganEdit::Rotary {
        fast: matches!(state.rotary_speed, RotarySpeed::Fast),
        stop_mode: state.rotary_stop,
    }));
}

/// Draw this bay and apply its input to `state` / the audio thread.
pub(super) fn render_bay_master(
    ui: &mut Ui,
    painter: &Painter,
    panel_rect: Rect,
    state: &mut Stage4PanelState,
    audio: &AudioHandle,
) {
    let o = panel_rect.min;
    let rc = |id: &str| rect_or_empty(id, o);
    render_controllers(ui, painter, o, state, audio);

    // Rotary Speaker group frame behind the rotary controls.
    let (top, bottom) = (rc("rotary.drive"), rc("rotary.speed"));
    let frame = Rect::from_min_max(
        pos2(top.left() - 16.0, top.top() - 14.0),
        pos2(top.right() + 7.0, bottom.bottom() + 12.0),
    );
    painter.rect_filled(frame, 4.0, Color32::from_rgb(110, 20, 24));
    painter.rect_stroke(
        frame,
        4.0,
        Stroke::new(1.0_f32, GROUP_BORDER),
        StrokeKind::Inside,
    );
    label(
        painter,
        pos2(frame.center().x, frame.top() + 4.0),
        "ROTARY SPEAKER",
        5.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    let drive = rc("rotary.drive");
    if knob(
        ui,
        drive.center(),
        drive.width().min(drive.height()) * 0.42,
        "rot_drive",
        &mut state.rotary_drive,
        0.0,
        1.0,
        "",
    ) {
        audio.send_command(AudioCommand::SetRotaryDrive(state.rotary_drive));
    }
    mark(ui, "rotary.drive");
    tag(painter, drive, "DRIVE", 6.0);
    render_rotary_buttons(ui, painter, o, state, audio);
}

fn render_controllers(
    ui: &mut Ui,
    painter: &Painter,
    o: egui::Pos2,
    state: &mut Stage4PanelState,
    audio: &AudioHandle,
) {
    let level = rect_or_empty("master.level", o);
    if knob(
        ui,
        level.center(),
        level.width().min(level.height()) * 0.42,
        "master_vol",
        &mut state.master_volume,
        0.0,
        1.0,
        "",
    ) {
        audio.send_command(AudioCommand::SetMasterVolume(state.master_volume));
    }
    mark(ui, "master.level");
    tag(painter, level, "MASTER LEVEL", 7.0);

    let (pitch, wheel) = (
        rect_or_empty("master.pitch_stick", o),
        rect_or_empty("master.mod_wheel", o),
    );
    let before = state.mod_wheel;
    pitch_and_mod_wheels(
        ui,
        pitch,
        wheel,
        &mut state.pitch_stick,
        &mut state.mod_wheel,
    );
    if state.mod_wheel != before {
        audio.send_midi(MidiEvent::ControlChange {
            channel: 0,
            controller: 1, // Mod wheel
            value: (state.mod_wheel * 127.0).round() as u8,
        });
    }
    mark(ui, "master.pitch_stick");
    mark(ui, "master.mod_wheel");
    for (r, text) in [(pitch, "PITCH STICK"), (wheel, "MOD WHEEL")] {
        label(
            painter,
            pos2(r.left() - 3.0, r.center().y),
            text,
            5.5,
            Color32::WHITE,
            Align2::RIGHT_CENTER,
        );
    }
}

fn render_rotary_buttons(
    ui: &mut Ui,
    painter: &Painter,
    o: egui::Pos2,
    state: &mut Stage4PanelState,
    audio: &AudioHandle,
) {
    let rc = |id: &str| rect_or_empty(id, o);
    let dim = missing("Rotary ORGAN routing", ROTARY_BEAN);

    // ORGAN (route to rotary, dimmed) with CLOSE MIC on Shift.
    let organ = rc("rotary.organ");
    let face = Face {
        led: Some(state.organ_ui.rotary_organ),
        dim: Some(&dim),
        ..Face::black()
    };
    match gesture_button(
        ui,
        &mut state.organ_ui.trackers,
        organ,
        "rotary.organ",
        true,
        face,
    ) {
        Some(Trigger::Primary) => state.organ_ui.rotary_organ = !state.organ_ui.rotary_organ,
        Some(Trigger::Shifted) => {
            state.organ_ui.close_mic = !state.organ_ui.close_mic;
            audio.send_command(AudioCommand::Organ(OrganEdit::CloseMic(
                state.organ_ui.close_mic,
            )));
        }
        _ => {}
    }
    tag(painter, organ, "ORGAN", 7.0);
    shift_legend(
        ui,
        organ,
        "rotary.close_mic",
        "CLOSE MIC",
        Some(state.organ_ui.close_mic),
        None,
    );

    // STOP MODE with ANGLE on Shift (ANGLE dimmed).
    let stop = rc("rotary.stop_mode");
    let face = Face {
        led: Some(state.rotary_stop),
        tip: "STOP MODE: the slow position becomes the brake",
        ..Face::black()
    };
    match gesture_button(
        ui,
        &mut state.organ_ui.trackers,
        stop,
        "rotary.stop_mode",
        true,
        face,
    ) {
        Some(Trigger::Primary) => {
            state.rotary_stop = !state.rotary_stop;
            send_rotary(state, audio);
        }
        Some(Trigger::Shifted) => {
            state.organ_ui.rotary_angle_wide = !state.organ_ui.rotary_angle_wide;
        }
        _ => {}
    }
    tag(painter, stop, "STOP MODE", 7.0);
    let angle_dim = missing("Rotary ANGLE control", ROTARY_BEAN);
    shift_legend(
        ui,
        stop,
        "rotary.angle",
        "ANGLE",
        Some(state.organ_ui.rotary_angle_wide),
        Some(&angle_dim),
    );

    // SLOW/STOP - FAST button and its two LEDs.
    let fast = matches!(state.rotary_speed, RotarySpeed::Fast);
    let leds = rc("rotary.speed_leds");
    let ly = leds.center().y;
    led_small(
        painter,
        pos2(leds.left() + 2.0, ly),
        !fast,
        LED_GREEN,
        LED_GREEN_DIM,
        1.6,
    );
    led_small(
        painter,
        pos2(leds.right() - 2.0, ly),
        fast,
        LED_RED,
        LED_RED_DIM,
        1.6,
    );
    label(
        painter,
        pos2(leds.left() - 3.0, ly),
        "SLOW",
        5.0,
        Color32::WHITE,
        Align2::RIGHT_CENTER,
    );
    label(
        painter,
        pos2(leds.right() + 3.0, ly),
        "FAST",
        5.0,
        Color32::WHITE,
        Align2::LEFT_CENTER,
    );
    mark(ui, "rotary.speed_leds");
    let speed = rc("rotary.speed");
    let face = Face {
        tip: "Toggle slow/stop and fast rotor speed",
        ..Face::black()
    };
    if let Some(Trigger::Primary) = gesture_button(
        ui,
        &mut state.organ_ui.trackers,
        speed,
        "rotary.speed",
        false,
        face,
    ) {
        state.rotary_speed = if fast {
            RotarySpeed::Slow
        } else {
            RotarySpeed::Fast
        };
        send_rotary(state, audio);
    }
    label(
        painter,
        pos2(speed.center().x, speed.bottom() + 4.0),
        "SLOW/STOP - FAST",
        5.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;

    #[test]
    fn render_bay_master_paints_shapes() {
        let n = shapes_painted(render_bay_master);
        assert!(n > 50, "render_bay_master painted only {n} shapes");
    }

    #[test]
    fn rotary_command_follows_speed_and_stop_state() {
        let (audio, rx) = AudioHandle::new();
        let mut s = Stage4PanelState {
            rotary_speed: RotarySpeed::Fast,
            rotary_stop: true,
            ..Default::default()
        };
        send_rotary(&s, &audio);
        assert!(matches!(
            rx.try_recv(),
            Ok(AudioCommand::Organ(OrganEdit::Rotary {
                fast: true,
                stop_mode: true
            }))
        ));
        s.rotary_stop = false;
        send_rotary(&s, &audio);
        assert!(matches!(
            rx.try_recv(),
            Ok(AudioCommand::Organ(OrganEdit::Rotary {
                stop_mode: false,
                ..
            }))
        ));
    }
}
