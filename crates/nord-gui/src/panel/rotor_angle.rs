//! Rotary ANGLE picker (Shift + STOP MODE, manual p.53): the list of rotor stop positions
//! `Free`, `1: Front Center`, ... drawn as an OLED style popup beside the STOP MODE button, plus
//! the wide/narrow microphone separation toggle as a last row.
//!
//! The list text is `nord_core::organ::STOP_ANGLE_LABELS` (course video ORG 10:08-10:22).

use super::piano_widgets as w;
use super::prelude::*;
use crate::audio::OrganEdit;
use nord_core::organ::{STOP_ANGLE_COUNT, STOP_ANGLE_LABELS};

const ROW_H: f32 = 11.0;
const WIDTH: f32 = 100.0;
const PAD: f32 = 4.0;
/// Title row plus the angle rows plus the mic row.
const ROWS: usize = STOP_ANGLE_LABELS.len() + 2;

/// What a click on the popup selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Pick {
    /// Stop angle index, `0` = Free.
    Angle(u8),
    /// The wide / narrow microphone toggle.
    Mic,
}

/// Popup rectangle: left of the STOP MODE button, bottom aligned with it.
pub(super) fn popup_rect(stop: Rect) -> Rect {
    let h = ROWS as f32 * ROW_H + 2.0 * PAD;
    Rect::from_min_size(
        pos2(stop.left() - WIDTH - 8.0, stop.bottom() - h),
        vec2(WIDTH, h),
    )
}

/// Row centre y of row `i` (`0` = title).
fn row_y(rect: Rect, i: usize) -> f32 {
    rect.top() + PAD + (i as f32 + 0.5) * ROW_H
}

/// Which entry a pointer position selects.
pub(super) fn pick_at(rect: Rect, pos: egui::Pos2) -> Option<Pick> {
    if !rect.contains(pos) {
        return None;
    }
    let row = ((pos.y - rect.top() - PAD) / ROW_H).floor();
    if row < 1.0 {
        return None;
    }
    let row = row as usize;
    match row {
        r if r <= STOP_ANGLE_LABELS.len() => Some(Pick::Angle((r - 1) as u8)),
        r if r == STOP_ANGLE_LABELS.len() + 1 => Some(Pick::Mic),
        _ => None,
    }
}

/// Draw the popup when it is open and apply a click.
pub(super) fn draw(
    ui: &mut Ui,
    p: &Painter,
    stop: Rect,
    state: &mut Stage4PanelState,
    audio: &AudioHandle,
) {
    if !state.organ_ui.stop_angle_open {
        return;
    }
    let rect = popup_rect(stop);
    let resp = ui.interact(rect, ui.id().with("rotor_angle_popup"), Sense::click());
    p.rect_filled(rect, 4.0_f32, OLED_BG);
    p.rect_stroke(
        rect,
        4.0_f32,
        Stroke::new(1.0_f32, Color32::from_gray(90)),
        StrokeKind::Inside,
    );
    let x = rect.left() + 6.0;
    w::small_text(
        p,
        pos2(x, row_y(rect, 0)),
        "ROTOR STOP ANGLE",
        OLED_FG,
        Align2::LEFT_CENTER,
    );
    let cur = usize::from(state.organ_ui.rotary_stop_angle.min(STOP_ANGLE_COUNT));
    for (i, label) in STOP_ANGLE_LABELS.iter().enumerate() {
        let y = row_y(rect, i + 1);
        if i == cur {
            let band = Rect::from_center_size(
                pos2(rect.center().x, y),
                vec2(rect.width() - 6.0, ROW_H - 1.0),
            );
            p.rect_stroke(
                band,
                2.0_f32,
                Stroke::new(1.0_f32, OLED_FG),
                StrokeKind::Inside,
            );
        }
        w::small_text(p, pos2(x + 2.0, y), label, OLED_FG, Align2::LEFT_CENTER);
    }
    let mic = if state.organ_ui.rotary_angle_wide {
        "Mic: Wide"
    } else {
        "Mic: Narrow"
    };
    w::small_text(
        p,
        pos2(x + 2.0, row_y(rect, STOP_ANGLE_LABELS.len() + 1)),
        mic,
        Color32::from_gray(170),
        Align2::LEFT_CENTER,
    );
    if resp.clicked() {
        let pick = resp
            .interact_pointer_pos()
            .and_then(|pos| pick_at(rect, pos));
        apply(state, audio, pick);
    }
}

/// Apply a pick: an angle choice is sent to the engine and closes the popup.
pub(super) fn apply(state: &mut Stage4PanelState, audio: &AudioHandle, pick: Option<Pick>) {
    match pick {
        Some(Pick::Angle(a)) => {
            state.organ_ui.rotary_stop_angle = a;
            state.organ_ui.stop_angle_open = false;
            audio.send_command(AudioCommand::Organ(OrganEdit::RotaryStopAngle(a)));
        }
        Some(Pick::Mic) => {
            state.organ_ui.rotary_angle_wide = !state.organ_ui.rotary_angle_wide;
            audio.send_command(AudioCommand::Organ(OrganEdit::RotaryAngle {
                wide: state.organ_ui.rotary_angle_wide,
            }));
        }
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_map_to_the_free_entry_the_six_angles_and_the_mic_toggle() {
        let stop = Rect::from_min_size(pos2(400.0, 300.0), vec2(22.0, 12.0));
        let r = popup_rect(stop);
        let at = |i: usize| pos2(r.center().x, row_y(r, i));
        assert_eq!(pick_at(r, at(0)), None, "title row");
        assert_eq!(pick_at(r, at(1)), Some(Pick::Angle(0)));
        assert_eq!(pick_at(r, at(2)), Some(Pick::Angle(1)));
        assert_eq!(pick_at(r, at(7)), Some(Pick::Angle(6)));
        assert_eq!(pick_at(r, at(8)), Some(Pick::Mic));
        assert_eq!(pick_at(r, pos2(0.0, 0.0)), None);
        assert!(r.right() < stop.left() && r.bottom() <= stop.bottom() + 0.01);
    }

    #[test]
    fn picking_an_angle_sends_the_edit_and_closes_the_popup() {
        let (audio, rx) = AudioHandle::new();
        let mut s = Stage4PanelState::default();
        s.organ_ui.stop_angle_open = true;
        apply(&mut s, &audio, Some(Pick::Angle(4)));
        assert_eq!(s.organ_ui.rotary_stop_angle, 4);
        assert!(!s.organ_ui.stop_angle_open);
        assert!(matches!(
            rx.try_recv(),
            Ok(AudioCommand::Organ(OrganEdit::RotaryStopAngle(4)))
        ));
        apply(&mut s, &audio, Some(Pick::Mic));
        assert!(!s.organ_ui.rotary_angle_wide);
        assert!(matches!(
            rx.try_recv(),
            Ok(AudioCommand::Organ(OrganEdit::RotaryAngle { wide: false }))
        ));
        apply(&mut s, &audio, None);
        assert!(rx.try_recv().is_err());
    }
}
