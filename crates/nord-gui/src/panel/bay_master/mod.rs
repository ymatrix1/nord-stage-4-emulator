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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;

    #[test]
    fn render_bay_master_paints_shapes() {
        let n = shapes_painted(render_bay_master);
        assert!(n > 50, "render_bay_master painted only {n} shapes");
    }
}
