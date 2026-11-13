//! Front-panel bay: master level, pitch/mod wheels and rotary speaker.

use crate::audio::{AudioCommand, AudioHandle};
use crate::panel::Stage4PanelState;
use crate::style::*;
use crate::widgets::*;
use egui::{pos2, vec2, Align2, Color32, Painter, Rect, Stroke, StrokeKind, Ui};
use nord_core::midi::MidiEvent;
use nord_core::organ::RotarySpeed;

/// Draws and drives the bay: master level, pitch/mod wheels and rotary speaker.
pub(super) fn render_bay_master(
    ui: &mut Ui,
    painter: &Painter,
    panel_rect: Rect,
    state: &mut Stage4PanelState,
    audio_handle: &AudioHandle,
) {
    // =========================================================================
    // BAY 1: MASTER & ROTARY (Left red zone: x: 10..120, y: 24..300)
    // =========================================================================
    let bay1 = Rect::from_min_max(
        pos2(panel_rect.left() + 8.0, panel_rect.top() + 24.0),
        pos2(panel_rect.left() + 116.0, panel_rect.top() + 300.0),
    );

    // Pitch stick & Mod wheel
    let pitch_rect =
        Rect::from_min_size(pos2(bay1.left() + 8.0, bay1.top() + 8.0), vec2(42.0, 46.0));
    let mod_rect =
        Rect::from_min_size(pos2(bay1.left() + 58.0, bay1.top() + 8.0), vec2(42.0, 46.0));
    if pitch_and_mod_wheels(
        ui,
        pitch_rect,
        mod_rect,
        &mut state.pitch_stick,
        &mut state.mod_wheel,
    ) {
        audio_handle.send_midi(MidiEvent::ControlChange {
            channel: 0,
            controller: 1, // Mod wheel
            value: (state.mod_wheel * 127.0).round() as u8,
        });
    }

    // Master Level Knob
    if knob(
        ui,
        pos2(bay1.center().x, bay1.top() + 88.0),
        16.0,
        "master_vol",
        &mut state.master_volume,
        0.0,
        1.0,
        "MASTER LEVEL",
    ) {
        audio_handle.send_command(AudioCommand::SetMasterVolume(state.master_volume));
    }

    // Rotary Speaker group frame
    let rot_rect = Rect::from_min_max(
        pos2(bay1.left() + 4.0, bay1.top() + 128.0),
        pos2(bay1.right() - 4.0, bay1.bottom() - 6.0),
    );
    painter.rect_filled(rot_rect, 4.0, Color32::from_rgb(110, 20, 24));
    painter.rect_stroke(
        rot_rect,
        4.0,
        Stroke::new(1.0_f32, GROUP_BORDER),
        StrokeKind::Inside,
    );
    label(
        painter,
        pos2(rot_rect.center().x, rot_rect.top() + 8.0),
        "ROTARY SPEAKER",
        9.5,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    if knob(
        ui,
        pos2(rot_rect.center().x, rot_rect.top() + 38.0),
        13.0,
        "rot_drive",
        &mut state.rotary_drive,
        0.0,
        1.0,
        "DRIVE",
    ) {
        audio_handle.send_command(AudioCommand::SetRotaryDrive(state.rotary_drive));
    }

    // Slow / Fast button & LEDs
    let is_fast = matches!(state.rotary_speed, RotarySpeed::Fast);
    let slow_led = pos2(rot_rect.left() + 16.0, rot_rect.top() + 74.0);
    let fast_led = pos2(rot_rect.right() - 16.0, rot_rect.top() + 74.0);
    led(painter, slow_led, !is_fast, LED_GREEN, LED_GREEN_DIM);
    led(painter, fast_led, is_fast, LED_RED, LED_RED_DIM);
    label(
        painter,
        pos2(slow_led.x, slow_led.y + 8.0),
        "SLOW",
        8.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );
    label(
        painter,
        pos2(fast_led.x, fast_led.y + 8.0),
        "FAST",
        8.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    let speed_btn = Rect::from_center_size(
        pos2(rot_rect.center().x, rot_rect.top() + 104.0),
        vec2(46.0, 18.0),
    );
    if nord_button(ui, speed_btn, "btn_rot_speed", false) {
        state.rotary_speed = if is_fast {
            RotarySpeed::Slow
        } else {
            RotarySpeed::Fast
        };
        audio_handle.send_command(AudioCommand::SetRotarySpeed(state.rotary_speed));
    }

    let stop_btn = Rect::from_center_size(
        pos2(rot_rect.center().x, rot_rect.top() + 130.0),
        vec2(46.0, 16.0),
    );
    if nord_button(ui, stop_btn, "btn_rot_stop", true) {
        state.rotary_stop = !state.rotary_stop;
        let spd = if state.rotary_stop {
            RotarySpeed::Stop
        } else {
            state.rotary_speed
        };
        audio_handle.send_command(AudioCommand::SetRotarySpeed(spd));
    }
    label(
        painter,
        pos2(stop_btn.center().x, stop_btn.center().y),
        "STOP MODE",
        8.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::run_panel_frame;

    #[test]
    fn test_render_bay_master_draws_shapes_headless() {
        let shapes = run_panel_frame(|ui, painter, rect, state, audio| {
            render_bay_master(ui, painter, rect, state, audio);
        });
        assert!(shapes > 0);
    }
}
