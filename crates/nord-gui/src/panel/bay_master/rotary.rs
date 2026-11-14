//! The Rotary Speaker box of the Master bay (manual p.53): DRIVE, ORGAN (+ CLOSE MIC on Shift),
//! STOP MODE (+ ANGLE on Shift) and the SLOW/STOP - FAST button with its two LEDs.
//!
//! The control rectangles come from `nord_panel::layout`; the frame and the printed legends are
//! laid out here. The buttons are only ~37 px wide, so the frame grows leftwards into the empty
//! Master area (the Organ section starts right of it) and the wide legends are centred on the
//! frame, not on the button.

use super::super::ctl::{gesture_button, led_small, mark, shift_legend, tag, Face};
use super::super::layout_px::rect_or_empty;
use super::super::prelude::*;
use crate::audio::OrganEdit;
use nord_panel::behaviour::Trigger;

/// Frame extent left of the button column centre (px).
const FRAME_LEFT: f32 = 52.0;
/// Frame extent right of the button column centre (px), capped by the section edge.
const FRAME_RIGHT: f32 = 30.0;
/// Free space kept between the frame and the Master / Organ section boundary (px).
const SECTION_GAP: f32 = 3.0;
/// Space from the frame top to the DRIVE knob top (room for title and the DRIVE name).
const TITLE_ROOM: f32 = 34.0;

pub(super) fn send_rotary(state: &Stage4PanelState, audio: &AudioHandle) {
    audio.send_command(AudioCommand::Organ(OrganEdit::Rotary {
        fast: matches!(state.rotary_speed, RotarySpeed::Fast),
        stop_mode: state.rotary_stop,
    }));
}

/// Rectangle of the Rotary Speaker group frame for a panel at `o`.
pub(super) fn frame_rect(o: egui::Pos2) -> Rect {
    let (top, bottom) = (
        rect_or_empty("rotary.drive", o),
        rect_or_empty("rotary.speed", o),
    );
    let cx = rect_or_empty("rotary.organ", o).center().x;
    let section = super::super::view::section_px(nord_panel::Section::Master, o);
    let right = (cx + FRAME_RIGHT).min(section.right() - SECTION_GAP);
    Rect::from_min_max(
        pos2(cx - FRAME_LEFT, top.top() - TITLE_ROOM),
        pos2(right, bottom.bottom() + 12.0),
    )
}

/// Draw the Rotary Speaker box and apply its input.
pub(super) fn render_rotary(
    ui: &mut Ui,
    painter: &Painter,
    o: egui::Pos2,
    state: &mut Stage4PanelState,
    audio: &AudioHandle,
) {
    let frame = frame_rect(o);
    #[cfg(test)]
    crate::widgets::probe::record(
        ui.ctx(),
        crate::widgets::probe::Kind::GroupInside,
        frame,
        "ROTARY SPEAKER",
    );
    painter.rect_filled(frame, 4.0, Color32::from_rgb(110, 20, 24));
    painter.rect_stroke(
        frame,
        4.0,
        Stroke::new(1.0_f32, GROUP_BORDER),
        StrokeKind::Inside,
    );
    label_fit(
        painter,
        pos2(frame.center().x, frame.top() + 6.0),
        "ROTARY SPEAKER",
        5.0,
        frame.width() - 6.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    let drive = rect_or_empty("rotary.drive", o);
    if knob(
        ui,
        drive.center(),
        drive.width().min(drive.height()) * 0.42,
        "rotary.drive",
        &mut state.rotary_drive,
        0.0,
        1.0,
        "",
    ) {
        audio.send_command(AudioCommand::SetRotaryDrive(state.rotary_drive));
    }
    mark(ui, "rotary.drive");
    tag(painter, drive, "DRIVE", 6.0);
    render_rotary_buttons(ui, painter, o, frame, state, audio);
}

fn render_rotary_buttons(
    ui: &mut Ui,
    painter: &Painter,
    o: egui::Pos2,
    frame: Rect,
    state: &mut Stage4PanelState,
    audio: &AudioHandle,
) {
    let rc = |id: &str| rect_or_empty(id, o);

    // ORGAN (route the organ through the rotary speaker) with CLOSE MIC on Shift.
    let organ = rc("rotary.organ");
    let face = Face {
        led: Some(state.organ_ui.rotary_organ),
        tip: "ORGAN: route the Organ through the rotary speaker. Shift+tap: CLOSE MIC",
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
        Some(Trigger::Primary) => {
            state.organ_ui.rotary_organ = !state.organ_ui.rotary_organ;
            audio.send_command(AudioCommand::Organ(OrganEdit::RotaryOrgan(
                state.organ_ui.rotary_organ,
            )));
        }
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

    // STOP MODE with the stop ANGLE picker (and the mic angle toggle) on Shift.
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
            state.organ_ui.stop_angle_open = !state.organ_ui.stop_angle_open;
        }
        _ => {}
    }
    tag(painter, stop, "STOP MODE", 7.0);
    shift_legend(
        ui,
        stop,
        "rotary.stop_angle",
        "ANGLE",
        Some(state.organ_ui.rotary_stop_angle != 0),
        None,
    );
    // The mic separation toggle is a row of the popup; the id is painted with the legend.
    mark(ui, "rotary.angle");
    super::super::rotor_angle::draw(ui, painter, stop, state, audio);
    render_speed(ui, painter, o, frame, state, audio);
}

