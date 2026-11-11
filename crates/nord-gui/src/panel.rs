//! Full 6-Bay Top Control Surface and 88-Key Interactive Keybed Layout.
//!
//! Spec: `docs/specs/visual-style-and-panel.md`

use crate::audio::{AudioCommand, AudioHandle};
use crate::style::*;
use crate::widgets::*;
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, Ui};
use nord_core::engine::Stage4Engine;
use nord_core::midi::MidiEvent;
use nord_core::organ::{OrganModel, RotarySpeed, ScannerMode};

/// Complete Nord Stage 4 interactive front-panel GUI component.
pub struct Stage4PanelState {
    pub prog_id: String,
    pub prog_title: String,
    pub organ_a_active: bool,
    pub organ_b_active: bool,
    pub organ_a_level: f32,
    pub organ_b_level: f32,
    pub organ_drawbars: [u8; 9],
    pub organ_model: OrganModel,
    pub rotary_speed: RotarySpeed,
    pub rotary_drive: f32,
    pub scanner_mode: ScannerMode,
    pub piano_a_active: bool,
    pub piano_b_active: bool,
    pub piano_a_level: f32,
    pub piano_b_level: f32,
    pub synth_a_active: bool,
    pub synth_b_active: bool,
    pub synth_c_active: bool,
    pub synth_a_level: f32,
    pub synth_b_level: f32,
    pub synth_c_level: f32,
    pub synth_cutoff: f32,
    pub synth_resonance: f32,
    pub fx_drive: f32,
    pub fx_reverb: f32,
    pub active_keys: [bool; 128],
}

impl Default for Stage4PanelState {
    fn default() -> Self {
        Self {
            prog_id: "A:11".to_string(),
            prog_title: "Nord Stage 4".to_string(),
            organ_a_active: true,
            organ_b_active: false,
            organ_a_level: 0.85,
            organ_b_level: 0.0,
            organ_drawbars: [8, 8, 8, 0, 0, 0, 0, 0, 0],
            organ_model: OrganModel::B3,
            rotary_speed: RotarySpeed::Slow,
            rotary_drive: 0.25,
            scanner_mode: ScannerMode::C3,
            piano_a_active: true,
            piano_b_active: false,
            piano_a_level: 0.90,
            piano_b_level: 0.0,
            synth_a_active: false,
            synth_b_active: false,
            synth_c_active: false,
            synth_a_level: 0.70,
            synth_b_level: 0.0,
            synth_c_level: 0.0,
            synth_cutoff: 2400.0,
            synth_resonance: 0.3,
            fx_drive: 0.1,
            fx_reverb: 0.25,
            active_keys: [false; 128],
        }
    }
}

