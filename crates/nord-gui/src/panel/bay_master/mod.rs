//! Bay 1: Master level, wheels and the Rotary Speaker box (manual p.11, p.53).
//!
//! Controls sit at their `nord_panel::layout` positions. Shift functions (ANGLE on STOP MODE,
//! CLOSE MIC on ORGAN) share the base button and are printed as legends below it.

mod rotary;

use super::ctl::{mark, tag};
use super::layout_px::rect_or_empty;
use super::prelude::*;

/// Draw this bay and apply its input to `state` / the audio thread.
pub(super) fn render_bay_master(
    ui: &mut Ui,
    painter: &Painter,
    panel_rect: Rect,
    state: &mut Stage4PanelState,
    audio: &AudioHandle,
) {
    let o = panel_rect.min;
    render_controllers(ui, painter, o, state, audio);
    rotary::render_rotary(ui, painter, o, state, audio);
}

/// 14-bit pitch bend (centre 8192) of a pitch stick position `-1.0..=1.0`.
/// VERIFIED: MIDI 1.0 pitch bend is 0..=16383 with 8192 at rest; checked by
/// `pitch_bend_value_spans_the_14_bit_range`.
fn pitch_bend_value(stick: f32) -> u16 {
    (8192.0 + stick.clamp(-1.0, 1.0) * 8191.0).round() as u16
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
        "master.level",
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
    let (before, stick_before) = (state.mod_wheel, state.pitch_stick);
    pitch_and_mod_wheels(
        ui,
        pitch,
        wheel,
        &mut state.pitch_stick,
        &mut state.mod_wheel,
    );
    if state.pitch_stick != stick_before {
        audio.send_keyboard_midi(MidiEvent::PitchBend {
            channel: 0,
            value: pitch_bend_value(state.pitch_stick),
        });
    }
    if state.mod_wheel != before {
        audio.send_keyboard_midi(MidiEvent::ControlChange {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;

    #[test]
    fn pitch_bend_value_spans_the_14_bit_range() {
        assert_eq!(pitch_bend_value(0.0), 8192);
        assert_eq!(pitch_bend_value(1.0), 16383);
        assert_eq!(pitch_bend_value(-1.0), 1);
        assert_eq!(pitch_bend_value(5.0), 16383, "clamped");
    }

    #[test]
    fn render_bay_master_paints_shapes() {
        let n = shapes_painted(render_bay_master);
        assert!(n > 50, "render_bay_master painted only {n} shapes");
    }
}