/// SLOW/STOP - FAST button, its two LEDs and legends.
fn render_speed(
    ui: &mut Ui,
    painter: &Painter,
    o: egui::Pos2,
    frame: Rect,
    state: &mut Stage4PanelState,
    audio: &AudioHandle,
) {
    let fast = matches!(state.rotary_speed, RotarySpeed::Fast);
    let leds = rect_or_empty("rotary.speed_leds", o);
    let ly = leds.center().y;
    // Hardware order: LED, then its name. The row rectangle spans both pairs.
    let (lx_slow, lx_fast) = (leds.left() - 0.4, leds.center().x + 3.0);
    // LEDs show the real rotor state (a pedal can change it); `fast` stays the UI toggle.
    let (led_slow, led_fast) = (
        state.telemetry.rotary_slow(fast),
        state.telemetry.rotary_fast(fast),
    );
    led_small(
        painter,
        pos2(lx_slow, ly),
        led_slow,
        LED_GREEN,
        LED_GREEN_DIM,
        1.6,
    );
    led_small(
        painter,
        pos2(lx_fast, ly),
        led_fast,
        LED_RED,
        LED_RED_DIM,
        1.6,
    );
    label(
        painter,
        pos2(lx_slow + 4.5, ly),
        "SLOW",
        5.0,
        Color32::WHITE,
        Align2::LEFT_CENTER,
    );
    label(
        painter,
        pos2(lx_fast + 4.5, ly),
        "FAST",
        5.0,
        Color32::WHITE,
        Align2::LEFT_CENTER,
    );
    mark(ui, "rotary.speed_leds");
    let speed = rect_or_empty("rotary.speed", o);
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
    label_fit(
        painter,
        pos2(frame.center().x, speed.bottom() + 6.5),
        "SLOW/STOP - FAST",
        5.0,
        frame.width() - 4.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn frame_is_wide_enough_for_its_labels_and_stays_in_the_master_section() {
        let o = pos2(0.0, 0.0);
        let f = frame_rect(o);
        let sec = super::super::super::view::section_px(nord_panel::Section::Master, o);
        assert!(f.width() >= 70.0, "frame {f:?}");
        assert!(sec.contains_rect(f), "frame {f:?} leaves {sec:?}");
        let speed = rect_or_empty("rotary.speed", o);
        assert!(f.contains_rect(speed.expand(2.0)));
    }
}