/// Renders the entire hardware front panel and synchronizes edits with the DSP audio handle.
pub fn render_stage4_panel(ui: &mut Ui, state: &mut Stage4PanelState, audio_handle: &AudioHandle) {
    ui.vertical(|ui| {
        // Main Top Panel (Crimson Chassis with 6 Horizontal Bays)
        let panel_size = vec2(1560.0, 310.0);
        let (panel_rect, _) = ui.allocate_exact_size(panel_size, Sense::hover());
        let painter = ui.painter();

        // Background Outer Crimson Chassis
        painter.rect_filled(panel_rect, 6.0, CHASSIS_RED);
        painter.rect_stroke(
            panel_rect,
            6.0,
            Stroke::new(2.0_f32, CHASSIS_DARK),
            StrokeKind::Inside,
        );

        // Render Bay 2 (Organ Section - Slate)
        let organ_rect = Rect::from_min_size(
            pos2(panel_rect.left() + 190.0, panel_rect.top() + 10.0),
            vec2(340.0, 290.0),
        );
        painter.rect_filled(organ_rect, 4.0, MODULE_SLATE);
        painter.rect_stroke(
            organ_rect,
            4.0,
            Stroke::new(1.5_f32, GROUP_BORDER),
            StrokeKind::Inside,
        );
        let organ_hdr = Rect::from_min_size(organ_rect.min, vec2(340.0, 18.0));
        painter.rect_filled(organ_hdr, 4.0, MODULE_HEADER_BAR);
        painter.text(
            organ_hdr.center(),
            Align2::CENTER_CENTER,
            "ORGAN SECTION",
            FontId::proportional(11.0),
            Color32::BLACK,
        );

        // Render Bay 3 (Piano Section - Slate)
        let piano_rect = Rect::from_min_size(
            pos2(panel_rect.left() + 540.0, panel_rect.top() + 10.0),
            vec2(310.0, 290.0),
        );
        painter.rect_filled(piano_rect, 4.0, MODULE_SLATE);
        painter.rect_stroke(
            piano_rect,
            4.0,
            Stroke::new(1.5_f32, GROUP_BORDER),
            StrokeKind::Inside,
        );
        let piano_hdr = Rect::from_min_size(piano_rect.min, vec2(310.0, 18.0));
        painter.rect_filled(piano_hdr, 4.0, MODULE_HEADER_BAR);
        painter.text(
            piano_hdr.center(),
            Align2::CENTER_CENTER,
            "PIANO SECTION",
            FontId::proportional(11.0),
            Color32::BLACK,
        );

        // Render Bay 4 (Program & Morph - Crimson)
        let prog_rect = Rect::from_min_size(
            pos2(panel_rect.left() + 860.0, panel_rect.top() + 10.0),
            vec2(210.0, 290.0),
        );
        painter.rect_filled(prog_rect, 4.0, Color32::from_rgb(120, 20, 24));
        painter.rect_stroke(
            prog_rect,
            4.0,
            Stroke::new(1.5_f32, GROUP_BORDER),
            StrokeKind::Inside,
        );

        // Render Bay 5 (Synth Section - Slate)
        let synth_rect = Rect::from_min_size(
            pos2(panel_rect.left() + 1080.0, panel_rect.top() + 10.0),
            vec2(280.0, 290.0),
        );
        painter.rect_filled(synth_rect, 4.0, MODULE_SLATE);
        painter.rect_stroke(
            synth_rect,
            4.0,
            Stroke::new(1.5_f32, GROUP_BORDER),
            StrokeKind::Inside,
        );
        let synth_hdr = Rect::from_min_size(synth_rect.min, vec2(280.0, 18.0));
        painter.rect_filled(synth_hdr, 4.0, MODULE_HEADER_BAR);
        painter.text(
            synth_hdr.center(),
            Align2::CENTER_CENTER,
            "SYNTH SECTION",
            FontId::proportional(11.0),
            Color32::BLACK,
        );

        // Render Bay 6 (Layer Effects - Slate)
        let fx_rect = Rect::from_min_size(
            pos2(panel_rect.left() + 1370.0, panel_rect.top() + 10.0),
            vec2(175.0, 290.0),
        );
        painter.rect_filled(fx_rect, 4.0, MODULE_SLATE);
        painter.rect_stroke(
            fx_rect,
            4.0,
            Stroke::new(1.5_f32, GROUP_BORDER),
            StrokeKind::Inside,
        );
        let fx_hdr = Rect::from_min_size(fx_rect.min, vec2(175.0, 18.0));
        painter.rect_filled(fx_hdr, 4.0, MODULE_HEADER_BAR);
        painter.text(
            fx_hdr.center(),
            Align2::CENTER_CENTER,
            "LAYER EFFECTS",
            FontId::proportional(11.0),
            Color32::BLACK,
        );

        // Interactive widgets overlay
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(panel_rect), |ui| {
            ui.horizontal(|ui| {
                ui.add_space(200.0);
                // Organ Faders & Drawbars
                if layer_fader_with_ladder(
                    ui,
                    &mut state.organ_a_level,
                    &mut state.organ_a_active,
                    "ORGAN A",
                )
                .changed()
                {
                    audio_handle.send_command(AudioCommand::SetOrganLevel(
                        if state.organ_a_active {
                            state.organ_a_level
                        } else {
                            0.0
                        },
                    ));
                }
                layer_fader_with_ladder(
                    ui,
                    &mut state.organ_b_level,
                    &mut state.organ_b_active,
                    "ORGAN B",
                );
                ui.add_space(8.0);
                let mut drawbar_changed = false;
                for i in 0..9 {
                    let is_white = matches!(i, 2 | 3 | 5 | 8);
                    let foot_labels = [
                        "16'", "5 1/3'", "8'", "4'", "2 2/3'", "2'", "1 3/5'", "1 1/3'", "1'",
                    ];
                    if drawbar(
                        ui,
                        &mut state.organ_drawbars[i],
                        "",
                        foot_labels[i],
                        is_white,
                    )
                    .changed()
                    {
                        drawbar_changed = true;
                    }
                }
                if drawbar_changed {
                    audio_handle.send_command(AudioCommand::SetDrawbars(state.organ_drawbars));
                }

                ui.add_space(25.0);
                // Piano Faders
                if layer_fader_with_ladder(
                    ui,
                    &mut state.piano_a_level,
                    &mut state.piano_a_active,
                    "PIANO A",
                )
                .changed()
                {
                    audio_handle.send_command(AudioCommand::SetPianoLevel(
                        if state.piano_a_active {
                            state.piano_a_level
                        } else {
                            0.0
                        },
                    ));
                }
                layer_fader_with_ladder(
                    ui,
                    &mut state.piano_b_level,
                    &mut state.piano_b_active,
                    "PIANO B",
                );

                ui.add_space(35.0);
                // Program OLED
                program_oled(
                    ui,
                    &state.prog_id,
                    &state.prog_title,
                    "B3 888000000",
                    "White Grand XL",
                    "SuperSaw Pad",
                );

                ui.add_space(35.0);
                // Synth Faders
                if layer_fader_with_ladder(
                    ui,
                    &mut state.synth_a_level,
                    &mut state.synth_a_active,
                    "SYNTH A",
                )
                .changed()
                {
                    audio_handle.send_command(AudioCommand::SetSynthLevel(
                        if state.synth_a_active {
                            state.synth_a_level
                        } else {
                            0.0
                        },
                    ));
                }
                layer_fader_with_ladder(
                    ui,
                    &mut state.synth_b_level,
                    &mut state.synth_b_active,
                    "SYNTH B",
                );
                layer_fader_with_ladder(
                    ui,
                    &mut state.synth_c_level,
                    &mut state.synth_c_active,
                    "SYNTH C",
                );

                ui.add_space(10.0);
                if nord_knob(ui, &mut state.synth_cutoff, 20.0, 15000.0, "FREQ").changed() {
                    audio_handle.send_command(AudioCommand::SetSynthCutoff(state.synth_cutoff));
                }
                if nord_knob(ui, &mut state.synth_resonance, 0.0, 1.0, "RES").changed() {
                    audio_handle
                        .send_command(AudioCommand::SetSynthResonance(state.synth_resonance));
                }

                ui.add_space(30.0);
                // FX Knobs
                nord_knob(ui, &mut state.fx_drive, 0.0, 1.0, "DRIVE");
                if nord_knob(ui, &mut state.fx_reverb, 0.0, 1.0, "REVERB").changed() {
                    audio_handle.send_command(AudioCommand::SetReverbMix(state.fx_reverb));
                }
            });
        });

        // 88-Key Interactive Keybed
        ui.add_space(10.0);
        render_keybed(ui, &mut state.active_keys, audio_handle);
    });
}

