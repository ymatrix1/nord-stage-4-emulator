//! PRESET / Drawbar Live and SYNC of the Organ bay (manual p.21).
//!
//! PRESET lights while the stored preset plays; a tap switches to Drawbar Live (the physical
//! drawbars play, the ladders go dark). Shift + PRESET is SYNC: the physical positions are copied
//! into the stored preset. Engine side: `OrganEdit::{DrawbarLive, LiveDrawbars, SyncPreset}`
//! (host `organz_drawbar_live_{n}`, `organz_live1..9_{n}`, `organz_sync_{n}`).

use super::ctl::{gesture_button, shift_legend, tag, Face};
use super::layout_px::rect_or_empty;
use super::prelude::*;
use crate::audio::OrganEdit;
use nord_panel::behaviour::Trigger;

/// Preset <-> Drawbar Live switch. Entering Live first sends the physical positions so the
/// engine plays what the sliders show.
pub(super) fn toggle_live(state: &mut Stage4PanelState, audio: &AudioHandle) {
    let live = !state.organ_ui.drawbar_live;
    state.organ_ui.drawbar_live = live;
    audio.send_command(AudioCommand::Organ(OrganEdit::LiveDrawbars(
        state.organ_ui.live_bars,
    )));
    audio.send_command(AudioCommand::Organ(OrganEdit::DrawbarLive(live)));
}

/// SYNC: the stored preset takes the physical positions (engine and panel).
pub(super) fn sync_preset(state: &mut Stage4PanelState, audio: &AudioHandle) {
    state.organ_drawbars = state.organ_ui.live_bars;
    audio.send_command(AudioCommand::Organ(OrganEdit::LiveDrawbars(
        state.organ_ui.live_bars,
    )));
    audio.send_command(AudioCommand::Organ(OrganEdit::SyncPreset));
    audio.send_command(AudioCommand::SetDrawbars(state.organ_drawbars));
}

pub(super) fn render_preset(
    ui: &mut Ui,
    painter: &Painter,
    origin: egui::Pos2,
    state: &mut Stage4PanelState,
    audio: &AudioHandle,
) {
    let btn = rect_or_empty("organ.preset", origin);
    let face = Face {
        led: Some(!state.organ_ui.drawbar_live),
        tip: "PRESET lit: the stored preset plays. Tap: Drawbar Live (physical drawbars). Shift+tap: SYNC",
        ..Face::black()
    };
    match gesture_button(
        ui,
        &mut state.organ_ui.trackers,
        btn,
        "organ.preset",
        true,
        face,
    ) {
        Some(Trigger::Primary) => toggle_live(state, audio),
        Some(Trigger::Shifted) => sync_preset(state, audio),
        _ => {}
    }
    tag(painter, btn, "PRESET", 8.0);
    shift_legend(ui, btn, "organ.sync", "SYNC", None, None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;

    #[test]
    fn preset_paints_with_its_sync_legend() {
        let n = shapes_painted(|ui, p, rect, s, a| render_preset(ui, p, rect.min, s, a));
        assert!(n > 5, "painted only {n} shapes");
    }

    #[test]
    fn entering_live_sends_the_physical_bars_then_the_mode() {
        let (audio, rx) = AudioHandle::new();
        let mut s = Stage4PanelState::default();
        s.organ_ui.live_bars = [1, 2, 3, 4, 5, 6, 7, 8, 0];
        toggle_live(&mut s, &audio);
        assert!(s.organ_ui.drawbar_live);
        assert!(matches!(
            rx.try_recv(),
            Ok(AudioCommand::Organ(OrganEdit::LiveDrawbars([
                1, 2, 3, 4, 5, 6, 7, 8, 0
            ])))
        ));
        assert!(matches!(
            rx.try_recv(),
            Ok(AudioCommand::Organ(OrganEdit::DrawbarLive(true)))
        ));
        toggle_live(&mut s, &audio);
        assert!(!s.organ_ui.drawbar_live);
    }

    #[test]
    fn sync_copies_the_physical_bars_into_the_panel_preset() {
        let (audio, rx) = AudioHandle::new();
        let mut s = Stage4PanelState::default();
        s.organ_ui.drawbar_live = true;
        s.organ_ui.live_bars = [0, 0, 0, 0, 8, 0, 0, 0, 0];
        sync_preset(&mut s, &audio);
        assert_eq!(s.organ_drawbars, [0, 0, 0, 0, 8, 0, 0, 0, 0]);
        let cmds: Vec<_> = rx.try_iter().collect();
        assert!(cmds
            .iter()
            .any(|c| matches!(c, AudioCommand::Organ(OrganEdit::SyncPreset))));
        assert!(cmds
            .iter()
            .any(|c| matches!(c, AudioCommand::SetDrawbars(d) if d[4] == 8)));
    }
}
