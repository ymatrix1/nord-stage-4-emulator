//! 88-key interactive keybed with the green split-point LEDs (Phase 6b B8g, `ns4-jpgx`).
//!
//! Placed with the layout rectangles `keybed.keys` and `keybed.split_leds`. Mouse input plays
//! the engine through the MIDI path; with SET KEY armed the next key sets the selected split
//! point instead of only sounding.

#![cfg_attr(test, allow(clippy::field_reassign_with_default))]

use super::layout_px::rect_or_empty;
use super::prelude::*;

/// First and last MIDI note of the 88-key keybed (A0..C8).
pub const FIRST_NOTE: u8 = 21;
pub const LAST_NOTE: u8 = 108;
/// White keys of the 88-key range.
pub const WHITE_KEYS: usize = 52;

/// Black-key pitch classes.
fn is_black(midi: u8) -> bool {
    matches!(midi % 12, 1 | 3 | 6 | 8 | 10)
}

/// Black key geometry relative to the white key width.
const BLACK_W: f32 = 0.64;
const BLACK_H: f32 = 0.62;

/// Note under the point `(rel_x, rel_y)` of a keybed `white_w` per white key and `height` tall;
/// black keys sit on top.
pub fn key_at(rel_x: f32, rel_y: f32, white_w: f32, height: f32) -> Option<u8> {
    if rel_x < 0.0 || rel_y < 0.0 || rel_y > height {
        return None;
    }
    let mut w_idx = 0usize;
    for midi in FIRST_NOTE..=LAST_NOTE {
        if is_black(midi) {
            let kx = w_idx as f32 * white_w - white_w * BLACK_W * 0.5;
            if rel_x >= kx && rel_x <= kx + white_w * BLACK_W && rel_y <= height * BLACK_H {
                return Some(midi);
            }
        } else {
            w_idx += 1;
        }
    }
    let target = (rel_x / white_w).floor() as usize;
    (FIRST_NOTE..=LAST_NOTE)
        .filter(|&m| !is_black(m))
        .nth(target)
}

/// X offset (from the keybed's left edge) of the middle of `note`'s key; notes outside the
/// keybed clamp to its ends.
pub fn note_x(note: u8, white_w: f32) -> f32 {
    let note = note.clamp(FIRST_NOTE, LAST_NOTE);
    let whites_before = (FIRST_NOTE..note).filter(|&m| !is_black(m)).count() as f32;
    if is_black(note) {
        whites_before * white_w
    } else {
        (whites_before + 0.5) * white_w
    }
}

/// Key velocity of a mouse press on the on-screen keybed.
const MOUSE_VELOCITY: u8 = 100;

/// Sends a Note On of the on-screen keybed. It goes through the keyboard path
/// (`AudioHandle::send_keyboard_midi`), so Local Control Off keeps the sound silent while the
/// note is still transmitted.
fn press_note(audio_handle: &AudioHandle, note: u8) {
    audio_handle.send_keyboard_midi(MidiEvent::NoteOn {
        channel: 0,
        note,
        velocity: MOUSE_VELOCITY,
    });
}

fn release_held(state: &mut Stage4PanelState, audio_handle: &AudioHandle) {
    if let Some(old) = state.mouse_held_note.take() {
        audio_handle.send_keyboard_midi(MidiEvent::NoteOff {
            channel: 0,
            note: old,
            velocity: 0,
        });
    }
}

/// Letters printed beside the three split-point LEDs (low, middle, high).
const SPLIT_NAMES: [&str; 3] = ["L", "M", "H"];