impl Stage4PanelState {
    /// Renders the front panel directly targeting a synchronous engine (e.g. for testing/offline rendering).
    pub fn ui(&mut self, ui: &mut Ui, engine: &mut Stage4Engine) {
        let (audio_handle, rx) = AudioHandle::new();
        render_stage4_panel(ui, self, &audio_handle);

        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                AudioCommand::Midi(ev) => engine.handle_midi(ev),
                AudioCommand::SetDrawbars(d) => engine.organ_a.drawbars = d,
                AudioCommand::SetOrganLevel(l) => engine.layer_levels[0].base = l,
                AudioCommand::SetPianoLevel(l) => engine.layer_levels[2].base = l,
                AudioCommand::SetSynthLevel(l) => engine.layer_levels[4].base = l,
                AudioCommand::SetSynthCutoff(co) => engine.synth_a.cutoff_hz = co,
                AudioCommand::SetSynthResonance(res) => engine.synth_a.resonance = res,
                AudioCommand::SetReverbMix(rev) => engine.fx_chains[0].reverb_mix = rev,
                AudioCommand::SetRotarySpeed(sp) => engine.organ_a.rotary_speed = sp,
                _ => {}
            }
        }
    }
}

/// 88-Key Piano Keyboard with interactive mouse clicking and Triple Split-Point visual markers.
fn render_keybed(ui: &mut Ui, active_keys: &mut [bool; 128], audio_handle: &AudioHandle) {
    let keybed_size = vec2(1560.0, 100.0);
    let (rect, response) = ui.allocate_exact_size(keybed_size, Sense::click_and_drag());
    let painter = ui.painter();

    // Keybed frame
    painter.rect_filled(rect, 2.0, Color32::from_rgb(15, 17, 20));

    // 88 keys: A0 (MIDI 21) .. C8 (MIDI 108)
    // 52 natural (white) keys
    let num_white = 52;
    let white_width = rect.width() / num_white as f32;

    // Check mouse interaction on keys
    if response.is_pointer_button_down_on() {
        if let Some(pos) = response.interact_pointer_pos() {
            // Determine clicked key
            let rel_x = pos.x - rect.left();
            let mut clicked_midi: Option<u8> = None;

            // First check black keys
            let mut white_idx = 0;
            for midi in 21u8..=108u8 {
                let is_black = matches!(midi % 12, 1 | 3 | 6 | 8 | 10);
                if is_black {
                    let kx = (white_idx as f32) * white_width - (white_width * 0.32);
                    let kw = white_width * 0.64;
                    let kh = (rect.height() - 6.0) * 0.62;
                    let rel_y = pos.y - rect.top();
                    if rel_x >= kx && rel_x <= (kx + kw) && rel_y <= kh {
                        clicked_midi = Some(midi);
                        break;
                    }
                } else {
                    white_idx += 1;
                }
            }

            // If not black key, find white key
            if clicked_midi.is_none() {
                let w_idx = (rel_x / white_width).floor() as usize;
                let mut curr_w = 0;
                for midi in 21u8..=108u8 {
                    let is_black = matches!(midi % 12, 1 | 3 | 6 | 8 | 10);
                    if !is_black {
                        if curr_w == w_idx {
                            clicked_midi = Some(midi);
                            break;
                        }
                        curr_w += 1;
                    }
                }
            }

            if let Some(midi) = clicked_midi {
                if !active_keys[midi as usize] {
                    active_keys[midi as usize] = true;
                    audio_handle.send_midi(MidiEvent::NoteOn {
                        channel: 0,
                        note: midi,
                        velocity: 100,
                    });
                }
            }
        }
    } else {
        // Release mouse pressed keys
        for midi in 21u8..=108u8 {
            if active_keys[midi as usize] {
                // If response lost click
                active_keys[midi as usize] = false;
                audio_handle.send_midi(MidiEvent::NoteOff {
                    channel: 0,
                    note: midi,
                    velocity: 0,
                });
            }
        }
    }

    let mut white_idx = 0;
    for midi in 21u8..=108u8 {
        let is_black = matches!(midi % 12, 1 | 3 | 6 | 8 | 10);
        if !is_black {
            let key_rect = Rect::from_min_size(
                pos2(
                    rect.left() + (white_idx as f32) * white_width,
                    rect.top() + 4.0,
                ),
                vec2(white_width - 1.0, rect.height() - 6.0),
            );
            let is_pressed = active_keys[midi as usize];
            let fill = if is_pressed {
                Color32::from_rgb(220, 240, 255)
            } else {
                Color32::WHITE
            };
            painter.rect_filled(key_rect, 2.0, fill);
            painter.rect_stroke(
                key_rect,
                2.0,
                Stroke::new(1.0_f32, Color32::from_gray(180)),
                StrokeKind::Inside,
            );
            white_idx += 1;
        }
    }

    // Black keys overlay
    white_idx = 0;
    for midi in 21u8..=108u8 {
        let is_black = matches!(midi % 12, 1 | 3 | 6 | 8 | 10);
        if is_black {
            let x = rect.left() + (white_idx as f32) * white_width - (white_width * 0.32);
            let key_rect = Rect::from_min_size(
                pos2(x, rect.top() + 4.0),
                vec2(white_width * 0.64, (rect.height() - 6.0) * 0.62),
            );
            let is_pressed = active_keys[midi as usize];
            let fill = if is_pressed {
                Color32::from_rgb(80, 100, 120)
            } else {
                Color32::from_rgb(25, 27, 30)
            };
            painter.rect_filled(key_rect, 1.5, fill);
            painter.rect_stroke(
                key_rect,
                1.5,
                Stroke::new(1.0_f32, Color32::BLACK),
                StrokeKind::Inside,
            );
        } else {
            white_idx += 1;
        }
    }

    // Split point red LEDs on keybed lip (F3/F#3 = MIDI 53, C5/C#5 = MIDI 72)
    let split_led1 = pos2(rect.left() + 20.0 * white_width, rect.top() + 2.0);
    let split_led2 = pos2(rect.left() + 32.0 * white_width, rect.top() + 2.0);
    painter.circle_filled(split_led1, 2.5, LED_RED);
    painter.circle_filled(split_led2, 2.5, LED_RED);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_panel_state_default() {
        let state = Stage4PanelState::default();
        assert_eq!(state.prog_id, "A:11");
        assert_eq!(state.organ_drawbars[0], 8);
        assert_eq!(state.synth_cutoff, 2400.0);
    }

    #[test]
    fn test_panel_state_audio_sync() {
        let mut state = Stage4PanelState::default();
        let (audio_handle, rx) = AudioHandle::new();
        state.organ_drawbars[0] = 5;
        audio_handle.send_command(AudioCommand::SetDrawbars(state.organ_drawbars));

        match rx.try_recv() {
            Ok(AudioCommand::SetDrawbars(d)) => {
                assert_eq!(d[0], 5);
            }
            _ => panic!("Expected SetDrawbars"),
        }
    }
}
