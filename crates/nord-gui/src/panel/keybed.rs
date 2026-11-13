//! 88-key interactive keybed.

use super::prelude::*;

/// 88-Key Piano Keyboard with interactive mouse clicking and Triple Split-Point visual markers.
pub(super) fn render_keybed(
    ui: &mut Ui,
    panel_rect: Rect,
    state: &mut Stage4PanelState,
    audio_handle: &AudioHandle,
) {
    let keybed_rect = Rect::from_min_size(
        pos2(panel_rect.left() + 10.0, panel_rect.top() + 310.0),
        vec2(panel_rect.width() - 20.0, 100.0),
    );
    let resp = ui.interact(
        keybed_rect,
        ui.id().with("stage4_keybed"),
        Sense::click_and_drag(),
    );
    let painter = ui.painter().clone();

    // Dark chassis lip behind keys
    painter.rect_filled(keybed_rect.expand(2.0), 3.0, Color32::from_rgb(12, 14, 16));

    // 88 keys: A0 (MIDI 21) .. C8 (MIDI 108)
    // 52 natural (white) keys
    let num_white = 52;
    let white_width = keybed_rect.width() / num_white as f32;

    // Mouse Interaction Tracking (fixes sticking notes!)
    if resp.is_pointer_button_down_on() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let rel_x = pos.x - keybed_rect.left();
            let mut clicked_midi: Option<u8> = None;

            // Check black keys first (top layer)
            let mut w_idx = 0;
            for midi in 21u8..=108u8 {
                let is_black = matches!(midi % 12, 1 | 3 | 6 | 8 | 10);
                if is_black {
                    let kx = (w_idx as f32) * white_width - (white_width * 0.32);
                    let kw = white_width * 0.64;
                    let kh = keybed_rect.height() * 0.62;
                    let rel_y = pos.y - keybed_rect.top();
                    if rel_x >= kx && rel_x <= (kx + kw) && rel_y <= kh {
                        clicked_midi = Some(midi);
                        break;
                    }
                } else {
                    w_idx += 1;
                }
            }

            // If not on black key, find white key
            if clicked_midi.is_none() {
                let target_w = (rel_x / white_width).floor() as usize;
                let mut curr_w = 0;
                for midi in 21u8..=108u8 {
                    let is_black = matches!(midi % 12, 1 | 3 | 6 | 8 | 10);
                    if !is_black {
                        if curr_w == target_w {
                            clicked_midi = Some(midi);
                            break;
                        }
                        curr_w += 1;
                    }
                }
            }

            // Update held note if changed
            if state.mouse_held_note != clicked_midi {
                if let Some(old_note) = state.mouse_held_note.take() {
                    audio_handle.send_midi(MidiEvent::NoteOff {
                        channel: 0,
                        note: old_note,
                        velocity: 0,
                    });
                }
                if let Some(new_note) = clicked_midi {
                    state.mouse_held_note = Some(new_note);
                    audio_handle.send_midi(MidiEvent::NoteOn {
                        channel: 0,
                        note: new_note,
                        velocity: 100,
                    });
                }
            }
        }
    } else {
        // Pointer released: release any mouse-held note
        if let Some(old_note) = state.mouse_held_note.take() {
            audio_handle.send_midi(MidiEvent::NoteOff {
                channel: 0,
                note: old_note,
                velocity: 0,
            });
        }
    }

    // Render 52 White keys
    let mut w_idx = 0;
    for midi in 21u8..=108u8 {
        let is_black = matches!(midi % 12, 1 | 3 | 6 | 8 | 10);
        if !is_black {
            let kx = keybed_rect.left() + (w_idx as f32) * white_width;
            let k_rect = Rect::from_min_size(
                pos2(kx, keybed_rect.top() + 2.0),
                vec2(white_width - 1.0, keybed_rect.height() - 4.0),
            );
            let is_pressed =
                state.midi_active_keys[midi as usize] || state.mouse_held_note == Some(midi);
            let fill = if is_pressed {
                Color32::from_rgb(220, 240, 255)
            } else {
                Color32::from_gray(248)
            };
            painter.rect_filled(k_rect, 2.0, fill);
            painter.rect_stroke(
                k_rect,
                2.0,
                Stroke::new(1.0_f32, Color32::from_gray(170)),
                StrokeKind::Inside,
            );
            w_idx += 1;
        }
    }

    // Render 36 Black keys overlay
    w_idx = 0;
    for midi in 21u8..=108u8 {
        let is_black = matches!(midi % 12, 1 | 3 | 6 | 8 | 10);
        if is_black {
            let kx = keybed_rect.left() + (w_idx as f32) * white_width - (white_width * 0.32);
            let k_rect = Rect::from_min_size(
                pos2(kx, keybed_rect.top() + 2.0),
                vec2(white_width * 0.64, keybed_rect.height() * 0.62),
            );
            let is_pressed =
                state.midi_active_keys[midi as usize] || state.mouse_held_note == Some(midi);
            let fill = if is_pressed {
                Color32::from_rgb(90, 110, 140)
            } else {
                Color32::from_rgb(24, 26, 30)
            };
            painter.rect_filled(k_rect, 2.0, fill);
            painter.rect_stroke(
                k_rect,
                2.0,
                Stroke::new(1.0_f32, Color32::BLACK),
                StrokeKind::Inside,
            );
        } else {
            w_idx += 1;
        }
    }

    // Split point red LEDs on keybed lip (F3/F#3 = MIDI 53, C5/C#5 = MIDI 72)
    let split_led1 = pos2(
        keybed_rect.left() + 20.0 * white_width,
        keybed_rect.top() + 2.0,
    );
    let split_led2 = pos2(
        keybed_rect.left() + 32.0 * white_width,
        keybed_rect.top() + 2.0,
    );
    painter.circle_filled(split_led1, 2.5, LED_RED);
    painter.circle_filled(split_led2, 2.5, LED_RED);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;

    #[test]
    fn keybed_paints_88_keys() {
        let n = shapes_painted(|ui, _p, rect, state, audio| render_keybed(ui, rect, state, audio));
        assert!(n >= 88, "keybed painted only {n} shapes");
    }
}