/// 88-key keyboard with mouse playing and the three split-point LEDs.
pub(super) fn render_keybed(
    ui: &mut Ui,
    panel_rect: Rect,
    state: &mut Stage4PanelState,
    audio_handle: &AudioHandle,
) {
    let keys = rect_or_empty("keybed.keys", panel_rect.min);
    let led_strip = rect_or_empty("keybed.split_leds", panel_rect.min);
    let resp = ui.interact(keys, ui.id().with("stage4_keybed"), Sense::click_and_drag());
    let painter = ui.painter().clone();
    let white_w = keys.width() / WHITE_KEYS as f32;

    painter.rect_filled(keys.expand(2.0), 3.0, Color32::from_rgb(12, 14, 16));

    if resp.is_pointer_button_down_on() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let new = key_at(
                pos.x - keys.left(),
                pos.y - keys.top(),
                white_w,
                keys.height(),
            );
            if state.mouse_held_note != new {
                release_held(state, audio_handle);
                if let Some(n) = new {
                    state.mouse_held_note = Some(n);
                    press_note(audio_handle, n);
                    for c in state.program_ui.key_pressed(n) {
                        audio_handle.send_command(c);
                    }
                }
            }
        }
    } else {
        release_held(state, audio_handle);
    }

    let pressed = |state: &Stage4PanelState, m: u8| {
        state.midi_active_keys[m as usize] || state.mouse_held_note == Some(m)
    };
    let mut w_idx = 0usize;
    for midi in FIRST_NOTE..=LAST_NOTE {
        if is_black(midi) {
            continue;
        }
        let k = Rect::from_min_size(
            pos2(keys.left() + w_idx as f32 * white_w, keys.top() + 2.0),
            vec2(white_w - 1.0, keys.height() - 4.0),
        );
        let fill = if pressed(state, midi) {
            Color32::from_rgb(220, 240, 255)
        } else {
            Color32::from_gray(248)
        };
        painter.rect_filled(k, 2.0, fill);
        painter.rect_stroke(
            k,
            2.0,
            Stroke::new(1.0_f32, Color32::from_gray(170)),
            StrokeKind::Inside,
        );
        w_idx += 1;
    }
    w_idx = 0;
    for midi in FIRST_NOTE..=LAST_NOTE {
        if !is_black(midi) {
            w_idx += 1;
            continue;
        }
        let k = Rect::from_min_size(
            pos2(
                keys.left() + w_idx as f32 * white_w - white_w * BLACK_W * 0.5,
                keys.top() + 2.0,
            ),
            vec2(white_w * BLACK_W, keys.height() * BLACK_H),
        );
        let fill = if pressed(state, midi) {
            Color32::from_rgb(90, 110, 140)
        } else {
            Color32::from_rgb(24, 26, 30)
        };
        painter.rect_filled(k, 2.0, fill);
        painter.rect_stroke(
            k,
            2.0,
            Stroke::new(1.0_f32, Color32::BLACK),
            StrokeKind::Inside,
        );
    }

    // Split-point LEDs (green): lit for each active point while the split is on (manual p.15, p.39).
    let split = &state.program_ui.split;
    let lit = split.leds();
    for (i, note) in split.notes.into_iter().enumerate() {
        let c = pos2(keys.left() + note_x(note, white_w), led_strip.center().y);
        let (core, bloom) = (if lit[i] { LED_GREEN } else { LED_GREEN_DIM }, lit[i]);
        painter.circle_filled(c, 3.5, Color32::from_rgb(14, 16, 19));
        painter.circle_filled(c, 2.6, core);
        if bloom {
            painter.circle_stroke(c, 4.2, Stroke::new(1.2_f32, LED_GREEN.gamma_multiply(0.4)));
        }
        // L / M / H: which split point the LED belongs to (manual p.15; the Program area prints
        // the same letters over the SPLIT button).
        label(
            &painter,
            c + vec2(6.5, 0.0),
            SPLIT_NAMES[i],
            7.5,
            Color32::from_gray(235),
            Align2::LEFT_CENTER,
        );
    }
    resp.on_hover_text("88 keys: click or drag to play. SET KEY then a key moves the split point");
    state.program_ui.mark("keybed.keys");
    state.program_ui.mark("keybed.split_leds");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;

    /// Peak of `frames` rendered frames.
    fn peak(e: &mut Stage4Engine, frames: usize) -> f32 {
        (0..frames).fold(0.0f32, |m, _| {
            let [l, r] = e.render_frame();
            m.max(l.abs()).max(r.abs())
        })
    }

    /// Plays Middle C through the keybed press path and applies every queued command to `e`.
    fn press_middle_c(e: &mut Stage4Engine) {
        let (audio, rx) = AudioHandle::new();
        press_note(&audio, 60);
        let mut vol = 0.8;
        for cmd in rx.try_iter() {
            crate::audio::apply_audio_command(cmd, e, &mut vol);
        }
    }

    #[test]
    fn keybed_notes_are_keyboard_events_not_midi_in() {
        let (audio, rx) = AudioHandle::new();
        press_note(&audio, 60);
        assert!(matches!(
            rx.try_recv(),
            Ok(AudioCommand::Keyboard(MidiEvent::NoteOn { note: 60, .. }))
        ));
        let mut state = Stage4PanelState::default();
        state.mouse_held_note = Some(60);
        release_held(&mut state, &audio);
        assert!(matches!(
            rx.try_recv(),
            Ok(AudioCommand::Keyboard(MidiEvent::NoteOff { note: 60, .. }))
        ));
    }

    #[test]
    fn local_control_off_silences_the_on_screen_keybed() {
        let mut off = Stage4Engine::new(48_000.0);
        off.midi_menu.local_control = false;
        press_middle_c(&mut off);
        assert_eq!(peak(&mut off, 2048), 0.0, "keybed is disconnected");
        // MIDI In still plays the instrument.
        off.handle_midi(MidiEvent::NoteOn {
            channel: 0,
            note: 62,
            velocity: 100,
        });
        assert!(peak(&mut off, 2048) > 1e-4);
        let mut on = Stage4Engine::new(48_000.0);
        press_middle_c(&mut on);
        assert!(peak(&mut on, 2048) > 1e-4, "Local Control On sounds");
    }

    #[test]
    fn keybed_paints_88_keys_and_three_leds() {
        let n = shapes_painted(|ui, _p, rect, state, audio| render_keybed(ui, rect, state, audio));
        assert!(n >= 88 + 3 * 2, "keybed painted only {n} shapes");
    }

    #[test]
    fn split_leds_are_labelled_l_m_h() {
        let strip = rect_or_empty("keybed.split_leds", egui::pos2(0.0, 0.0));
        let (labs, _) = crate::panel::legibility_tests::labels_of(None, vec2(3300.0, 900.0));
        for name in SPLIT_NAMES {
            assert!(
                labs.iter().any(|l| l.text == name
                    && l.size >= crate::widgets::MIN_FONT_PX
                    && strip.expand(2.0).contains(l.rect.center())),
                "{name} not printed beside its LED"
            );
        }
    }

    #[test]
    fn there_are_52_white_and_36_black_keys() {
        let black = (FIRST_NOTE..=LAST_NOTE).filter(|&m| is_black(m)).count();
        assert_eq!(black, 36);
        assert_eq!(usize::from(LAST_NOTE - FIRST_NOTE) + 1 - black, WHITE_KEYS);
    }

    #[test]
    fn key_at_finds_white_and_black_keys() {
        let (w, h) = (10.0, 100.0);
        assert_eq!(
            key_at(1.0, 90.0, w, h),
            Some(21),
            "A0 is the first white key"
        );
        assert_eq!(
            key_at(10.0, 10.0, w, h),
            Some(22),
            "A#0 sits between the first two whites"
        );
        assert_eq!(
            key_at(10.0, 90.0, w, h),
            Some(23),
            "below the black key it is white B0"
        );
        assert_eq!(key_at(519.0, 90.0, w, h), Some(108), "last white key is C8");
        assert_eq!(key_at(-1.0, 10.0, w, h), None);
        assert_eq!(key_at(600.0, 10.0, w, h), None, "past the last key");
    }

    #[test]
    fn note_x_places_the_split_leds_on_their_keys() {
        let w = 10.0;
        assert_eq!(note_x(21, w), 5.0);
        let c4 = note_x(60, w);
        let f_sharp3 = note_x(54, w);
        assert!(c4 > f_sharp3);
        assert_eq!(note_x(0, w), note_x(21, w), "clamped");
        assert_eq!(
            note_x(22, w),
            10.0,
            "a black key is centred on the white-key boundary"
        );
    }

    #[test]
    fn split_leds_light_for_active_points_only() {
        let mut state = Stage4PanelState::default();
        assert_eq!(state.program_ui.split.leds(), [false; 3]);
        state.program_ui.split.on = true;
        assert_eq!(state.program_ui.split.leds(), [false, true, false]);
    }
}
